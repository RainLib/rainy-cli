use crate::config::{self, LockedCapability};
use crate::error::{RainyError, RainyResult};
use crate::patch::{self, ChangeSet};
use crate::registry::{
    ActionSpec, CapabilityDefinition, CapabilityInput, CapabilityProvider, RegistryClient,
};
use chrono::Utc;
use handlebars::Handlebars;
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_yaml::{Mapping, Value};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct AddCapabilityRequest {
    pub capability_id: String,
    pub provider: Option<String>,
    pub force: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPlan {
    pub id: String,
    pub capability: String,
    pub provider: String,
    pub actions: Vec<PlannedAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannedAction {
    pub id: String,
    pub uses: String,
    pub input: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct CapabilityOutcome {
    pub plan: ExecutionPlan,
    pub changes: ChangeSet,
}

#[derive(Debug)]
struct ActionContext<'a> {
    workspace: &'a Path,
    config: &'a config::ProjectConfig,
    capability: &'a CapabilityDefinition,
    variables: serde_json::Value,
    force: bool,
    virtual_files: RefCell<BTreeMap<String, Option<String>>>,
}

impl ActionContext<'_> {
    fn read_file(&self, relative: &str) -> RainyResult<Option<String>> {
        if let Some(value) = self.virtual_files.borrow().get(relative) {
            return Ok(value.clone());
        }
        let path = self.workspace.join(relative);
        if path.is_file() {
            return Ok(Some(std::fs::read_to_string(path)?));
        }
        Ok(None)
    }

    fn record_changes(&self, changes: &ChangeSet) {
        let mut virtual_files = self.virtual_files.borrow_mut();
        for change in &changes.changes {
            if change.noop {
                continue;
            }
            virtual_files.insert(change.path.clone(), change.after.clone());
        }
    }
}

pub fn plan_add_capability(
    workspace: &Path,
    request: AddCapabilityRequest,
) -> RainyResult<CapabilityOutcome> {
    let config = config::load_config(workspace)?;
    let lock = config::load_lock(workspace)?;
    let registry = RegistryClient::load(workspace)?;
    let capability = registry.get_capability(&request.capability_id)?;
    ensure_dependencies_installed(&lock, &capability)?;
    let provider = resolve_provider(&capability, request.provider.as_deref())?;
    let variables = variables_for(&config, &capability.inputs, &provider);
    let ctx = ActionContext {
        workspace,
        config: &config,
        capability: &capability,
        variables,
        force: request.force,
        virtual_files: RefCell::new(BTreeMap::new()),
    };

    let mut actions = Vec::new();
    let mut changes = ChangeSet::new();

    let provider_actions = capability
        .providers
        .iter()
        .find(|candidate| candidate.id == provider)
        .map(|candidate| candidate.actions.install.as_slice())
        .unwrap_or_default();
    for action in capability
        .actions
        .install
        .iter()
        .chain(provider_actions.iter())
    {
        let rendered = render_yaml_value(&ctx, &action.with_value)?;
        let input = serde_json::to_value(&rendered)?;
        actions.push(PlannedAction {
            id: action.id.clone(),
            uses: action.uses.clone(),
            input,
        });
        changes.extend(plan_action(&ctx, action, rendered)?);
    }

    append_lock_change(
        workspace,
        &config,
        lock,
        &capability,
        &provider,
        &actions,
        &mut changes,
    )?;

    Ok(CapabilityOutcome {
        plan: ExecutionPlan {
            id: format!("add-{}", capability.id),
            capability: capability.id,
            provider,
            actions,
        },
        changes,
    })
}

pub fn plan_from_execution_plan(
    workspace: &Path,
    plan: ExecutionPlan,
    force: bool,
) -> RainyResult<CapabilityOutcome> {
    if plan.id.starts_with("remove-") {
        return plan_remove_capability(workspace, &plan.capability);
    }

    let config = config::load_config(workspace)?;
    let lock = config::load_lock(workspace)?;
    let registry = RegistryClient::load(workspace)?;
    let capability = registry.get_capability(&plan.capability)?;
    ensure_dependencies_installed(&lock, &capability)?;
    let requested_provider = if plan.provider == "default" {
        None
    } else {
        Some(plan.provider.as_str())
    };
    let provider = resolve_provider(&capability, requested_provider)?;
    let variables = variables_for(&config, &capability.inputs, &provider);
    let ctx = ActionContext {
        workspace,
        config: &config,
        capability: &capability,
        variables,
        force,
        virtual_files: RefCell::new(BTreeMap::new()),
    };
    let mut changes = ChangeSet::new();
    for action in &plan.actions {
        let input = serde_yaml::to_value(&action.input)?;
        let spec = ActionSpec {
            id: action.id.clone(),
            uses: action.uses.clone(),
            with_value: input.clone(),
        };
        changes.extend(plan_action(&ctx, &spec, input)?);
    }
    append_lock_change(
        workspace,
        &config,
        lock,
        &capability,
        &provider,
        &plan.actions,
        &mut changes,
    )?;

    Ok(CapabilityOutcome { plan, changes })
}

pub fn plan_upgrade_capability(
    workspace: &Path,
    capability_id: &str,
    force: bool,
) -> RainyResult<CapabilityOutcome> {
    let lock = config::load_lock(workspace)?;
    let installed = lock.capabilities.get(capability_id).ok_or_else(|| {
        RainyError::plan(
            "CAPABILITY_NOT_INSTALLED",
            format!("capability is not installed: {capability_id}"),
        )
    })?;
    plan_add_capability(
        workspace,
        AddCapabilityRequest {
            capability_id: capability_id.to_string(),
            provider: installed.provider.clone(),
            force,
        },
    )
}

pub fn plan_switch_provider(
    workspace: &Path,
    capability_id: &str,
    provider: &str,
    force: bool,
) -> RainyResult<CapabilityOutcome> {
    let config = config::load_config(workspace)?;
    let lock = config::load_lock(workspace)?;
    if lock.lockfile_version < 2 {
        return Err(RainyError::plan(
            "CAPABILITY_OWNERSHIP_REQUIRED",
            "provider switching requires capability.lock v2 ownership; upgrade or reinstall the capability before switching",
        ));
    }
    let installed = lock.capabilities.get(capability_id).ok_or_else(|| {
        RainyError::plan(
            "CAPABILITY_NOT_INSTALLED",
            format!("capability is not installed: {capability_id}"),
        )
    })?;
    let old_provider = installed.provider.as_deref().ok_or_else(|| {
        RainyError::plan(
            "CAPABILITY_PROVIDER_NOT_LOCKED",
            format!("installed capability {capability_id} has no locked provider"),
        )
    })?;
    if old_provider == provider {
        return plan_add_capability(
            workspace,
            AddCapabilityRequest {
                capability_id: capability_id.to_string(),
                provider: Some(provider.to_string()),
                force,
            },
        );
    }

    let registry = RegistryClient::load(workspace)?;
    let capability = registry.get_capability(capability_id)?;
    let old = capability
        .providers
        .iter()
        .find(|candidate| candidate.id == old_provider)
        .ok_or_else(|| {
            RainyError::plan(
                "CAPABILITY_PROVIDER_INVALID",
                format!("locked provider {old_provider} is no longer declared by {capability_id}"),
            )
        })?;
    let new = capability
        .providers
        .iter()
        .find(|candidate| candidate.id == provider)
        .ok_or_else(|| {
            RainyError::plan(
                "CAPABILITY_PROVIDER_INVALID",
                format!("provider {provider} is not valid for {capability_id}"),
            )
        })?;
    verify_provider_switch_ownership(workspace, installed, old, old_provider)?;
    let new_action_ids = new
        .actions
        .install
        .iter()
        .map(|action| action.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let variables = variables_for(&config, &capability.inputs, old_provider);
    let ctx = ActionContext {
        workspace,
        config: &config,
        capability: &capability,
        variables,
        force,
        virtual_files: RefCell::new(BTreeMap::new()),
    };
    let mut removal_plan = Vec::new();
    let mut removal_changes = ChangeSet::new();
    for action in old
        .actions
        .install
        .iter()
        .filter(|action| !new_action_ids.contains(action.id.as_str()))
    {
        if !matches!(
            action.uses.as_str(),
            "jvm.addDependency" | "jvm.addBom" | "jvm.removeDependency"
        ) {
            return Err(RainyError::plan(
                "CAPABILITY_OWNERSHIP_REQUIRED",
                format!(
                    "provider action {} ({}) cannot be removed safely; add a matching action ID to every provider",
                    action.id, action.uses
                ),
            ));
        }
        let mut rendered = render_yaml_value(&ctx, &action.with_value)?;
        let mapping = rendered.as_mapping_mut().ok_or_else(|| {
            RainyError::plan(
                "CAPABILITY_PROVIDER_ACTION_INVALID",
                format!("provider action {} input must be an object", action.id),
            )
        })?;
        mapping.insert(
            Value::String("managedActionId".to_string()),
            Value::String(action.id.clone()),
        );
        let removal = ActionSpec {
            id: format!("remove-{}", action.id),
            uses: "jvm.removeDependency".to_string(),
            with_value: rendered.clone(),
        };
        let input = serde_json::to_value(&rendered)?;
        removal_plan.push(PlannedAction {
            id: removal.id.clone(),
            uses: removal.uses.clone(),
            input,
        });
        removal_changes.extend(plan_action(&ctx, &removal, rendered)?);
    }

    let mut outcome = plan_add_capability(
        workspace,
        AddCapabilityRequest {
            capability_id: capability_id.to_string(),
            provider: Some(provider.to_string()),
            force,
        },
    )?;
    removal_plan.append(&mut outcome.plan.actions);
    outcome.plan.actions = removal_plan;
    removal_changes.extend(outcome.changes);
    outcome.changes = removal_changes;
    outcome.plan.id = format!("switch-provider-{capability_id}-{provider}");
    Ok(outcome)
}

pub fn plan_remove_capability(
    workspace: &Path,
    capability_id: &str,
) -> RainyResult<CapabilityOutcome> {
    let mut lock = config::load_lock(workspace)?;
    ensure_no_installed_dependents(workspace, &lock, capability_id)?;
    let Some(removed) = lock.capabilities.remove(capability_id) else {
        return Ok(CapabilityOutcome {
            plan: ExecutionPlan {
                id: format!("remove-{capability_id}"),
                capability: capability_id.to_string(),
                provider: "installed".to_string(),
                actions: Vec::new(),
            },
            changes: ChangeSet::new(),
        });
    };

    let shared_artifacts = lock
        .capabilities
        .values()
        .flat_map(|capability| capability.artifacts.iter().cloned())
        .collect::<std::collections::BTreeSet<_>>();
    let mut changes = ChangeSet::new();

    if lock.lockfile_version >= 2 {
        let mut managed_text_files = BTreeMap::<String, String>::new();
        let mut managed_yaml_files = BTreeMap::<String, Value>::new();
        for managed in &removed.managed_changes {
            match managed.kind.as_str() {
                "jvm-managed-block" => {
                    let before = match managed_text_files.get(&managed.path) {
                        Some(value) => value.clone(),
                        None => read_managed_file(workspace, &managed.path)?,
                    };
                    if !before.contains(&format!("{}:begin", managed.marker)) {
                        return Err(RainyError::plan(
                            "CAPABILITY_OWNERSHIP_REQUIRED",
                            format!(
                                "managed marker {} is missing from {}; local drift must be resolved first",
                                managed.marker, managed.path
                            ),
                        ));
                    }
                    verify_jvm_managed_value(&before, managed)?;
                    let after = if managed.path.ends_with("pom.xml") {
                        remove_managed_block(&before, &managed.marker, "<!--", "-->")?
                    } else {
                        remove_managed_block(&before, &managed.marker, "//", "")?
                    };
                    managed_text_files.insert(managed.path.clone(), after);
                }
                "yaml-semantic-path" => {
                    let mut document = match managed_yaml_files.remove(&managed.path) {
                        Some(value) => value,
                        None => serde_yaml::from_str::<Value>(&read_managed_file(
                            workspace,
                            &managed.path,
                        )?)?,
                    };
                    verify_yaml_managed_value(&document, managed)?;
                    remove_yaml_pointer(&mut document, &managed.marker)?;
                    managed_yaml_files.insert(managed.path.clone(), document);
                }
                other => {
                    return Err(RainyError::plan(
                        "CAPABILITY_OWNERSHIP_REQUIRED",
                        format!("unsupported managed ownership kind: {other}"),
                    ));
                }
            }
        }
        for (path, after) in managed_text_files {
            changes.push(patch::change_for_file(
                workspace,
                &path,
                after,
                format!("remove managed JVM changes for {capability_id}"),
            )?);
        }
        for (path, document) in managed_yaml_files {
            changes.push(patch::change_for_file(
                workspace,
                &path,
                serde_yaml::to_string(&document)?,
                format!("remove managed YAML configuration for {capability_id}"),
            )?);
        }
    }

    for artifact in removed.artifacts {
        if shared_artifacts.contains(&artifact) {
            continue;
        }
        if artifact.ends_with("pom.xml")
            || artifact.ends_with("build.gradle")
            || artifact.ends_with("build.gradle.kts")
        {
            if lock.lockfile_version < 2 {
                return Err(RainyError::plan(
                    "CAPABILITY_OWNERSHIP_REQUIRED",
                    format!(
                        "lock v1 cannot prove ownership inside {artifact}; reinstall to upgrade the lock before removal"
                    ),
                ));
            }
            continue;
        }
        let path = workspace.join(&artifact);
        if path.is_file() {
            changes.push(patch::delete_file(workspace, artifact)?);
        } else if path.is_dir() {
            let mut files = WalkDir::new(&path)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_file())
                .map(|entry| {
                    entry
                        .path()
                        .strip_prefix(workspace)
                        .unwrap_or(entry.path())
                        .to_string_lossy()
                        .replace('\\', "/")
                })
                .collect::<Vec<_>>();
            files.sort();
            files.reverse();
            for file in files {
                changes.push(patch::delete_file(workspace, file)?);
            }
        }
    }

    lock.skills
        .retain(|skill| !skill.starts_with(&format!("{capability_id}@")));
    let lock_content = config::save_lock_content(&lock)?;
    changes.push(patch::change_for_file(
        workspace,
        "capability.lock",
        lock_content,
        format!("remove capability {capability_id}"),
    )?);

    Ok(CapabilityOutcome {
        plan: ExecutionPlan {
            id: format!("remove-{capability_id}"),
            capability: capability_id.to_string(),
            provider: "installed".to_string(),
            actions: Vec::new(),
        },
        changes,
    })
}

fn ensure_dependencies_installed(
    lock: &config::CapabilityLock,
    capability: &CapabilityDefinition,
) -> RainyResult<()> {
    let missing = capability
        .depends_on
        .iter()
        .filter(|dependency| !lock.capabilities.contains_key(*dependency))
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(RainyError::plan(
            "CAPABILITY_DEPENDENCY_MISSING",
            format!(
                "capability {} requires installed dependencies: {}",
                capability.id,
                missing.join(", ")
            ),
        ));
    }
    Ok(())
}

fn ensure_no_installed_dependents(
    workspace: &Path,
    lock: &config::CapabilityLock,
    capability_id: &str,
) -> RainyResult<()> {
    let Ok(registry) = RegistryClient::load(workspace) else {
        return Ok(());
    };
    let mut dependents = Vec::new();
    for id in lock.capabilities.keys() {
        if id == capability_id {
            continue;
        }
        let Ok(capability) = registry.get_capability(id) else {
            continue;
        };
        if capability
            .depends_on
            .iter()
            .any(|dependency| dependency == capability_id)
        {
            dependents.push(id.clone());
        }
    }
    if !dependents.is_empty() {
        return Err(RainyError::plan(
            "CAPABILITY_DEPENDENT_INSTALLED",
            format!(
                "cannot remove {capability_id}; installed capabilities depend on it: {}",
                dependents.join(", ")
            ),
        ));
    }
    Ok(())
}

fn append_lock_change(
    workspace: &Path,
    project_config: &config::ProjectConfig,
    mut lock: config::CapabilityLock,
    capability: &CapabilityDefinition,
    provider: &str,
    actions: &[PlannedAction],
    changes: &mut ChangeSet,
) -> RainyResult<()> {
    lock.lockfile_version = 2;
    let artifacts = changes.changed_files();
    let managed_changes = managed_changes_for_actions(
        workspace,
        project_config,
        &capability.id,
        provider,
        actions,
        changes,
    )?;
    let source = capability.pack_root.to_string_lossy().to_string();
    let digest = crate::registry::pack_digest(&capability.pack_root)?;
    let next_capability = if let Some(existing) = lock.capabilities.get(&capability.id) {
        let mut existing = existing.clone();
        if let Some(locked_digest) = &existing.digest
            && locked_digest != &digest
        {
            return Err(RainyError::registry(
                "PACK_DIGEST_MISMATCH",
                format!(
                    "pack {} changed since it was locked; expected {}, got {}",
                    capability.pack_name, locked_digest, digest
                ),
            ));
        }
        existing.version = capability.version.clone();
        existing.provider = Some(provider.to_string());
        existing.source = Some(source.clone());
        existing.digest = Some(digest.clone());
        existing.managed_changes = managed_changes;
        for artifact in artifacts {
            if !existing.artifacts.contains(&artifact) {
                existing.artifacts.push(artifact);
            }
        }
        existing
    } else {
        LockedCapability {
            version: capability.version.clone(),
            provider: Some(provider.to_string()),
            pack: format!("{}@{}", capability.pack_name, capability.version),
            source: Some(source),
            digest: Some(digest),
            installed_at: Utc::now(),
            artifacts,
            managed_changes,
        }
    };
    lock.capabilities
        .insert(capability.id.clone(), next_capability);
    let skill = format!("{}@{}", capability.id, capability.version);
    if !lock.skills.contains(&skill) {
        lock.skills.push(skill);
    }
    let lock_content = config::save_lock_content(&lock)?;
    changes.push(patch::change_for_file(
        workspace,
        "capability.lock",
        lock_content,
        format!("record capability {}", capability.id),
    )?);
    Ok(())
}

fn managed_changes_for_actions(
    workspace: &Path,
    project_config: &config::ProjectConfig,
    capability_id: &str,
    provider: &str,
    actions: &[PlannedAction],
    changes: &ChangeSet,
) -> RainyResult<Vec<config::ManagedChange>> {
    let mut managed = Vec::new();
    for action in actions {
        if action.uses == "yaml.merge" {
            let file = action
                .input
                .get("file")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    RainyError::action(
                        "YAML_FILE_REQUIRED",
                        format!("action {} requires file", action.id),
                    )
                })?;
            validate_managed_relative_path(file)?;
            let patch = action.input.get("patch").ok_or_else(|| {
                RainyError::action(
                    "YAML_PATCH_REQUIRED",
                    format!("action {} requires patch", action.id),
                )
            })?;
            let mut leaves = Vec::new();
            collect_json_leaves(patch, String::new(), &mut leaves)?;
            for (pointer, value) in leaves {
                managed.push(config::ManagedChange {
                    action_id: action.id.clone(),
                    kind: "yaml-semantic-path".to_string(),
                    path: file.to_string(),
                    marker: pointer,
                    expected_value_sha256: Some(value_sha256(value)?),
                    provider: Some(provider.to_string()),
                });
            }
            continue;
        }
        if !matches!(action.uses.as_str(), "jvm.addDependency" | "jvm.addBom") {
            continue;
        }
        let target_name = action
            .input
            .get("target")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                RainyError::action(
                    "JVM_BUILD_TARGET_REQUIRED",
                    format!("action {} requires target", action.id),
                )
            })?;
        let target = project_config
            .build
            .targets
            .get(target_name)
            .ok_or_else(|| {
                RainyError::action(
                    "JVM_BUILD_TARGET_NOT_FOUND",
                    format!("build target not found: {target_name}"),
                )
            })?;
        let (system, root) = crate::build::resolve_target_definition(workspace, target)?;
        let filename = match system {
            config::BuildSystem::Maven => "pom.xml",
            config::BuildSystem::Gradle => "build.gradle",
            config::BuildSystem::Auto => unreachable!("auto build system must be resolved"),
        };
        let path = root
            .join(filename)
            .strip_prefix(workspace)
            .map_err(|_| {
                RainyError::action("JVM_BUILD_FILE_INVALID", "build file is outside workspace")
            })?
            .to_string_lossy()
            .replace('\\', "/");
        let marker = managed_marker(capability_id, &action.id);
        let final_content = changes
            .changes
            .iter()
            .rev()
            .find(|change| change.path == path)
            .and_then(|change| change.after.as_deref())
            .ok_or_else(|| {
                RainyError::plan(
                    "CAPABILITY_OWNERSHIP_REQUIRED",
                    format!("cannot prove the planned managed block in {path}"),
                )
            })?;
        let block = extract_managed_block(final_content, &marker)?;
        managed.push(config::ManagedChange {
            action_id: action.id.clone(),
            kind: "jvm-managed-block".to_string(),
            path,
            marker,
            expected_value_sha256: Some(bytes_sha256(block.as_bytes())),
            provider: Some(provider.to_string()),
        });
    }
    managed.sort_by(|left, right| left.action_id.cmp(&right.action_id));
    Ok(managed)
}

fn verify_provider_switch_ownership(
    workspace: &Path,
    installed: &LockedCapability,
    old_provider: &CapabilityProvider,
    old_provider_id: &str,
) -> RainyResult<()> {
    let expected_action_ids = old_provider
        .actions
        .install
        .iter()
        .filter(|action| {
            matches!(
                action.uses.as_str(),
                "jvm.addDependency" | "jvm.addBom" | "yaml.merge"
            )
        })
        .map(|action| action.id.as_str())
        .collect::<BTreeSet<_>>();
    for action_id in expected_action_ids {
        if !installed
            .managed_changes
            .iter()
            .any(|managed| managed.action_id == action_id)
        {
            return Err(RainyError::plan(
                "CAPABILITY_OWNERSHIP_REQUIRED",
                format!(
                    "provider action {action_id} has no semantic ownership record; reinstall the current provider before switching"
                ),
            ));
        }
    }
    for managed in &installed.managed_changes {
        if managed.provider.as_deref() != Some(old_provider_id) {
            return Err(RainyError::plan(
                "CAPABILITY_OWNERSHIP_REQUIRED",
                format!(
                    "managed change {} is not owned by the locked provider {old_provider_id}",
                    managed.action_id
                ),
            ));
        }
        let content = read_managed_file(workspace, &managed.path)?;
        match managed.kind.as_str() {
            "jvm-managed-block" => verify_jvm_managed_value(&content, managed)?,
            "yaml-semantic-path" => {
                let document = serde_yaml::from_str::<Value>(&content)?;
                verify_yaml_managed_value(&document, managed)?;
            }
            other => {
                return Err(RainyError::plan(
                    "CAPABILITY_OWNERSHIP_REQUIRED",
                    format!("unsupported managed ownership kind: {other}"),
                ));
            }
        }
    }
    Ok(())
}

fn collect_json_leaves<'a>(
    value: &'a serde_json::Value,
    pointer: String,
    leaves: &mut Vec<(String, &'a serde_json::Value)>,
) -> RainyResult<()> {
    match value {
        serde_json::Value::Object(mapping) => {
            for (key, value) in mapping {
                let escaped = key.replace('~', "~0").replace('/', "~1");
                collect_json_leaves(value, format!("{pointer}/{escaped}"), leaves)?;
            }
        }
        _ if pointer.is_empty() => {
            return Err(RainyError::action(
                "YAML_PATCH_INVALID",
                "managed YAML patch must be an object",
            ));
        }
        _ => leaves.push((pointer, value)),
    }
    Ok(())
}

fn value_sha256(value: &serde_json::Value) -> RainyResult<String> {
    Ok(bytes_sha256(&serde_json::to_vec(value)?))
}

fn bytes_sha256(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

fn extract_managed_block<'a>(content: &'a str, marker: &str) -> RainyResult<&'a str> {
    let begin = content.find(&format!("{marker}:begin")).ok_or_else(|| {
        RainyError::plan(
            "CAPABILITY_OWNERSHIP_REQUIRED",
            format!("managed marker {marker}:begin is missing"),
        )
    })?;
    let start = content[..begin].rfind('\n').map_or(0, |index| index + 1);
    let end_marker = format!("{marker}:end");
    let end_start = content[begin..]
        .find(&end_marker)
        .map(|index| begin + index)
        .ok_or_else(|| {
            RainyError::plan(
                "CAPABILITY_OWNERSHIP_REQUIRED",
                format!("managed marker {marker}:end is missing"),
            )
        })?;
    let end = content[end_start..]
        .find('\n')
        .map_or(content.len(), |index| end_start + index + 1);
    Ok(&content[start..end])
}

fn verify_jvm_managed_value(content: &str, managed: &config::ManagedChange) -> RainyResult<()> {
    let expected = managed.expected_value_sha256.as_deref().ok_or_else(|| {
        RainyError::plan(
            "CAPABILITY_OWNERSHIP_REQUIRED",
            format!(
                "managed JVM block {} has no value digest; reinstall before changing it",
                managed.marker
            ),
        )
    })?;
    let actual = bytes_sha256(extract_managed_block(content, &managed.marker)?.as_bytes());
    if actual != expected {
        return Err(RainyError::plan(
            "CAPABILITY_OWNERSHIP_REQUIRED",
            format!(
                "managed JVM block {} in {} has local drift",
                managed.marker, managed.path
            ),
        ));
    }
    Ok(())
}

fn verify_yaml_managed_value(document: &Value, managed: &config::ManagedChange) -> RainyResult<()> {
    let expected = managed.expected_value_sha256.as_deref().ok_or_else(|| {
        RainyError::plan(
            "CAPABILITY_OWNERSHIP_REQUIRED",
            format!(
                "managed YAML path {} has no value digest; reinstall before changing it",
                managed.marker
            ),
        )
    })?;
    let actual_value = yaml_value_at_pointer(document, &managed.marker).ok_or_else(|| {
        RainyError::plan(
            "CAPABILITY_OWNERSHIP_REQUIRED",
            format!(
                "managed YAML path {} is missing from {}",
                managed.marker, managed.path
            ),
        )
    })?;
    let actual_json = serde_json::to_value(actual_value)?;
    if value_sha256(&actual_json)? != expected {
        return Err(RainyError::plan(
            "CAPABILITY_OWNERSHIP_REQUIRED",
            format!(
                "managed YAML path {} in {} has local drift",
                managed.marker, managed.path
            ),
        ));
    }
    Ok(())
}

fn yaml_value_at_pointer<'a>(document: &'a Value, pointer: &str) -> Option<&'a Value> {
    let mut current = document;
    for segment in yaml_pointer_segments(pointer).ok()? {
        current = current.as_mapping()?.get(Value::String(segment))?;
    }
    Some(current)
}

fn remove_yaml_pointer(document: &mut Value, pointer: &str) -> RainyResult<()> {
    let segments = yaml_pointer_segments(pointer)?;
    if segments.is_empty() || !remove_yaml_segments(document, &segments) {
        return Err(RainyError::plan(
            "CAPABILITY_OWNERSHIP_REQUIRED",
            format!("managed YAML path is missing: {pointer}"),
        ));
    }
    Ok(())
}

fn remove_yaml_segments(value: &mut Value, segments: &[String]) -> bool {
    let Some(mapping) = value.as_mapping_mut() else {
        return false;
    };
    let key = Value::String(segments[0].clone());
    if segments.len() == 1 {
        return mapping.remove(&key).is_some();
    }
    let Some(child) = mapping.get_mut(&key) else {
        return false;
    };
    let removed = remove_yaml_segments(child, &segments[1..]);
    if removed && child.as_mapping().is_some_and(Mapping::is_empty) {
        mapping.remove(&key);
    }
    removed
}

fn yaml_pointer_segments(pointer: &str) -> RainyResult<Vec<String>> {
    if !pointer.starts_with('/') {
        return Err(RainyError::plan(
            "CAPABILITY_OWNERSHIP_REQUIRED",
            format!("managed YAML pointer is invalid: {pointer}"),
        ));
    }
    Ok(pointer[1..]
        .split('/')
        .map(|segment| segment.replace("~1", "/").replace("~0", "~"))
        .collect())
}

fn validate_managed_relative_path(path: &str) -> RainyResult<()> {
    let relative = Path::new(path);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(RainyError::plan(
            "CAPABILITY_OWNERSHIP_REQUIRED",
            format!("managed path is unsafe: {path}"),
        ));
    }
    Ok(())
}

fn read_managed_file(workspace: &Path, path: &str) -> RainyResult<String> {
    validate_managed_relative_path(path)?;
    let workspace = workspace.canonicalize()?;
    let candidate = workspace.join(path);
    let canonical = candidate.canonicalize().map_err(|_| {
        RainyError::plan(
            "CAPABILITY_OWNERSHIP_REQUIRED",
            format!("managed file is missing: {path}"),
        )
    })?;
    if !canonical.starts_with(&workspace) || !canonical.is_file() {
        return Err(RainyError::plan(
            "CAPABILITY_OWNERSHIP_REQUIRED",
            format!("managed file is unsafe: {path}"),
        ));
    }
    Ok(std::fs::read_to_string(canonical)?)
}

fn resolve_provider(
    capability: &CapabilityDefinition,
    provider: Option<&str>,
) -> RainyResult<String> {
    if capability.providers.is_empty() {
        if let Some(provider) = provider {
            return Err(RainyError::plan(
                "CAPABILITY_PROVIDER_UNSUPPORTED",
                format!(
                    "capability {} does not support provider {provider}",
                    capability.id
                ),
            ));
        }
        return Ok("default".to_string());
    }
    if let Some(provider) = provider {
        if capability.providers.iter().any(|item| item.id == provider) {
            return Ok(provider.to_string());
        }
        return Err(RainyError::plan(
            "CAPABILITY_PROVIDER_INVALID",
            format!("provider {provider} is not valid for {}", capability.id),
        ));
    }
    let default_providers: Vec<_> = capability
        .providers
        .iter()
        .filter(|provider| provider.default)
        .collect();
    match default_providers.as_slice() {
        [default_provider] => Ok(default_provider.id.clone()),
        [] if capability.providers.len() == 1 => Ok(capability.providers[0].id.clone()),
        [] => Err(RainyError::plan(
            "CAPABILITY_PROVIDER_REQUIRED",
            format!(
                "capability {} has multiple providers and no default; pass --provider",
                capability.id
            ),
        )),
        _ => Err(RainyError::plan(
            "CAPABILITY_PROVIDER_DEFAULT_CONFLICT",
            format!(
                "capability {} declares multiple default providers",
                capability.id
            ),
        )),
    }
}

fn variables_for(
    config: &config::ProjectConfig,
    inputs: &std::collections::BTreeMap<String, CapabilityInput>,
    provider: &str,
) -> serde_json::Value {
    let mut input_values = serde_json::Map::new();
    for (key, input) in inputs {
        if let Some(value) = &input.default {
            input_values.insert(key.clone(), json!(yaml_scalar_to_string(value)));
        }
    }
    json!({
        "paths": {
            "backend": config.paths.backend,
            "frontend": config.paths.frontend,
            "generated": config.paths.generated,
            "evidence": config.paths.evidence
        },
        "package": {
            "java": config.package.java,
            "npmScope": config.package.npm_scope
        },
        "packagePath": config::package_path(config),
        "provider": {
            "id": provider
        },
        "inputs": input_values
    })
}

fn plan_action(
    ctx: &ActionContext<'_>,
    action: &ActionSpec,
    input: Value,
) -> RainyResult<ChangeSet> {
    let changes = match action.uses.as_str() {
        "jvm.addDependency" => jvm_add_dependency(ctx, action, input),
        "jvm.addBom" => jvm_add_bom(ctx, action, input),
        "jvm.removeDependency" => jvm_remove_dependency(ctx, action, input),
        "maven.addDependency" => maven_add_dependency(ctx, input),
        "maven.addBom" => maven_add_bom(ctx, input),
        "yaml.merge" => yaml_merge(ctx, input),
        "json.merge" => json_merge(ctx, input, false),
        "jsonc.merge" => json_merge(ctx, input, true),
        "toml.merge" => toml_merge(ctx, input),
        "template.render" => template_render(ctx, input),
        "file.create" => file_create(ctx, input),
        "file.append" => file_append(ctx, input),
        "dockerCompose.addService" => docker_compose_add_service(ctx, input),
        "packageJson.addDependency" => package_json_add_dependency(ctx, input),
        "packageJson.addScript" => package_json_add_script(ctx, input),
        "githubActions.addWorkflow" => template_render(ctx, input),
        "devcontainer.merge" => json_merge(ctx, input, true),
        "helm.renderChart" => template_render(ctx, input),
        "capabilityLock.update" => Ok(ChangeSet::new()),
        "agentsMd.generate" => template_render(ctx, input),
        "command.runValidation" => Ok(ChangeSet::new()),
        other => Err(RainyError::action(
            "ACTION_UNKNOWN",
            format!("unknown action: {other}"),
        )),
    }?;
    ctx.record_changes(&changes);
    Ok(changes)
}

fn jvm_add_dependency(
    ctx: &ActionContext<'_>,
    action: &ActionSpec,
    input: Value,
) -> RainyResult<ChangeSet> {
    let target = required_string(&input, "target")?;
    let group_id = required_coordinate(&input, "groupId")?;
    let artifact_id = required_coordinate(&input, "artifactId")?;
    let version = optional_string(&input, "version");
    let (system, file, before) = jvm_build_file(ctx, &target)?;
    let marker = managed_marker(&ctx.capability.id, &action.id);
    let after = match system {
        config::BuildSystem::Maven => {
            let scope = optional_string(&input, "scope");
            let body = format!(
                "        <dependency>\n            <groupId>{}</groupId>\n            <artifactId>{}</artifactId>\n{}{}        </dependency>",
                xml_escape(&group_id),
                xml_escape(&artifact_id),
                version
                    .as_deref()
                    .map(|value| format!("            <version>{}</version>\n", xml_escape(value)))
                    .unwrap_or_default(),
                scope
                    .as_deref()
                    .map(|value| format!("            <scope>{}</scope>\n", xml_escape(value)))
                    .unwrap_or_default()
            );
            maven_managed_dependency(&before, &marker, &body, false)?
        }
        config::BuildSystem::Gradle => {
            let configuration = optional_string(&input, "configuration")
                .unwrap_or_else(|| "implementation".to_string());
            validate_gradle_identifier(&configuration)?;
            let coordinate = version
                .as_deref()
                .map(|version| format!("{group_id}:{artifact_id}:{version}"))
                .unwrap_or_else(|| format!("{group_id}:{artifact_id}"));
            let body = format!("dependencies {{\n    {configuration} \"{coordinate}\"\n}}");
            replace_or_append_managed_block(&before, &marker, &body, "//")?
        }
        config::BuildSystem::Auto => unreachable!("auto build system must be resolved"),
    };
    change_for_managed_build_file(
        ctx,
        file,
        after,
        format!("add JVM dependency {group_id}:{artifact_id}"),
    )
}

fn jvm_add_bom(
    ctx: &ActionContext<'_>,
    action: &ActionSpec,
    input: Value,
) -> RainyResult<ChangeSet> {
    let target = required_string(&input, "target")?;
    let group_id = required_coordinate(&input, "groupId")?;
    let artifact_id = required_coordinate(&input, "artifactId")?;
    let version = required_coordinate(&input, "version")?;
    let (system, file, before) = jvm_build_file(ctx, &target)?;
    let marker = managed_marker(&ctx.capability.id, &action.id);
    let after = match system {
        config::BuildSystem::Maven => {
            let body = format!(
                "            <dependency>\n                <groupId>{}</groupId>\n                <artifactId>{}</artifactId>\n                <version>{}</version>\n                <type>pom</type>\n                <scope>import</scope>\n            </dependency>",
                xml_escape(&group_id),
                xml_escape(&artifact_id),
                xml_escape(&version)
            );
            maven_managed_dependency(&before, &marker, &body, true)?
        }
        config::BuildSystem::Gradle => {
            let configuration = optional_string(&input, "configuration")
                .unwrap_or_else(|| "implementation".to_string());
            validate_gradle_identifier(&configuration)?;
            let coordinate = format!("{group_id}:{artifact_id}:{version}");
            let body = if optional_string(&input, "gradleBomMode").as_deref()
                == Some("spring-dependency-management")
            {
                format!(
                    "dependencyManagement {{\n    imports {{\n        mavenBom \"{coordinate}\"\n    }}\n}}"
                )
            } else {
                format!("dependencies {{\n    {configuration} platform(\"{coordinate}\")\n}}")
            };
            replace_or_append_managed_block(&before, &marker, &body, "//")?
        }
        config::BuildSystem::Auto => unreachable!("auto build system must be resolved"),
    };
    change_for_managed_build_file(
        ctx,
        file,
        after,
        format!("add JVM BOM {group_id}:{artifact_id}"),
    )
}

fn jvm_remove_dependency(
    ctx: &ActionContext<'_>,
    action: &ActionSpec,
    input: Value,
) -> RainyResult<ChangeSet> {
    let target = required_string(&input, "target")?;
    let managed_action =
        optional_string(&input, "managedActionId").unwrap_or_else(|| action.id.clone());
    let (system, file, before) = jvm_build_file(ctx, &target)?;
    let marker = managed_marker(&ctx.capability.id, &managed_action);
    let after = match system {
        config::BuildSystem::Maven => remove_managed_block(&before, &marker, "<!--", "-->"),
        config::BuildSystem::Gradle => remove_managed_block(&before, &marker, "//", ""),
        config::BuildSystem::Auto => unreachable!("auto build system must be resolved"),
    }?;
    change_for_managed_build_file(
        ctx,
        file,
        after,
        format!("remove managed JVM dependency {marker}"),
    )
}

fn jvm_build_file(
    ctx: &ActionContext<'_>,
    target_name: &str,
) -> RainyResult<(config::BuildSystem, String, String)> {
    let target = ctx.config.build.targets.get(target_name).ok_or_else(|| {
        RainyError::action(
            "JVM_BUILD_TARGET_NOT_FOUND",
            format!("build target not found: {target_name}"),
        )
    })?;
    let (system, root) = crate::build::resolve_target_definition(ctx.workspace, target)?;
    let filename = match system {
        config::BuildSystem::Maven => "pom.xml",
        config::BuildSystem::Gradle => "build.gradle",
        config::BuildSystem::Auto => unreachable!("auto build system must be resolved"),
    };
    let path = root.join(filename);
    if !path.is_file() {
        return Err(RainyError::action(
            "JVM_BUILD_FILE_NOT_FOUND",
            format!(
                "{} build file not found: {}",
                build_system_label(system),
                path.display()
            ),
        ));
    }
    let relative = path
        .strip_prefix(ctx.workspace)
        .map_err(|_| {
            RainyError::action("JVM_BUILD_FILE_INVALID", "build file is outside workspace")
        })?
        .to_string_lossy()
        .replace('\\', "/");
    let before = ctx.read_file(&relative)?.ok_or_else(|| {
        RainyError::action(
            "JVM_BUILD_FILE_NOT_FOUND",
            format!(
                "{} build file not found: {relative}",
                build_system_label(system)
            ),
        )
    })?;
    Ok((system, relative, before))
}

fn change_for_managed_build_file(
    ctx: &ActionContext<'_>,
    file: String,
    after: String,
    summary: String,
) -> RainyResult<ChangeSet> {
    let mut changes = ChangeSet::new();
    changes.push(patch::change_for_file(ctx.workspace, file, after, summary)?);
    Ok(changes)
}

fn managed_marker(capability: &str, action: &str) -> String {
    format!("rainy-managed:{capability}:{action}")
}

fn replace_or_append_managed_block(
    before: &str,
    marker: &str,
    body: &str,
    comment: &str,
) -> RainyResult<String> {
    let begin = format!("{comment} {marker}:begin");
    let end = format!("{comment} {marker}:end");
    if before.contains(&begin) {
        replace_managed_range(before, &begin, &end, &format!("{begin}\n{body}\n{end}"))
    } else {
        Ok(format!("{}\n\n{begin}\n{body}\n{end}\n", before.trim_end()))
    }
}

fn maven_managed_dependency(
    before: &str,
    marker: &str,
    body: &str,
    bom: bool,
) -> RainyResult<String> {
    if !before.contains("<project") || !before.contains("</project>") {
        return Err(RainyError::action(
            "MAVEN_POM_INVALID",
            "pom.xml is not valid",
        ));
    }
    let begin = format!("<!-- {marker}:begin -->");
    let end = format!("<!-- {marker}:end -->");
    let block = format!("{begin}\n{body}\n{end}");
    let replaced = replace_managed_range(before, &begin, &end, &block)?;
    if replaced != before {
        return Ok(replaced);
    }
    if bom {
        if let Some(dm_end) = before.find("</dependencyManagement>") {
            let prefix = &before[..dm_end];
            if let Some(deps_end) = prefix.rfind("</dependencies>") {
                return Ok(insert_at(before, deps_end, &format!("{block}\n        ")));
            }
        }
        let section = format!(
            "    <dependencyManagement>\n        <dependencies>\n{block}\n        </dependencies>\n    </dependencyManagement>\n"
        );
        return insert_before_project_end(before, &section);
    }
    let search_start = before
        .find("</dependencyManagement>")
        .map(|index| index + "</dependencyManagement>".len())
        .unwrap_or(0);
    if let Some(relative) = before[search_start..].find("</dependencies>") {
        return Ok(insert_at(
            before,
            search_start + relative,
            &format!("{block}\n    "),
        ));
    }
    insert_before_project_end(
        before,
        &format!("    <dependencies>\n{block}\n    </dependencies>\n"),
    )
}

fn replace_managed_range(before: &str, begin: &str, end: &str, block: &str) -> RainyResult<String> {
    let Some(start) = before.find(begin) else {
        return Ok(before.to_string());
    };
    let end_start = before[start..].find(end).ok_or_else(|| {
        RainyError::action(
            "MANAGED_BLOCK_INVALID",
            format!("managed block {begin} has no end marker"),
        )
    })? + start;
    let end_index = end_start + end.len();
    let mut updated = String::with_capacity(before.len() + block.len());
    updated.push_str(&before[..start]);
    updated.push_str(block);
    updated.push_str(&before[end_index..]);
    Ok(updated)
}

fn remove_managed_block(
    before: &str,
    marker: &str,
    prefix: &str,
    suffix: &str,
) -> RainyResult<String> {
    let begin = format!(
        "{prefix} {marker}:begin{}",
        if suffix.is_empty() { "" } else { " " }
    );
    let begin = format!("{begin}{suffix}");
    let end = format!(
        "{prefix} {marker}:end{}",
        if suffix.is_empty() { "" } else { " " }
    );
    let end = format!("{end}{suffix}");
    let Some(start) = before.find(&begin) else {
        return Ok(before.to_string());
    };
    let end_start = before[start..].find(&end).ok_or_else(|| {
        RainyError::action(
            "MANAGED_BLOCK_INVALID",
            format!("managed block {marker} is incomplete"),
        )
    })? + start;
    let mut end_index = end_start + end.len();
    if before.as_bytes().get(end_index) == Some(&b'\n') {
        end_index += 1;
    }
    Ok(format!("{}{}", &before[..start], &before[end_index..]))
}

fn insert_before_project_end(before: &str, section: &str) -> RainyResult<String> {
    let index = before.rfind("</project>").ok_or_else(|| {
        RainyError::action(
            "MAVEN_POM_INVALID",
            "pom.xml has no closing project element",
        )
    })?;
    Ok(insert_at(before, index, section))
}

fn insert_at(before: &str, index: usize, content: &str) -> String {
    format!("{}{}{}", &before[..index], content, &before[index..])
}

fn required_coordinate(input: &Value, key: &str) -> RainyResult<String> {
    let value = required_string(input, key)?;
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err(RainyError::action(
            "JVM_COORDINATE_INVALID",
            format!("{key} contains unsupported characters"),
        ));
    }
    Ok(value)
}

fn validate_gradle_identifier(value: &str) -> RainyResult<()> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(RainyError::action(
            "GRADLE_CONFIGURATION_INVALID",
            "Gradle configuration must be an identifier",
        ));
    }
    Ok(())
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn build_system_label(system: config::BuildSystem) -> &'static str {
    match system {
        config::BuildSystem::Maven => "Maven",
        config::BuildSystem::Gradle => "Gradle",
        config::BuildSystem::Auto => "automatic",
    }
}

fn maven_add_bom(ctx: &ActionContext<'_>, input: Value) -> RainyResult<ChangeSet> {
    let module_path = required_string(&input, "modulePath")?;
    let group_id = required_string(&input, "groupId")?;
    let artifact_id = required_string(&input, "artifactId")?;
    let version = required_string(&input, "version")?;
    let pom_rel = format!("{module_path}/pom.xml");
    let before = ctx.read_file(&pom_rel)?.ok_or_else(|| {
        RainyError::action(
            "MAVEN_POM_NOT_FOUND",
            format!("pom.xml not found: {pom_rel}"),
        )
    })?;
    if before.contains(&format!("<groupId>{group_id}</groupId>"))
        && before.contains(&format!("<artifactId>{artifact_id}</artifactId>"))
        && before.contains("<type>pom</type>")
        && before.contains("<scope>import</scope>")
    {
        return Ok(ChangeSet::new());
    }
    if !before.contains("<project") {
        return Err(RainyError::action(
            "MAVEN_POM_INVALID",
            "pom.xml is not valid",
        ));
    }
    let bom = format!(
        "            <dependency>\n                <groupId>{group_id}</groupId>\n                <artifactId>{artifact_id}</artifactId>\n                <version>{version}</version>\n                <type>pom</type>\n                <scope>import</scope>\n            </dependency>\n"
    );
    let after = if before.contains("</dependencyManagement>") {
        before.replacen(
            "</dependencies>\n    </dependencyManagement>",
            &format!("{bom}        </dependencies>\n    </dependencyManagement>"),
            1,
        )
    } else {
        before.replacen(
            "</project>",
            &format!(
                "    <dependencyManagement>\n        <dependencies>\n{bom}        </dependencies>\n    </dependencyManagement>\n</project>"
            ),
            1,
        )
    };
    let mut changes = ChangeSet::new();
    changes.push(patch::change_for_file(
        ctx.workspace,
        pom_rel,
        after,
        format!("add Maven BOM {group_id}:{artifact_id}"),
    )?);
    Ok(changes)
}

fn maven_add_dependency(ctx: &ActionContext<'_>, input: Value) -> RainyResult<ChangeSet> {
    let module_path = required_string(&input, "modulePath")?;
    let group_id = required_string(&input, "groupId")?;
    let artifact_id = required_string(&input, "artifactId")?;
    let version = required_string(&input, "version")?;
    let scope = optional_string(&input, "scope");
    let pom_rel = format!("{module_path}/pom.xml");
    let before = ctx.read_file(&pom_rel)?.ok_or_else(|| {
        RainyError::action(
            "MAVEN_POM_NOT_FOUND",
            format!("pom.xml not found: {pom_rel}"),
        )
    })?;
    if before.contains(&format!("<groupId>{group_id}</groupId>"))
        && before.contains(&format!("<artifactId>{artifact_id}</artifactId>"))
    {
        return Ok(ChangeSet::new());
    }
    if !before.contains("<project") {
        return Err(RainyError::action(
            "MAVEN_POM_INVALID",
            "pom.xml is not valid",
        ));
    }

    let dependency = format!(
        "        <dependency>\n            <groupId>{group_id}</groupId>\n            <artifactId>{artifact_id}</artifactId>\n            <version>{version}</version>\n{}        </dependency>\n",
        scope
            .map(|scope| format!("            <scope>{scope}</scope>\n"))
            .unwrap_or_default()
    );

    let after = if before.contains("</dependencies>") {
        before.replacen(
            "</dependencies>",
            &format!("{dependency}    </dependencies>"),
            1,
        )
    } else {
        before.replacen(
            "</project>",
            &format!("    <dependencies>\n{dependency}    </dependencies>\n</project>"),
            1,
        )
    };

    let mut changes = ChangeSet::new();
    changes.push(patch::change_for_file(
        ctx.workspace,
        pom_rel,
        after,
        format!("add Maven dependency {group_id}:{artifact_id}"),
    )?);
    Ok(changes)
}

fn yaml_merge(ctx: &ActionContext<'_>, input: Value) -> RainyResult<ChangeSet> {
    let file = required_string(&input, "file")?;
    let patch_value = required_value(&input, "patch")?;
    let strategy =
        optional_string(&input, "mergeStrategy").unwrap_or_else(|| "preserve".to_string());
    let existing = match ctx.read_file(&file)? {
        Some(content) => serde_yaml::from_str::<Value>(&content)?,
        None => Value::Mapping(Mapping::new()),
    };
    let merged = merge_yaml(existing.clone(), patch_value.clone(), &strategy)?;
    if yaml_semantic_equal(&existing, &merged) {
        return Ok(ChangeSet::new());
    }
    let after = serde_yaml::to_string(&merged)?;
    let mut changes = ChangeSet::new();
    changes.push(patch::change_for_file(
        ctx.workspace,
        file,
        after,
        "merge YAML configuration",
    )?);
    Ok(changes)
}

fn json_merge(ctx: &ActionContext<'_>, input: Value, jsonc: bool) -> RainyResult<ChangeSet> {
    let file = required_string(&input, "file")?;
    let patch_value = required_value(&input, "patch")?;
    let strategy =
        optional_string(&input, "mergeStrategy").unwrap_or_else(|| "preserve".to_string());
    let existing = if let Some(content) = ctx.read_file(&file)? {
        let content = if jsonc {
            strip_jsonc_comments(&content)
        } else {
            content
        };
        serde_json::from_str::<serde_json::Value>(&content)?
    } else {
        json!({})
    };
    let patch_json = serde_json::to_value(patch_value)?;
    let merged = merge_json(existing.clone(), patch_json, &strategy)?;
    if existing == merged {
        return Ok(ChangeSet::new());
    }
    let mut changes = ChangeSet::new();
    changes.push(patch::change_for_file(
        ctx.workspace,
        file,
        format!("{}\n", serde_json::to_string_pretty(&merged)?),
        "merge JSON configuration",
    )?);
    Ok(changes)
}

fn toml_merge(ctx: &ActionContext<'_>, input: Value) -> RainyResult<ChangeSet> {
    let file = required_string(&input, "file")?;
    let patch_value = required_value(&input, "patch")?;
    let strategy =
        optional_string(&input, "mergeStrategy").unwrap_or_else(|| "preserve".to_string());
    let existing_json = if let Some(content) = ctx.read_file(&file)? {
        let existing: toml::Value = toml::from_str(&content)
            .map_err(|err| RainyError::action("TOML_INVALID", err.to_string()))?;
        serde_json::to_value(existing)?
    } else {
        json!({})
    };
    let patch_json = serde_json::to_value(patch_value)?;
    let merged = merge_json(existing_json.clone(), patch_json, &strategy)?;
    if existing_json == merged {
        return Ok(ChangeSet::new());
    }
    let toml_value: toml::Value = serde_json::from_value(merged)?;
    let mut changes = ChangeSet::new();
    changes.push(patch::change_for_file(
        ctx.workspace,
        file,
        toml::to_string_pretty(&toml_value)
            .map_err(|err| RainyError::action("TOML_SERIALIZE_FAILED", err.to_string()))?,
        "merge TOML configuration",
    )?);
    Ok(changes)
}

fn file_create(ctx: &ActionContext<'_>, input: Value) -> RainyResult<ChangeSet> {
    let path = required_string(&input, "path")?;
    let content = optional_string(&input, "content").unwrap_or_default();
    if ctx.read_file(&path)?.is_some() && !ctx.force {
        return Ok(ChangeSet::new());
    }
    let mut changes = ChangeSet::new();
    changes.push(patch::change_for_file(
        ctx.workspace,
        path,
        content,
        "create file",
    )?);
    Ok(changes)
}

fn file_append(ctx: &ActionContext<'_>, input: Value) -> RainyResult<ChangeSet> {
    let path = required_string(&input, "path")?;
    let content = required_string(&input, "content")?;
    let before = ctx.read_file(&path)?.unwrap_or_default();
    if before.contains(&content) {
        return Ok(ChangeSet::new());
    }
    let mut after = before;
    if !after.is_empty() && !after.ends_with('\n') {
        after.push('\n');
    }
    after.push_str(&content);
    if !after.ends_with('\n') {
        after.push('\n');
    }
    let mut changes = ChangeSet::new();
    changes.push(patch::change_for_file(
        ctx.workspace,
        path,
        after,
        "append file",
    )?);
    Ok(changes)
}

fn template_render(ctx: &ActionContext<'_>, input: Value) -> RainyResult<ChangeSet> {
    let template = required_string(&input, "template")?;
    let target = required_string(&input, "target")?;
    let template_root = ctx.capability.pack_root.join(template);
    if !template_root.exists() {
        return Err(RainyError::action(
            "TEMPLATE_NOT_FOUND",
            format!("template path not found: {}", template_root.display()),
        ));
    }

    let mut changes = ChangeSet::new();
    let mut handlebars = Handlebars::new();
    handlebars.set_strict_mode(true);
    for entry in WalkDir::new(&template_root) {
        let entry = entry.map_err(anyhow::Error::from)?;
        if entry.file_type().is_dir() {
            continue;
        }
        let source_path = entry.path();
        let rel = source_path
            .strip_prefix(&template_root)
            .map_err(anyhow::Error::from)?;
        let mut rel_string = rel.to_string_lossy().replace('\\', "/");
        if rel_string.ends_with(".hbs") {
            rel_string.truncate(rel_string.len() - 4);
        }
        let rendered_rel = render_string(ctx, &rel_string)?;
        let target_rel = format!("{}/{}", target.trim_end_matches('/'), rendered_rel);
        let bytes = std::fs::read(source_path)?;
        let content = match String::from_utf8(bytes) {
            Ok(text) => handlebars
                .render_template(&text, &ctx.variables)
                .map_err(|err| RainyError::action("TEMPLATE_RENDER_FAILED", err.to_string()))?,
            Err(err) => String::from_utf8_lossy(err.as_bytes()).to_string(),
        };
        if let Some(existing) = ctx.read_file(&target_rel)?
            && !ctx.force
        {
            if existing != content {
                return Err(RainyError::action(
                    "TEMPLATE_CONFLICT",
                    format!("template target already exists with different content: {target_rel}"),
                ));
            }
        }
        changes.push(patch::change_for_file(
            ctx.workspace,
            target_rel,
            content,
            "render template",
        )?);
    }
    Ok(changes)
}

fn docker_compose_add_service(ctx: &ActionContext<'_>, input: Value) -> RainyResult<ChangeSet> {
    let file = required_string(&input, "file")?;
    let service_name = required_string(&input, "serviceName")?;
    let template = required_string(&input, "template")?;
    let template_path = ctx.capability.pack_root.join(template);
    if !template_path.exists() {
        return Err(RainyError::action(
            "TEMPLATE_NOT_FOUND",
            format!("compose template not found: {}", template_path.display()),
        ));
    }
    let rendered = render_string(ctx, &std::fs::read_to_string(template_path)?)?;
    let service_yaml: Value = serde_yaml::from_str(&rendered)?;
    let mut compose = if let Some(content) = ctx.read_file(&file)? {
        serde_yaml::from_str::<Value>(&content)?
    } else {
        let mut root = Mapping::new();
        root.insert(
            Value::String("services".to_string()),
            Value::Mapping(Mapping::new()),
        );
        Value::Mapping(root)
    };

    let root = compose
        .as_mapping_mut()
        .ok_or_else(|| RainyError::action("COMPOSE_INVALID", "compose file must be a mapping"))?;
    let services_key = Value::String("services".to_string());
    if !root.contains_key(&services_key) {
        root.insert(services_key.clone(), Value::Mapping(Mapping::new()));
    }
    let services = root
        .get_mut(&services_key)
        .and_then(Value::as_mapping_mut)
        .ok_or_else(|| RainyError::action("COMPOSE_INVALID", "services must be a mapping"))?;
    let service_key = Value::String(service_name);
    if !services.contains_key(&service_key) {
        services.insert(service_key, service_yaml.clone());
    }
    ensure_compose_named_volumes(&mut compose, &service_yaml)?;

    let mut changes = ChangeSet::new();
    changes.push(patch::change_for_file(
        ctx.workspace,
        file,
        serde_yaml::to_string(&compose)?,
        "add docker compose service",
    )?);
    Ok(changes)
}

fn ensure_compose_named_volumes(compose: &mut Value, service_yaml: &Value) -> RainyResult<()> {
    let Some(service_volumes) = service_yaml
        .as_mapping()
        .and_then(|mapping| mapping.get(Value::String("volumes".to_string())))
        .and_then(Value::as_sequence)
    else {
        return Ok(());
    };
    let named_volumes = service_volumes
        .iter()
        .filter_map(Value::as_str)
        .filter_map(|volume| volume.split(':').next())
        .filter(|source| {
            !source.is_empty()
                && !source.starts_with('.')
                && !source.starts_with('/')
                && !source.contains("${")
        })
        .map(|source| Value::String(source.to_string()))
        .collect::<Vec<_>>();
    if named_volumes.is_empty() {
        return Ok(());
    }
    let root = compose
        .as_mapping_mut()
        .ok_or_else(|| RainyError::action("COMPOSE_INVALID", "compose file must be a mapping"))?;
    let volumes_key = Value::String("volumes".to_string());
    if !root.contains_key(&volumes_key) {
        root.insert(volumes_key.clone(), Value::Mapping(Mapping::new()));
    }
    let volumes = root
        .get_mut(&volumes_key)
        .and_then(Value::as_mapping_mut)
        .ok_or_else(|| RainyError::action("COMPOSE_INVALID", "volumes must be a mapping"))?;
    for volume in named_volumes {
        if !volumes.contains_key(&volume) {
            volumes.insert(volume, Value::Mapping(Mapping::new()));
        }
    }
    Ok(())
}

fn package_json_add_dependency(ctx: &ActionContext<'_>, input: Value) -> RainyResult<ChangeSet> {
    let file = required_string(&input, "file")?;
    let name = required_string(&input, "name")?;
    let version = required_string(&input, "version")?;
    let dev = bool_value(&input, "dev").unwrap_or(false);
    let mut package = if let Some(content) = ctx.read_file(&file)? {
        serde_json::from_str::<serde_json::Value>(&content)?
    } else {
        json!({})
    };
    let section = if dev {
        "devDependencies"
    } else {
        "dependencies"
    };
    if package.get(section).is_none() {
        package[section] = json!({});
    }
    if package[section].get(&name).is_none() {
        package[section][&name] = json!(version);
    }
    let mut changes = ChangeSet::new();
    changes.push(patch::change_for_file(
        ctx.workspace,
        file,
        format!("{}\n", serde_json::to_string_pretty(&package)?),
        format!("add package dependency {name}"),
    )?);
    Ok(changes)
}

fn package_json_add_script(ctx: &ActionContext<'_>, input: Value) -> RainyResult<ChangeSet> {
    let file = required_string(&input, "file")?;
    let name = required_string(&input, "name")?;
    let script = required_string(&input, "script")?;
    let mut package = if let Some(content) = ctx.read_file(&file)? {
        serde_json::from_str::<serde_json::Value>(&content)?
    } else {
        json!({})
    };
    if package.get("scripts").is_none() {
        package["scripts"] = json!({});
    }
    if package["scripts"].get(&name).is_none() {
        package["scripts"][&name] = json!(script);
    }
    let mut changes = ChangeSet::new();
    changes.push(patch::change_for_file(
        ctx.workspace,
        file,
        format!("{}\n", serde_json::to_string_pretty(&package)?),
        format!("add package script {name}"),
    )?);
    Ok(changes)
}

fn render_yaml_value(ctx: &ActionContext<'_>, value: &Value) -> RainyResult<Value> {
    match value {
        Value::String(text) => Ok(Value::String(render_string(ctx, text)?)),
        Value::Sequence(items) => Ok(Value::Sequence(
            items
                .iter()
                .map(|item| render_yaml_value(ctx, item))
                .collect::<RainyResult<Vec<_>>>()?,
        )),
        Value::Mapping(mapping) => {
            let mut output = Mapping::new();
            for (key, value) in mapping {
                output.insert(render_yaml_value(ctx, key)?, render_yaml_value(ctx, value)?);
            }
            Ok(Value::Mapping(output))
        }
        other => Ok(other.clone()),
    }
}

fn render_string(ctx: &ActionContext<'_>, text: &str) -> RainyResult<String> {
    let mut handlebars = Handlebars::new();
    handlebars.set_strict_mode(true);
    handlebars
        .render_template(text, &ctx.variables)
        .map_err(|err| RainyError::plan("VARIABLE_RENDER_FAILED", err.to_string()))
}

fn merge_yaml(existing: Value, patch: Value, strategy: &str) -> RainyResult<Value> {
    match (existing, patch) {
        (Value::Mapping(mut existing), Value::Mapping(patch)) => {
            for (key, patch_value) in patch {
                if let Some(existing_value) = existing.remove(&key) {
                    existing.insert(key, merge_yaml(existing_value, patch_value, strategy)?);
                } else {
                    existing.insert(key, patch_value);
                }
            }
            Ok(Value::Mapping(existing))
        }
        (existing, patch) => match strategy {
            "overwrite" => Ok(patch),
            "failOnConflict" if existing != patch => {
                Err(RainyError::action("PATCH_CONFLICT", "YAML merge conflict"))
            }
            _ => Ok(existing),
        },
    }
}

fn merge_json(
    existing: serde_json::Value,
    patch: serde_json::Value,
    strategy: &str,
) -> RainyResult<serde_json::Value> {
    match (existing, patch) {
        (serde_json::Value::Object(mut existing), serde_json::Value::Object(patch)) => {
            for (key, patch_value) in patch {
                if let Some(existing_value) = existing.remove(&key) {
                    existing.insert(key, merge_json(existing_value, patch_value, strategy)?);
                } else {
                    existing.insert(key, patch_value);
                }
            }
            Ok(serde_json::Value::Object(existing))
        }
        (existing, patch) => match strategy {
            "overwrite" => Ok(patch),
            "failOnConflict" if existing != patch => {
                Err(RainyError::action("PATCH_CONFLICT", "JSON merge conflict"))
            }
            _ => Ok(existing),
        },
    }
}

fn strip_jsonc_comments(content: &str) -> String {
    content
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            !trimmed.starts_with("//")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn yaml_semantic_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Mapping(left), Value::Mapping(right)) => {
            left.len() == right.len()
                && left.iter().all(|(key, left_value)| {
                    right
                        .get(key)
                        .is_some_and(|right_value| yaml_semantic_equal(left_value, right_value))
                })
        }
        (Value::Sequence(left), Value::Sequence(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right.iter())
                    .all(|(left, right)| yaml_semantic_equal(left, right))
        }
        _ => left == right,
    }
}

fn required_string(input: &Value, key: &str) -> RainyResult<String> {
    optional_string(input, key)
        .ok_or_else(|| RainyError::action("ACTION_INPUT_INVALID", format!("missing input: {key}")))
}

fn optional_string(input: &Value, key: &str) -> Option<String> {
    get(input, key).and_then(|value| match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    })
}

fn bool_value(input: &Value, key: &str) -> Option<bool> {
    get(input, key).and_then(Value::as_bool)
}

fn required_value<'a>(input: &'a Value, key: &str) -> RainyResult<&'a Value> {
    get(input, key)
        .ok_or_else(|| RainyError::action("ACTION_INPUT_INVALID", format!("missing input: {key}")))
}

fn get<'a>(input: &'a Value, key: &str) -> Option<&'a Value> {
    input
        .as_mapping()
        .and_then(|mapping| mapping.get(Value::String(key.to_string())))
}

fn yaml_scalar_to_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Null => String::new(),
        other => serde_yaml::to_string(other)
            .unwrap_or_default()
            .trim()
            .to_string(),
    }
}
