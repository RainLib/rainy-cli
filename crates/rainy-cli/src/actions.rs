use crate::config::{self, LockedCapability};
use crate::error::{RainyError, RainyResult};
use crate::patch::{self, ChangeSet};
use crate::registry::{ActionSpec, CapabilityDefinition, CapabilityInput, RegistryClient};
use chrono::Utc;
use handlebars::Handlebars;
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_yaml::{Mapping, Value};
use std::path::Path;
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct AddCapabilityRequest {
    pub capability_id: String,
    pub provider: Option<String>,
    pub force: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPlan {
    pub id: String,
    pub capability: String,
    pub provider: String,
    pub actions: Vec<PlannedAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    capability: &'a CapabilityDefinition,
    variables: serde_json::Value,
    force: bool,
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
    let variables = variables_for(&config, &capability.inputs);
    let ctx = ActionContext {
        workspace,
        capability: &capability,
        variables,
        force: request.force,
    };

    let mut actions = Vec::new();
    let mut changes = ChangeSet::new();

    for action in &capability.actions.install {
        let rendered = render_yaml_value(&ctx, &action.with_value)?;
        let input = serde_json::to_value(&rendered)?;
        actions.push(PlannedAction {
            id: action.id.clone(),
            uses: action.uses.clone(),
            input,
        });
        changes.extend(plan_action(&ctx, action, rendered)?);
    }

    append_lock_change(workspace, lock, &capability, &provider, &mut changes)?;

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
    let variables = variables_for(&config, &capability.inputs);
    let ctx = ActionContext {
        workspace,
        capability: &capability,
        variables,
        force,
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
    append_lock_change(workspace, lock, &capability, &provider, &mut changes)?;

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

    for artifact in removed.artifacts {
        if shared_artifacts.contains(&artifact) {
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
    mut lock: config::CapabilityLock,
    capability: &CapabilityDefinition,
    provider: &str,
    changes: &mut ChangeSet,
) -> RainyResult<()> {
    let artifacts = changes.changed_files();
    let next_capability = if let Some(existing) = lock.capabilities.get(&capability.id) {
        let mut existing = existing.clone();
        existing.provider = Some(provider.to_string());
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
            installed_at: Utc::now(),
            artifacts,
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
        "inputs": input_values
    })
}

fn plan_action(
    ctx: &ActionContext<'_>,
    action: &ActionSpec,
    input: Value,
) -> RainyResult<ChangeSet> {
    match action.uses.as_str() {
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
    }
}

fn maven_add_bom(ctx: &ActionContext<'_>, input: Value) -> RainyResult<ChangeSet> {
    let module_path = required_string(&input, "modulePath")?;
    let group_id = required_string(&input, "groupId")?;
    let artifact_id = required_string(&input, "artifactId")?;
    let version = required_string(&input, "version")?;
    let pom_rel = format!("{module_path}/pom.xml");
    let pom_path = ctx.workspace.join(&pom_rel);
    if !pom_path.exists() {
        return Err(RainyError::action(
            "MAVEN_POM_NOT_FOUND",
            format!("pom.xml not found: {pom_rel}"),
        ));
    }
    let before = std::fs::read_to_string(&pom_path)?;
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
    let pom_path = ctx.workspace.join(&pom_rel);
    if !pom_path.exists() {
        return Err(RainyError::action(
            "MAVEN_POM_NOT_FOUND",
            format!("pom.xml not found: {pom_rel}"),
        ));
    }
    let before = std::fs::read_to_string(&pom_path)?;
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
    let path = ctx.workspace.join(&file);
    let existing = if path.exists() {
        serde_yaml::from_str::<Value>(&std::fs::read_to_string(&path)?)?
    } else {
        Value::Mapping(Mapping::new())
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
    let path = ctx.workspace.join(&file);
    let existing = if path.exists() {
        let content = std::fs::read_to_string(&path)?;
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
    let path = ctx.workspace.join(&file);
    let existing_json = if path.exists() {
        let existing: toml::Value = toml::from_str(&std::fs::read_to_string(&path)?)
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
    if ctx.workspace.join(&path).exists() && !ctx.force {
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
    let abs = ctx.workspace.join(&path);
    let before = if abs.exists() {
        std::fs::read_to_string(&abs)?
    } else {
        String::new()
    };
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
        let target_path = ctx.workspace.join(&target_rel);
        if target_path.exists() && !ctx.force {
            let existing = std::fs::read_to_string(&target_path)?;
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
    let path = ctx.workspace.join(&file);
    let mut compose = if path.exists() {
        serde_yaml::from_str::<Value>(&std::fs::read_to_string(&path)?)?
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
    let path = ctx.workspace.join(&file);
    let mut package = if path.exists() {
        serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(&path)?)?
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
    let path = ctx.workspace.join(&file);
    let mut package = if path.exists() {
        serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(&path)?)?
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
