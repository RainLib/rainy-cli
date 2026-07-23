use crate::cli::{
    PackCommand, PackSubcommand, RegistryAddArgs, RegistryCommand, RegistryDoctorArgs,
    RegistryRemoveArgs, RegistrySubcommand, RegistrySyncArgs, SkillTarget,
};
use crate::config::{self, ProjectConfig, RegistrySourceConfig};
use crate::error::{RainyError, RainyResult};
use crate::output::CommandOutput;
use crate::patch::{self, ChangeSet};
use crate::policy;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions};
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

const MAX_REGISTRY_RESPONSE_BYTES: u64 = 5 * 1024 * 1024;
const MAX_ARCHIVE_BYTES: u64 = 100 * 1024 * 1024;
const MAX_EXTRACTED_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 10_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityPack {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    pub metadata: PackMetadata,
    #[serde(default)]
    pub requires: BTreeMap<String, String>,
    #[serde(default)]
    pub exports: PackExports,
    #[serde(skip)]
    pub root: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackMetadata {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PackExports {
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub validators: Vec<String>,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub plugins: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryReport {
    pub protocol_version: String,
    pub status: String,
    pub operation: String,
    pub registries: Vec<RegistryInfo>,
    pub checks: Vec<RegistryCheck>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryInfo {
    pub name: String,
    pub source_type: String,
    pub source: String,
    pub priority: i32,
    pub requested_ref: Option<String>,
    pub resolved_ref: Option<String>,
    pub digest: Option<String>,
    pub cache_path: Option<String>,
    pub modules: Vec<String>,
    pub synced: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RegistryCheck {
    pub id: String,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityDefinition {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(rename = "dependsOn", default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub providers: Vec<CapabilityProvider>,
    #[serde(default)]
    pub inputs: BTreeMap<String, CapabilityInput>,
    pub actions: CapabilityActions,
    #[serde(default)]
    pub validations: Vec<ValidationCommand>,
    #[serde(default)]
    pub doctor: CapabilityDoctor,
    #[serde(rename = "agentRules", default)]
    pub agent_rules: Vec<String>,
    #[serde(default)]
    pub policy: config::PolicySection,
    #[serde(skip)]
    pub pack_root: PathBuf,
    #[serde(skip)]
    pub pack_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityProvider {
    pub id: String,
    #[serde(default)]
    pub default: bool,
    #[serde(rename = "requiredConfig", default)]
    pub required_config: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityInput {
    #[serde(rename = "type")]
    pub input_type: String,
    #[serde(default)]
    pub default: Option<serde_yaml::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityActions {
    #[serde(default)]
    pub install: Vec<ActionSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionSpec {
    pub id: String,
    pub uses: String,
    #[serde(rename = "with", default)]
    pub with_value: serde_yaml::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationCommand {
    pub id: String,
    pub command: String,
    #[serde(rename = "workingDirectory", default)]
    pub working_directory: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapabilityDoctor {
    #[serde(default)]
    pub checks: Vec<DoctorCheckSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorCheckSpec {
    pub id: String,
    pub uses: String,
    #[serde(rename = "with", default)]
    pub with_value: serde_yaml::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct CapabilitySummary {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub pack: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CapabilityInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub depends_on: Vec<String>,
    pub providers: Vec<String>,
    pub actions: Vec<String>,
    pub pack: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PackInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpRegistryIndex {
    pub protocol_version: String,
    #[serde(default)]
    pub packs: Vec<HttpRegistryPack>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpRegistryPack {
    pub name: String,
    pub version: String,
    pub base_url: String,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub digests: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackSignature {
    pub protocol_version: String,
    pub algorithm: String,
    pub digest: String,
    pub signed_at: String,
    pub files: Vec<PackFileDigest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackFileDigest {
    pub path: String,
    pub digest: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CapabilityGraph {
    pub nodes: Vec<String>,
    pub edges: Vec<CapabilityEdge>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CapabilityEdge {
    pub from: String,
    pub to: String,
}

pub fn capability_list(workspace: &Path) -> RainyResult<CommandOutput> {
    let registry = RegistryClient::load(workspace)?;
    let capabilities = registry
        .capabilities()
        .into_iter()
        .map(|capability| CapabilitySummary {
            id: capability.id,
            name: capability.name,
            version: capability.version,
            description: capability.description,
            pack: capability.pack_name,
        })
        .collect();
    Ok(CommandOutput::Capabilities { capabilities })
}

pub fn capability_explain(workspace: &Path, id: &str) -> RainyResult<CommandOutput> {
    let registry = RegistryClient::load(workspace)?;
    let capability = registry.get_capability(id)?;
    let info = CapabilityInfo {
        id: capability.id,
        name: capability.name,
        version: capability.version,
        description: capability.description,
        depends_on: capability.depends_on,
        providers: capability
            .providers
            .into_iter()
            .map(|provider| provider.id)
            .collect(),
        actions: capability
            .actions
            .install
            .into_iter()
            .map(|action| format!("{} ({})", action.id, action.uses))
            .collect(),
        pack: capability.pack_name,
    };
    Ok(CommandOutput::Capability { capability: info })
}

pub fn capability_graph(workspace: &Path) -> RainyResult<CommandOutput> {
    let registry = RegistryClient::load(workspace)?;
    let capabilities = registry.capabilities();
    let mut nodes = BTreeSet::new();
    let mut edges = Vec::new();
    for capability in capabilities {
        nodes.insert(capability.id.clone());
        for dep in capability.depends_on {
            nodes.insert(dep.clone());
            edges.push(CapabilityEdge {
                from: capability.id.clone(),
                to: dep,
            });
        }
    }
    Ok(CommandOutput::CapabilityGraph {
        graph: CapabilityGraph {
            nodes: nodes.into_iter().collect(),
            edges,
        },
    })
}

pub fn handle_pack_command(workspace: &Path, command: PackCommand) -> RainyResult<CommandOutput> {
    match command.command {
        PackSubcommand::List => {
            let registry = RegistryClient::load(workspace)?;
            let packs = registry
                .packs
                .into_iter()
                .map(|pack| PackInfo {
                    name: pack.metadata.name,
                    version: pack.metadata.version,
                    description: pack.metadata.description.unwrap_or_default(),
                    path: pack.root.to_string_lossy().to_string(),
                })
                .collect();
            Ok(CommandOutput::Packs { packs })
        }
        PackSubcommand::Inspect { id } => {
            let registry = RegistryClient::load(workspace)?;
            let packs = registry
                .packs
                .into_iter()
                .filter(|pack| pack.metadata.name == id)
                .map(|pack| PackInfo {
                    name: pack.metadata.name,
                    version: pack.metadata.version,
                    description: pack.metadata.description.unwrap_or_default(),
                    path: pack.root.to_string_lossy().to_string(),
                })
                .collect::<Vec<_>>();
            if packs.is_empty() {
                return Err(RainyError::registry(
                    "PACK_NOT_FOUND",
                    format!("pack not found: {id}"),
                ));
            }
            Ok(CommandOutput::Packs { packs })
        }
        PackSubcommand::Install(args) => install_pack(workspace, args),
        PackSubcommand::Update(args) => update_packs(workspace, args),
        PackSubcommand::Sign(args) => sign_pack(&args.path),
        PackSubcommand::Verify(args) => verify_pack_signature(&args.path),
    }
}

pub fn handle_registry_command(
    workspace: &Path,
    command: RegistryCommand,
) -> RainyResult<CommandOutput> {
    match command.command {
        RegistrySubcommand::List => registry_list(workspace),
        RegistrySubcommand::Add(args) => registry_add(workspace, args),
        RegistrySubcommand::Sync(args) => registry_sync(workspace, args),
        RegistrySubcommand::Remove(args) => registry_remove(workspace, args),
        RegistrySubcommand::Doctor(args) => registry_doctor(workspace, args),
    }
}

fn registry_list(workspace: &Path) -> RainyResult<CommandOutput> {
    let config = config::load_config(workspace)?;
    let lock = config::load_registry_lock(workspace)?;
    Ok(CommandOutput::Registry {
        report: registry_report(
            "list",
            "passed",
            config
                .capability_registry
                .sources
                .iter()
                .enumerate()
                .map(|(index, source)| registry_info(source, index, &lock))
                .collect(),
            Vec::new(),
        ),
    })
}

fn registry_add(workspace: &Path, args: RegistryAddArgs) -> RainyResult<CommandOutput> {
    let apply = resolve_apply_flags(args.dry_run, args.apply)?;
    validate_registry_name(&args.name)?;
    let source = registry_source_from_input(
        Some(args.name.clone()),
        args.source,
        args.reference,
        args.sha256,
        args.priority,
    )?;
    let mut project = config::load_config(workspace)?;
    project
        .capability_registry
        .sources
        .retain(|existing| existing.configured_name() != Some(args.name.as_str()));
    project.capability_registry.sources.push(source.clone());
    let mut changes = ChangeSet::new();
    changes.push(patch::change_for_file(
        workspace,
        "rainy.yaml",
        config::serialize_config(&project)?,
        format!("configure registry {}", args.name),
    )?);
    if apply {
        policy::check_changes(workspace, &changes)?;
        patch::apply_changes(workspace, &changes)?;
    }
    Ok(CommandOutput::Registry {
        report: registry_report(
            "add",
            if apply { "applied" } else { "dry-run" },
            vec![registry_info(
                &source,
                project.capability_registry.sources.len() - 1,
                &config::load_registry_lock(workspace)?,
            )],
            Vec::new(),
        ),
    })
}

fn registry_sync(workspace: &Path, args: RegistrySyncArgs) -> RainyResult<CommandOutput> {
    let apply = resolve_apply_flags(args.dry_run, args.apply)?;
    if !args.all && args.module.is_empty() {
        return Err(RainyError::registry(
            "REGISTRY_MODULE_SELECTION_REQUIRED",
            "select one or more --module values or pass --all",
        ));
    }
    if args.install_skills && args.target.is_empty() {
        return Err(RainyError::registry(
            "REGISTRY_SKILL_TARGET_REQUIRED",
            "--install-skills requires at least one --target",
        ));
    }
    let project = config::load_config(workspace)?;
    let selected = project
        .capability_registry
        .sources
        .iter()
        .enumerate()
        .filter(|(index, source)| {
            args.all_registries
                || args.name.as_deref() == Some(registry_name(source, *index).as_str())
        })
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return Err(RainyError::registry(
            "REGISTRY_NOT_FOUND",
            format!(
                "registry not found: {}",
                args.name.as_deref().unwrap_or("configured registries")
            ),
        ));
    }
    if !apply {
        let lock = config::load_registry_lock(workspace)?;
        return Ok(CommandOutput::Registry {
            report: registry_report(
                "sync",
                "dry-run",
                selected
                    .into_iter()
                    .map(|(index, source)| {
                        let mut info = registry_info(source, index, &lock);
                        info.modules = if args.all {
                            vec!["*".to_string()]
                        } else {
                            args.module.clone()
                        };
                        info
                    })
                    .collect(),
                Vec::new(),
            ),
        });
    }

    preflight_registry_lock_write(workspace)?;
    let mut lock = config::load_registry_lock(workspace)?;
    let mut infos = Vec::new();
    for (index, source) in selected {
        let name = registry_name(source, index);
        let previous_skills = lock
            .registries
            .get(&name)
            .map(|locked| locked.installed_skills.clone())
            .unwrap_or_default();
        let mut locked = sync_registry_source(
            workspace,
            &name,
            source,
            if args.all { &[] } else { &args.module },
        )?;
        locked.installed_skills = if args.install_skills {
            let registry_root = locked_registry_path(workspace, source, &locked)?;
            install_registry_skills(
                workspace,
                &registry_root,
                &args.target,
                &previous_skills,
                args.force,
            )?
        } else {
            previous_skills
        };
        lock.registries.insert(name.clone(), locked);
        infos.push(registry_info(source, index, &lock));
    }
    let mut changes = ChangeSet::new();
    changes.push(patch::change_for_file(
        workspace,
        ".rainy/registry.lock",
        config::save_registry_lock_content(&lock)?,
        "lock synchronized registries",
    )?);
    policy::check_changes(workspace, &changes)?;
    patch::apply_changes(workspace, &changes)?;
    Ok(CommandOutput::Registry {
        report: registry_report("sync", "applied", infos, Vec::new()),
    })
}

fn registry_remove(workspace: &Path, args: RegistryRemoveArgs) -> RainyResult<CommandOutput> {
    let apply = resolve_apply_flags(args.dry_run, args.apply)?;
    let mut project = config::load_config(workspace)?;
    let original_len = project.capability_registry.sources.len();
    project
        .capability_registry
        .sources
        .retain(|source| source.configured_name() != Some(args.name.as_str()));
    if project.capability_registry.sources.len() == original_len {
        return Err(RainyError::registry(
            "REGISTRY_NOT_FOUND",
            format!("registry not found: {}", args.name),
        ));
    }
    let mut lock = config::load_registry_lock(workspace)?;
    lock.registries.remove(&args.name);
    let mut changes = ChangeSet::new();
    changes.push(patch::change_for_file(
        workspace,
        "rainy.yaml",
        config::serialize_config(&project)?,
        format!("remove registry {}", args.name),
    )?);
    changes.push(patch::change_for_file(
        workspace,
        ".rainy/registry.lock",
        config::save_registry_lock_content(&lock)?,
        format!("unlock registry {}", args.name),
    )?);
    if apply {
        policy::check_changes(workspace, &changes)?;
        patch::apply_changes(workspace, &changes)?;
    }
    Ok(CommandOutput::Registry {
        report: registry_report(
            "remove",
            if apply { "applied" } else { "dry-run" },
            vec![RegistryInfo {
                name: args.name,
                source_type: "removed".to_string(),
                source: String::new(),
                priority: 0,
                requested_ref: None,
                resolved_ref: None,
                digest: None,
                cache_path: None,
                modules: Vec::new(),
                synced: false,
            }],
            Vec::new(),
        ),
    })
}

fn registry_doctor(workspace: &Path, args: RegistryDoctorArgs) -> RainyResult<CommandOutput> {
    let project = config::load_config(workspace)?;
    let lock = config::load_registry_lock(workspace)?;
    let mut infos = Vec::new();
    let mut checks = Vec::new();
    for (index, source) in project.capability_registry.sources.iter().enumerate() {
        let name = registry_name(source, index);
        if args.name.as_deref().is_some_and(|wanted| wanted != name) {
            continue;
        }
        infos.push(registry_info(source, index, &lock));
        let source_path = source_path(workspace, source, index)?;
        let exists = source_path.exists();
        checks.push(RegistryCheck {
            id: format!("registry.{name}.cache"),
            status: if exists { "passed" } else { "failed" }.to_string(),
            message: if exists {
                format!("registry source is available at {}", source_path.display())
            } else {
                "registry is not synchronized; run registry sync".to_string()
            },
        });
        checks.push(RegistryCheck {
            id: format!("registry.{name}.lock"),
            status: if lock.registries.contains_key(&name) {
                "passed"
            } else {
                "failed"
            }
            .to_string(),
            message: if lock.registries.contains_key(&name) {
                "registry lock entry exists".to_string()
            } else {
                "registry lock entry is missing".to_string()
            },
        });
    }
    if infos.is_empty() && args.name.is_some() {
        return Err(RainyError::registry(
            "REGISTRY_NOT_FOUND",
            format!("registry not found: {}", args.name.unwrap_or_default()),
        ));
    }
    let status = if checks.iter().any(|check| check.status == "failed") {
        "failed"
    } else {
        "passed"
    };
    Ok(CommandOutput::Registry {
        report: registry_report("doctor", status, infos, checks),
    })
}

fn registry_report(
    operation: &str,
    status: &str,
    registries: Vec<RegistryInfo>,
    checks: Vec<RegistryCheck>,
) -> RegistryReport {
    RegistryReport {
        protocol_version: "rainy.registry-report.v1".to_string(),
        status: status.to_string(),
        operation: operation.to_string(),
        registries,
        checks,
    }
}

fn registry_source_from_input(
    name: Option<String>,
    source: String,
    reference: Option<String>,
    sha256: Option<String>,
    priority: i32,
) -> RainyResult<RegistrySourceConfig> {
    if let Some(url) = source.strip_prefix("git+") {
        validate_git_url(url)?;
        return Ok(RegistrySourceConfig::Git {
            name,
            priority,
            url: url.to_string(),
            reference,
        });
    }
    if let Some(url) = source.strip_prefix("http+") {
        validate_network_url(url, "HTTP_REGISTRY_UNSUPPORTED_URL")?;
        return Ok(RegistrySourceConfig::Http {
            name,
            priority,
            url: url.to_string(),
        });
    }
    if source.starts_with("https://") || source.starts_with("http://") {
        validate_network_url(&source, "PACK_SOURCE_UNSUPPORTED_URL")?;
        if is_archive_url(&source) {
            if let Some(digest) = &sha256 {
                validate_sha256(digest)?;
            }
            return Ok(RegistrySourceConfig::Archive {
                name,
                priority,
                url: source,
                sha256,
            });
        }
        return Ok(RegistrySourceConfig::Http {
            name,
            priority,
            url: source,
        });
    }
    if reference.is_some() || sha256.is_some() {
        return Err(RainyError::registry(
            "REGISTRY_SOURCE_INVALID",
            "--ref is valid only for git+ sources and --sha256 only for archive URLs",
        ));
    }
    Ok(RegistrySourceConfig::Local {
        name,
        priority,
        path: source,
    })
}

fn validate_registry_name(name: &str) -> RainyResult<()> {
    if !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Ok(());
    }
    Err(RainyError::registry(
        "REGISTRY_NAME_INVALID",
        "registry name must contain 1-64 ASCII letters, digits, '-' or '_'",
    ))
}

fn registry_name(source: &RegistrySourceConfig, index: usize) -> String {
    source
        .configured_name()
        .map(str::to_string)
        .unwrap_or_else(|| format!("registry-{}", index + 1))
}

fn registry_info(
    source: &RegistrySourceConfig,
    index: usize,
    lock: &config::RegistryLock,
) -> RegistryInfo {
    let name = registry_name(source, index);
    let locked = lock.registries.get(&name);
    let (source_type, source_value, requested_ref) = match source {
        RegistrySourceConfig::Local { path, .. } => ("local", path.clone(), None),
        RegistrySourceConfig::Git { url, reference, .. } => ("git", url.clone(), reference.clone()),
        RegistrySourceConfig::Http { url, .. } => ("http", url.clone(), None),
        RegistrySourceConfig::Archive { url, .. } => ("archive", url.clone(), None),
    };
    RegistryInfo {
        name,
        source_type: source_type.to_string(),
        source: source_value,
        priority: source.priority(),
        requested_ref,
        resolved_ref: locked.and_then(|locked| locked.resolved_ref.clone()),
        digest: locked.map(|locked| locked.digest.clone()),
        cache_path: locked.and_then(|locked| locked.cache_path.clone()),
        modules: locked
            .map(|locked| locked.modules.clone())
            .unwrap_or_default(),
        synced: locked.is_some(),
    }
}

fn rainy_home() -> RainyResult<PathBuf> {
    if let Some(path) = std::env::var_os("RAINY_HOME") {
        return Ok(PathBuf::from(path));
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .ok_or_else(|| {
            RainyError::registry(
                "RAINY_HOME_NOT_FOUND",
                "cannot determine Rainy home; set RAINY_HOME to an absolute directory",
            )
        })?;
    Ok(PathBuf::from(home).join(".rainy"))
}

fn registry_source_identity(source: &RegistrySourceConfig) -> String {
    match source {
        RegistrySourceConfig::Local { path, .. } => format!("local:{path}"),
        RegistrySourceConfig::Git { url, .. } => format!("git:{url}"),
        RegistrySourceConfig::Http { url, .. } => format!("http:{url}"),
        RegistrySourceConfig::Archive { url, .. } => format!("archive:{url}"),
    }
}

fn registry_cache_path(name: &str, source: &RegistrySourceConfig) -> RainyResult<PathBuf> {
    registry_cache_path_for_identity(name, &registry_source_identity(source))
}

fn registry_cache_path_for_local(name: &str, source: &Path) -> RainyResult<PathBuf> {
    registry_cache_path_for_identity(name, &format!("local:{}", source.to_string_lossy()))
}

fn registry_cache_path_for_identity(name: &str, identity: &str) -> RainyResult<PathBuf> {
    let digest = hex(&Sha256::digest(identity.as_bytes()));
    Ok(rainy_home()?
        .join("registries")
        .join(name)
        .join(&digest[..12]))
}

fn source_path(
    workspace: &Path,
    source: &RegistrySourceConfig,
    index: usize,
) -> RainyResult<PathBuf> {
    match source {
        RegistrySourceConfig::Local { path, .. } => {
            let lock = config::load_registry_lock(workspace)?;
            if let Some(cache_path) = lock
                .registries
                .get(&registry_name(source, index))
                .and_then(|locked| locked.cache_path.as_deref())
            {
                return Ok(PathBuf::from(cache_path));
            }
            let path = PathBuf::from(path);
            if path.is_absolute() {
                Ok(path)
            } else {
                Ok(workspace.join(path))
            }
        }
        _ => registry_cache_path(&registry_name(source, index), source),
    }
}

fn locked_registry_path(
    workspace: &Path,
    source: &RegistrySourceConfig,
    locked: &config::LockedRegistry,
) -> RainyResult<PathBuf> {
    if let Some(path) = &locked.cache_path {
        return Ok(PathBuf::from(path));
    }
    match source {
        RegistrySourceConfig::Local { path, .. } => Ok(normalize_path(workspace, path)),
        _ => Err(RainyError::registry(
            "REGISTRY_CACHE_NOT_FOUND",
            "synchronized remote registry has no cache path",
        )),
    }
}

fn sync_registry_source(
    workspace: &Path,
    name: &str,
    source: &RegistrySourceConfig,
    modules: &[String],
) -> RainyResult<config::LockedRegistry> {
    let (source_type, source_value, requested_ref, resolved_ref, root) = match source {
        RegistrySourceConfig::Local { path, .. } => {
            let root = normalize_path(workspace, path)
                .canonicalize()
                .map_err(|err| {
                    RainyError::registry(
                        "PACK_SOURCE_NOT_FOUND",
                        format!("registry source not found: {path} ({err})"),
                    )
                })?;
            validate_pack_source(&root)?;
            validate_selected_modules(&root, modules)?;
            let root = if modules.is_empty() {
                root
            } else {
                sync_local_registry(name, &root, modules)?
            };
            ("local", path.clone(), None, None, root)
        }
        RegistrySourceConfig::Git { url, reference, .. } => {
            let (root, resolved) =
                sync_git_registry(name, source, url, reference.as_deref(), modules)?;
            ("git", url.clone(), reference.clone(), Some(resolved), root)
        }
        RegistrySourceConfig::Http { url, .. } => {
            let root = sync_http_registry_modules(name, source, url, modules)?;
            ("http", url.clone(), None, None, root)
        }
        RegistrySourceConfig::Archive { url, sha256, .. } => {
            let (root, digest) =
                sync_archive_registry(name, source, url, sha256.as_deref(), modules)?;
            (
                "archive",
                url.clone(),
                None,
                Some(format!("sha256:{digest}")),
                root,
            )
        }
    };
    let discovered = discover_pack_modules(&root)?;
    Ok(config::LockedRegistry {
        source_type: source_type.to_string(),
        source: source_value,
        requested_ref,
        resolved_ref,
        digest: registry_digest(&root)?,
        cache_path: if source_type == "local" && modules.is_empty() {
            None
        } else {
            Some(root.to_string_lossy().to_string())
        },
        all_modules: modules.is_empty(),
        modules: discovered,
        installed_skills: Vec::new(),
        synced_at: chrono::Utc::now(),
    })
}

fn sync_git_registry(
    name: &str,
    source: &RegistrySourceConfig,
    url: &str,
    reference: Option<&str>,
    modules: &[String],
) -> RainyResult<(PathBuf, String)> {
    validate_git_url(url)?;
    let target = registry_cache_path(name, source)?;
    let _cache_lock = lock_registry_cache(&target)?;
    let staging = staging_path(&target);
    reset_staging(&staging)?;
    if let Some(parent) = staging.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut command = std::process::Command::new("git");
    command.args(["clone", "--depth", "1", "--no-tags"]);
    if let Some(reference) = reference {
        command.args(["--branch", reference]);
    }
    let output = command.arg(url).arg(&staging).output()?;
    if !output.status.success() {
        let Some(reference) = reference else {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(RainyError::registry(
                "REGISTRY_GIT_FETCH_FAILED",
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ));
        };
        reset_staging(&staging)?;
        std::fs::create_dir_all(&staging)?;
        run_git(&staging, &["init", "--quiet"])?;
        run_git(&staging, &["remote", "add", "origin", url])?;
        run_git(
            &staging,
            &["fetch", "--depth", "1", "--no-tags", "origin", reference],
        )?;
        run_git(&staging, &["checkout", "--quiet", "--detach", "FETCH_HEAD"])?;
    }
    let resolved = std::process::Command::new("git")
        .args(["-C"])
        .arg(&staging)
        .args(["rev-parse", "HEAD"])
        .output()?;
    if !resolved.status.success() {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(RainyError::registry(
            "REGISTRY_GIT_REF_INVALID",
            String::from_utf8_lossy(&resolved.stderr).trim().to_string(),
        ));
    }
    reject_unsafe_tree_entries(&staging)?;
    filter_pack_modules(&staging, modules)?;
    validate_pack_source(&staging)?;
    atomic_replace_directory(&staging, &target)?;
    Ok((
        target,
        String::from_utf8_lossy(&resolved.stdout).trim().to_string(),
    ))
}

fn run_git(repository: &Path, args: &[&str]) -> RainyResult<()> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(args)
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(RainyError::registry(
            "REGISTRY_GIT_FETCH_FAILED",
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ))
    }
}

fn reject_unsafe_tree_entries(root: &Path) -> RainyResult<()> {
    for entry in walkdir::WalkDir::new(root).follow_links(false) {
        let entry = entry.map_err(|err| {
            RainyError::registry(
                "REGISTRY_SOURCE_UNSAFE_ENTRY",
                format!("cannot inspect registry tree: {err}"),
            )
        })?;
        if entry.path().strip_prefix(root).is_ok_and(|relative| {
            relative
                .components()
                .next()
                .is_some_and(|component| component.as_os_str() == ".git")
        }) {
            continue;
        }
        if entry.file_type().is_symlink() {
            return Err(RainyError::registry(
                "REGISTRY_SOURCE_UNSAFE_ENTRY",
                format!("symbolic links are not allowed: {}", entry.path().display()),
            ));
        }
    }
    Ok(())
}

fn sync_local_registry(name: &str, source: &Path, modules: &[String]) -> RainyResult<PathBuf> {
    let target = registry_cache_path_for_local(name, source)?;
    let _cache_lock = lock_registry_cache(&target)?;
    let staging = staging_path(&target);
    reset_staging(&staging)?;
    std::fs::create_dir_all(&staging)?;
    if source.join("pack.yaml").is_file() {
        copy_directory_secure(source, &staging)?;
    } else {
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() || !entry.path().join("pack.yaml").is_file() {
                continue;
            }
            let pack: CapabilityPack =
                serde_yaml::from_str(&std::fs::read_to_string(entry.path().join("pack.yaml"))?)?;
            if modules.contains(&pack.metadata.name) {
                copy_directory_secure(&entry.path(), &staging.join(entry.file_name()))?;
            }
        }
    }
    validate_pack_source(&staging)?;
    atomic_replace_directory(&staging, &target)?;
    Ok(target)
}

fn sync_archive_registry(
    name: &str,
    source: &RegistrySourceConfig,
    url: &str,
    configured_sha256: Option<&str>,
    modules: &[String],
) -> RainyResult<(PathBuf, String)> {
    let bytes = http_get_bytes(url, MAX_ARCHIVE_BYTES)?;
    let actual = hex(&Sha256::digest(&bytes));
    let expected = match configured_sha256 {
        Some(value) => value.to_string(),
        None => http_get(&format!("{url}.sha256"))?
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_string(),
    };
    validate_sha256(&expected)?;
    if !actual.eq_ignore_ascii_case(&expected) {
        return Err(RainyError::registry(
            "REGISTRY_ARCHIVE_CHECKSUM_INVALID",
            format!("archive checksum mismatch: expected {expected}, got {actual}"),
        ));
    }
    let target = registry_cache_path(name, source)?;
    let _cache_lock = lock_registry_cache(&target)?;
    let staging = staging_path(&target);
    reset_staging(&staging)?;
    std::fs::create_dir_all(&staging)?;
    if url_path(url).ends_with(".zip") {
        extract_zip(&bytes, &staging)?;
    } else {
        extract_tar_gz(&bytes, &staging)?;
    }
    let content_root = normalize_extracted_root(&staging)?;
    filter_pack_modules(&content_root, modules)?;
    validate_pack_source(&content_root)?;
    if content_root != staging {
        let promoted = staging.with_extension("promoted");
        if promoted.exists() {
            std::fs::remove_dir_all(&promoted)?;
        }
        std::fs::rename(&content_root, &promoted)?;
        std::fs::remove_dir_all(&staging)?;
        atomic_replace_directory(&promoted, &target)?;
    } else {
        atomic_replace_directory(&staging, &target)?;
    }
    Ok((target, actual))
}

fn is_archive_url(url: &str) -> bool {
    let path = url_path(url);
    path.ends_with(".tar.gz") || path.ends_with(".tgz") || path.ends_with(".zip")
}

fn url_path(url: &str) -> String {
    url.split(['?', '#'])
        .next()
        .unwrap_or(url)
        .to_ascii_lowercase()
}

fn extract_tar_gz(bytes: &[u8], target: &Path) -> RainyResult<()> {
    let decoder = flate2::read::GzDecoder::new(Cursor::new(bytes));
    let mut archive = tar::Archive::new(decoder);
    let mut count = 0_usize;
    let mut total = 0_u64;
    for entry in archive.entries().map_err(|err| {
        RainyError::registry("REGISTRY_ARCHIVE_INVALID", format!("invalid tar.gz: {err}"))
    })? {
        let mut entry = entry.map_err(|err| {
            RainyError::registry(
                "REGISTRY_ARCHIVE_INVALID",
                format!("invalid tar entry: {err}"),
            )
        })?;
        count += 1;
        if count > MAX_ARCHIVE_ENTRIES {
            return Err(RainyError::registry(
                "REGISTRY_ARCHIVE_LIMIT_EXCEEDED",
                "archive contains too many entries",
            ));
        }
        let path = entry
            .path()
            .map_err(|err| {
                RainyError::registry("REGISTRY_ARCHIVE_INVALID", format!("invalid path: {err}"))
            })?
            .into_owned();
        validate_archive_path(&path)?;
        let entry_type = entry.header().entry_type();
        if !(entry_type.is_file() || entry_type.is_dir()) {
            return Err(RainyError::registry(
                "REGISTRY_ARCHIVE_UNSAFE_ENTRY",
                format!(
                    "links and special files are not allowed: {}",
                    path.display()
                ),
            ));
        }
        total = total.saturating_add(entry.size());
        if total > MAX_EXTRACTED_BYTES {
            return Err(RainyError::registry(
                "REGISTRY_ARCHIVE_LIMIT_EXCEEDED",
                "archive expands beyond the 512 MiB limit",
            ));
        }
        entry.unpack_in(target).map_err(|err| {
            RainyError::registry(
                "REGISTRY_ARCHIVE_INVALID",
                format!("cannot extract {}: {err}", path.display()),
            )
        })?;
    }
    Ok(())
}

fn extract_zip(bytes: &[u8], target: &Path) -> RainyResult<()> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|err| {
        RainyError::registry("REGISTRY_ARCHIVE_INVALID", format!("invalid zip: {err}"))
    })?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(RainyError::registry(
            "REGISTRY_ARCHIVE_LIMIT_EXCEEDED",
            "archive contains too many entries",
        ));
    }
    let mut total = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|err| {
            RainyError::registry(
                "REGISTRY_ARCHIVE_INVALID",
                format!("invalid zip entry: {err}"),
            )
        })?;
        let path = entry.enclosed_name().ok_or_else(|| {
            RainyError::registry(
                "REGISTRY_ARCHIVE_UNSAFE_ENTRY",
                format!("unsafe zip path: {}", entry.name()),
            )
        })?;
        validate_archive_path(&path)?;
        if entry.is_symlink() {
            return Err(RainyError::registry(
                "REGISTRY_ARCHIVE_UNSAFE_ENTRY",
                format!("symbolic links are not allowed: {}", path.display()),
            ));
        }
        total = total.saturating_add(entry.size());
        if total > MAX_EXTRACTED_BYTES {
            return Err(RainyError::registry(
                "REGISTRY_ARCHIVE_LIMIT_EXCEEDED",
                "archive expands beyond the 512 MiB limit",
            ));
        }
        let output = target.join(path);
        if entry.is_dir() {
            std::fs::create_dir_all(&output)?;
            continue;
        }
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::File::create(output)?;
        std::io::copy(&mut entry, &mut file)?;
    }
    Ok(())
}

fn validate_archive_path(path: &Path) -> RainyResult<()> {
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(RainyError::registry(
            "REGISTRY_ARCHIVE_UNSAFE_ENTRY",
            format!("archive path escapes the target: {}", path.display()),
        ));
    }
    Ok(())
}

fn normalize_extracted_root(staging: &Path) -> RainyResult<PathBuf> {
    if contains_pack(staging)? {
        return Ok(staging.to_path_buf());
    }
    let entries = std::fs::read_dir(staging)?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .collect::<Vec<_>>();
    if entries.len() == 1 && contains_pack(&entries[0].path())? {
        return Ok(entries[0].path());
    }
    Err(RainyError::registry(
        "PACK_SOURCE_INVALID",
        "archive must contain pack.yaml or one directory containing pack modules",
    ))
}

fn contains_pack(root: &Path) -> RainyResult<bool> {
    if root.join("pack.yaml").is_file() {
        return Ok(true);
    }
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() && entry.path().join("pack.yaml").is_file() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn discover_pack_modules(root: &Path) -> RainyResult<Vec<String>> {
    if root.join("pack.yaml").is_file() {
        let pack: CapabilityPack =
            serde_yaml::from_str(&std::fs::read_to_string(root.join("pack.yaml"))?)?;
        return Ok(vec![pack.metadata.name]);
    }
    let mut modules = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() || !entry.path().join("pack.yaml").is_file() {
            continue;
        }
        let pack: CapabilityPack =
            serde_yaml::from_str(&std::fs::read_to_string(entry.path().join("pack.yaml"))?)?;
        modules.push(pack.metadata.name);
    }
    modules.sort();
    Ok(modules)
}

fn validate_selected_modules(root: &Path, selected: &[String]) -> RainyResult<()> {
    if selected.is_empty() {
        return Ok(());
    }
    let available = discover_pack_modules(root)?;
    let missing = selected
        .iter()
        .filter(|module| !available.contains(module))
        .cloned()
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(RainyError::registry(
            "REGISTRY_MODULE_NOT_FOUND",
            format!(
                "registry modules not found: {}; available: {}",
                missing.join(", "),
                available.join(", ")
            ),
        ))
    }
}

fn filter_pack_modules(root: &Path, selected: &[String]) -> RainyResult<()> {
    validate_selected_modules(root, selected)?;
    if selected.is_empty() || root.join("pack.yaml").is_file() {
        return Ok(());
    }
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() || !entry.path().join("pack.yaml").is_file() {
            continue;
        }
        let pack: CapabilityPack =
            serde_yaml::from_str(&std::fs::read_to_string(entry.path().join("pack.yaml"))?)?;
        if !selected.contains(&pack.metadata.name) {
            std::fs::remove_dir_all(entry.path())?;
        }
    }
    Ok(())
}

fn copy_directory_secure(source: &Path, target: &Path) -> RainyResult<()> {
    std::fs::create_dir_all(target)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let destination = target.join(entry.file_name());
        if file_type.is_symlink() {
            return Err(RainyError::registry(
                "REGISTRY_SOURCE_UNSAFE_ENTRY",
                format!("symbolic links are not allowed: {}", entry.path().display()),
            ));
        }
        if file_type.is_dir() {
            copy_directory_secure(&entry.path(), &destination)?;
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), destination)?;
        } else {
            return Err(RainyError::registry(
                "REGISTRY_SOURCE_UNSAFE_ENTRY",
                format!("special files are not allowed: {}", entry.path().display()),
            ));
        }
    }
    Ok(())
}

fn install_registry_skills(
    workspace: &Path,
    registry_root: &Path,
    targets: &[SkillTarget],
    previous: &[config::InstalledRegistrySkill],
    force: bool,
) -> RainyResult<Vec<config::InstalledRegistrySkill>> {
    let target_names = targets
        .iter()
        .map(skill_target_name)
        .map(str::to_string)
        .collect::<Vec<_>>();
    install_registry_skills_for_targets(workspace, registry_root, &target_names, previous, force)
}

fn install_registry_skills_for_targets(
    workspace: &Path,
    registry_root: &Path,
    targets: &[String],
    previous: &[config::InstalledRegistrySkill],
    force: bool,
) -> RainyResult<Vec<config::InstalledRegistrySkill>> {
    let mut exports = Vec::new();
    for pack_root in pack_roots(registry_root)? {
        let pack: CapabilityPack =
            serde_yaml::from_str(&std::fs::read_to_string(pack_root.join("pack.yaml"))?)?;
        for relative in pack.exports.skills {
            validate_relative_registry_file(&relative)?;
            let source = pack_root.join(&relative);
            if !source.is_dir() || !source.join("SKILL.md").is_file() {
                return Err(RainyError::registry(
                    "REGISTRY_SKILL_INVALID",
                    format!(
                        "Skill export must be a directory containing SKILL.md: {}/{}",
                        pack.metadata.name, relative
                    ),
                ));
            }
            let id = source
                .file_name()
                .and_then(|name| name.to_str())
                .filter(|name| !name.is_empty())
                .ok_or_else(|| {
                    RainyError::registry(
                        "REGISTRY_SKILL_INVALID",
                        format!("Skill export has no valid directory name: {relative}"),
                    )
                })?
                .to_string();
            exports.push((id, source));
        }
    }

    let mut planned = Vec::new();
    let mut destinations = BTreeSet::new();
    for target in targets {
        let root = registry_skill_target_root(target)?;
        for (id, source) in &exports {
            let relative = root.join(id);
            let relative_text = relative.to_string_lossy().replace('\\', "/");
            if !destinations.insert(relative_text.clone()) {
                continue;
            }
            let destination = workspace.join(&relative);
            let incoming_digest = registry_digest(source)?;
            if destination.exists() {
                let existing_digest = registry_digest(&destination)?;
                if existing_digest != incoming_digest {
                    let managed_digest = previous
                        .iter()
                        .find(|item| item.path == relative_text)
                        .map(|item| item.digest.as_str());
                    if managed_digest != Some(existing_digest.as_str()) && !force {
                        return Err(RainyError::registry(
                            "REGISTRY_SKILL_CONFLICT",
                            format!(
                                "Skill {} has local changes at {}; review them and rerun with --force to replace",
                                id,
                                destination.display()
                            ),
                        ));
                    }
                }
            }
            planned.push((
                config::InstalledRegistrySkill {
                    id: id.clone(),
                    target: target.clone(),
                    path: relative_text,
                    digest: incoming_digest,
                },
                source.clone(),
                destination,
            ));
        }
    }

    let mut policy_changes = ChangeSet::new();
    for (item, _, destination) in &planned {
        policy_changes.push(crate::patch::Change {
            kind: if destination.exists() {
                crate::patch::ChangeKind::Modify
            } else {
                crate::patch::ChangeKind::Create
            },
            path: item.path.clone(),
            before: None,
            after: Some(format!("enterprise Skill {}", item.id)),
            summary: format!("install enterprise Skill {}", item.id),
            noop: destination.exists()
                && registry_digest(destination).is_ok_and(|digest| digest == item.digest),
        });
    }
    policy::check_changes(workspace, &policy_changes)?;

    for (item, source, destination) in &planned {
        if destination.exists()
            && registry_digest(destination).is_ok_and(|digest| digest == item.digest)
        {
            continue;
        }
        let staging = staging_path(destination);
        reset_staging(&staging)?;
        copy_directory_secure(source, &staging)?;
        atomic_replace_directory(&staging, destination)?;
    }
    Ok(planned.into_iter().map(|(item, _, _)| item).collect())
}

fn pack_roots(root: &Path) -> RainyResult<Vec<PathBuf>> {
    if root.join("pack.yaml").is_file() {
        return Ok(vec![root.to_path_buf()]);
    }
    let mut roots = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() && entry.path().join("pack.yaml").is_file() {
            roots.push(entry.path());
        }
    }
    roots.sort();
    Ok(roots)
}

fn skill_target_name(target: &SkillTarget) -> &'static str {
    match target {
        SkillTarget::Universal => "universal",
        SkillTarget::Codex => "codex",
        SkillTarget::Claude => "claude",
        SkillTarget::Cursor => "cursor",
        SkillTarget::GithubCopilot => "github-copilot",
        SkillTarget::Gemini => "gemini",
        SkillTarget::Opencode => "opencode",
    }
}

fn registry_skill_target_root(target: &str) -> RainyResult<&'static Path> {
    match target {
        "universal" | "codex" => Ok(Path::new(".agents/skills")),
        "claude" => Ok(Path::new(".claude/skills")),
        "cursor" => Ok(Path::new(".cursor/skills")),
        "github-copilot" => Ok(Path::new(".github/skills")),
        "gemini" => Ok(Path::new(".gemini/skills")),
        "opencode" => Ok(Path::new(".opencode/skills")),
        _ => Err(RainyError::registry(
            "REGISTRY_SKILL_TARGET_UNSUPPORTED",
            format!("unsupported Skill target: {target}"),
        )),
    }
}

fn registry_digest(root: &Path) -> RainyResult<String> {
    let mut files = walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| {
            RainyError::registry(
                "REGISTRY_DIGEST_FAILED",
                format!("cannot inspect registry files: {err}"),
            )
        })?;
    files.retain(|entry| {
        entry.file_type().is_file()
            && !entry
                .path()
                .strip_prefix(root)
                .unwrap_or(entry.path())
                .components()
                .any(|component| component.as_os_str() == ".git")
    });
    files.sort_by(|left, right| left.path().cmp(right.path()));
    let mut digest = Sha256::new();
    for entry in files {
        let relative = entry.path().strip_prefix(root).unwrap_or(entry.path());
        digest.update(relative.to_string_lossy().as_bytes());
        digest.update(b"\0");
        digest.update(std::fs::read(entry.path())?);
        digest.update(b"\0");
    }
    Ok(hex(&digest.finalize()))
}

fn staging_path(target: &Path) -> PathBuf {
    target.with_extension(format!("tmp.{}", std::process::id()))
}

fn reset_staging(staging: &Path) -> RainyResult<()> {
    if staging.exists() {
        std::fs::remove_dir_all(staging)?;
    }
    Ok(())
}

fn atomic_replace_directory(staging: &Path, target: &Path) -> RainyResult<()> {
    let backup = target.with_extension(format!("backup.{}", std::process::id()));
    if backup.exists() {
        std::fs::remove_dir_all(&backup)?;
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if target.exists() {
        std::fs::rename(target, &backup)?;
    }
    if let Err(error) = std::fs::rename(staging, target) {
        if backup.exists() {
            let _ = std::fs::rename(&backup, target);
        }
        return Err(error.into());
    }
    if backup.exists() {
        std::fs::remove_dir_all(backup)?;
    }
    Ok(())
}

struct RegistryCacheLock(File);

impl Drop for RegistryCacheLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.0);
    }
}

fn lock_registry_cache(target: &Path) -> RainyResult<RegistryCacheLock> {
    let parent = target.parent().ok_or_else(|| {
        RainyError::registry(
            "REGISTRY_CACHE_INVALID",
            format!("registry cache has no parent: {}", target.display()),
        )
    })?;
    std::fs::create_dir_all(parent)?;
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(target.with_extension("lock"))?;
    file.lock_exclusive()?;
    Ok(RegistryCacheLock(file))
}

fn http_get_bytes(url: &str, limit: u64) -> RainyResult<Vec<u8>> {
    validate_network_url(url, "REGISTRY_ARCHIVE_UNSUPPORTED_URL")?;
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout_read(Duration::from_secs(30))
        .timeout_write(Duration::from_secs(10))
        .redirects(3)
        .build();
    let response = agent
        .get(url)
        .set("User-Agent", "rainy-cli")
        .call()
        .map_err(|err| {
            RainyError::registry(
                "REGISTRY_ARCHIVE_FETCH_FAILED",
                format!("request failed: {err}"),
            )
        })?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|err| RainyError::registry("REGISTRY_ARCHIVE_FETCH_FAILED", err.to_string()))?;
    if bytes.len() as u64 > limit {
        return Err(RainyError::registry(
            "REGISTRY_ARCHIVE_RESPONSE_TOO_LARGE",
            format!("archive exceeds the {} MiB limit", limit / 1024 / 1024),
        ));
    }
    Ok(bytes)
}

#[derive(Debug, Clone)]
pub struct RegistryClient {
    packs: Vec<CapabilityPack>,
    capabilities: BTreeMap<String, CapabilityDefinition>,
}

impl RegistryClient {
    pub fn load(workspace: &Path) -> RainyResult<Self> {
        let config = config::load_config(workspace).ok();
        let sources = registry_sources(workspace, config.as_ref())?;
        let mut packs = Vec::new();
        let mut capabilities = BTreeMap::new();

        for source in sources {
            if source.exists() {
                load_packs_from_dir(&source, &mut packs, &mut capabilities)?;
            }
        }

        if capabilities.is_empty() {
            return Err(RainyError::registry(
                "REGISTRY_EMPTY",
                "no local capability packs found",
            ));
        }

        Ok(Self {
            packs,
            capabilities,
        })
    }

    pub fn get_capability(&self, id: &str) -> RainyResult<CapabilityDefinition> {
        self.capabilities.get(id).cloned().ok_or_else(|| {
            RainyError::registry(
                "CAPABILITY_NOT_FOUND",
                format!("capability not found: {id}"),
            )
        })
    }

    pub fn capabilities(&self) -> Vec<CapabilityDefinition> {
        self.capabilities.values().cloned().collect()
    }
}

fn registry_sources(workspace: &Path, config: Option<&ProjectConfig>) -> RainyResult<Vec<PathBuf>> {
    let mut sources = Vec::new();
    if let Some(config) = config {
        let mut configured = config
            .capability_registry
            .sources
            .iter()
            .enumerate()
            .collect::<Vec<_>>();
        configured.sort_by_key(|(_, source)| std::cmp::Reverse(source.priority()));
        for (index, source) in configured {
            sources.push(source_path(workspace, source, index)?);
        }
    }
    sources.push(config::default_registry_path()?);
    let mut seen = BTreeSet::new();
    sources.retain(|source| seen.insert(source.clone()));
    Ok(sources)
}

fn load_packs_from_dir(
    source: &Path,
    packs: &mut Vec<CapabilityPack>,
    capabilities: &mut BTreeMap<String, CapabilityDefinition>,
) -> RainyResult<()> {
    if source.join("pack.yaml").exists() {
        load_pack_at(source, packs, capabilities)?;
        return Ok(());
    }

    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let root = entry.path();
        if !root.join("pack.yaml").exists() {
            continue;
        }
        load_pack_at(&root, packs, capabilities)?;
    }
    Ok(())
}

fn load_pack_at(
    root: &Path,
    packs: &mut Vec<CapabilityPack>,
    capabilities: &mut BTreeMap<String, CapabilityDefinition>,
) -> RainyResult<()> {
    if std::env::var_os("RAINY_PACK_TRUSTED_PUBLIC_KEY").is_some() {
        verify_pack_signature(root)?;
    }
    let pack_path = root.join("pack.yaml");
    let content = std::fs::read_to_string(&pack_path)?;
    let mut pack: CapabilityPack = serde_yaml::from_str(&content)?;
    pack.root = root.to_path_buf();
    for capability_path in &pack.exports.capabilities {
        let path = root.join(capability_path);
        let content = std::fs::read_to_string(&path)?;
        let mut capability: CapabilityDefinition = serde_yaml::from_str(&content)?;
        capability.pack_root = root.to_path_buf();
        capability.pack_name = pack.metadata.name.clone();
        if capabilities.contains_key(&capability.id) {
            return Err(RainyError::registry(
                "CAPABILITY_DUPLICATE",
                format!("duplicate capability id: {}", capability.id),
            ));
        }
        capabilities.insert(capability.id.clone(), capability);
    }
    packs.push(pack);
    Ok(())
}

fn install_pack(workspace: &Path, args: crate::cli::PackInstallArgs) -> RainyResult<CommandOutput> {
    let apply = resolve_apply_flags(args.dry_run, args.apply)?;
    if args.install_skills && args.target.is_empty() {
        return Err(RainyError::registry(
            "REGISTRY_SKILL_TARGET_REQUIRED",
            "--install-skills requires at least one --target",
        ));
    }
    let name = args
        .name
        .unwrap_or_else(|| generated_registry_name(&args.source));
    validate_registry_name(&name)?;
    let source = registry_source_from_input(
        Some(name.clone()),
        args.source,
        args.reference,
        args.sha256,
        0,
    )?;
    let mut config = config::load_config(workspace)?;
    config
        .capability_registry
        .sources
        .retain(|existing| existing.configured_name() != Some(name.as_str()));
    config.capability_registry.sources.push(source.clone());
    let mut lock = config::load_registry_lock(workspace)?;
    if apply {
        preflight_registry_lock_write(workspace)?;
        let previous_skills = lock
            .registries
            .get(&name)
            .map(|locked| locked.installed_skills.clone())
            .unwrap_or_default();
        let mut locked = sync_registry_source(
            workspace,
            &name,
            &source,
            if args.all || args.module.is_empty() {
                &[]
            } else {
                &args.module
            },
        )?;
        locked.installed_skills = if args.install_skills {
            let registry_root = locked_registry_path(workspace, &source, &locked)?;
            install_registry_skills(
                workspace,
                &registry_root,
                &args.target,
                &previous_skills,
                args.force,
            )?
        } else {
            previous_skills
        };
        lock.registries.insert(name.clone(), locked);
    }
    let mut changes = ChangeSet::new();
    changes.push(patch::change_for_file(
        workspace,
        "rainy.yaml",
        config::serialize_config(&config)?,
        "install capability registry source",
    )?);
    if apply {
        changes.push(patch::change_for_file(
            workspace,
            ".rainy/registry.lock",
            config::save_registry_lock_content(&lock)?,
            "lock capability registry source",
        )?);
        policy::check_changes(workspace, &changes)?;
        patch::apply_changes(workspace, &changes)?;
    }
    Ok(CommandOutput::Registry {
        report: registry_report(
            "install",
            if apply { "applied" } else { "dry-run" },
            vec![registry_info(
                &source,
                config.capability_registry.sources.len() - 1,
                &lock,
            )],
            Vec::new(),
        ),
    })
}

fn resolve_apply_flags(dry_run: bool, apply: bool) -> RainyResult<bool> {
    if dry_run && apply {
        return Err(RainyError::registry(
            "APPLY_MODE_CONFLICT",
            "--dry-run and --apply cannot be used together",
        ));
    }
    Ok(apply)
}

fn preflight_registry_lock_write(workspace: &Path) -> RainyResult<()> {
    let path = workspace.join(".rainy/registry.lock");
    let before = if path.exists() {
        Some(std::fs::read_to_string(&path)?)
    } else {
        None
    };
    let mut changes = ChangeSet::new();
    changes.push(crate::patch::Change {
        kind: if before.is_some() {
            crate::patch::ChangeKind::Modify
        } else {
            crate::patch::ChangeKind::Create
        },
        path: ".rainy/registry.lock".to_string(),
        before,
        after: Some("lockfileVersion: 1\nregistries: {}\n".to_string()),
        summary: "preflight registry lock write".to_string(),
        noop: false,
    });
    policy::check_changes(workspace, &changes)
}

fn update_packs(workspace: &Path, args: crate::cli::PackUpdateArgs) -> RainyResult<CommandOutput> {
    let apply = resolve_apply_flags(args.dry_run, args.apply)?;
    if !apply {
        return Ok(CommandOutput::change_dry_run(
            "pack update",
            ChangeSet::new(),
        ));
    }
    let project = config::load_config(workspace)?;
    let previous = config::load_registry_lock(workspace)?;
    preflight_registry_lock_write(workspace)?;
    let mut next = previous.clone();
    let mut infos = Vec::new();
    for (index, source) in project.capability_registry.sources.iter().enumerate() {
        let name = registry_name(source, index);
        let modules = previous
            .registries
            .get(&name)
            .filter(|locked| !locked.all_modules)
            .map(|locked| locked.modules.as_slice())
            .unwrap_or(&[]);
        let previous_skills = previous
            .registries
            .get(&name)
            .map(|locked| locked.installed_skills.clone())
            .unwrap_or_default();
        let mut locked = sync_registry_source(workspace, &name, source, modules)?;
        if !previous_skills.is_empty() {
            let targets = previous_skills
                .iter()
                .map(|skill| skill.target.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            locked.installed_skills = install_registry_skills_for_targets(
                workspace,
                &locked_registry_path(workspace, source, &locked)?,
                &targets,
                &previous_skills,
                false,
            )?;
        }
        next.registries.insert(name.clone(), locked);
        infos.push(registry_info(source, index, &next));
    }
    let mut changes = ChangeSet::new();
    changes.push(patch::change_for_file(
        workspace,
        ".rainy/registry.lock",
        config::save_registry_lock_content(&next)?,
        "update registry locks",
    )?);
    policy::check_changes(workspace, &changes)?;
    patch::apply_changes(workspace, &changes)?;
    Ok(CommandOutput::Registry {
        report: registry_report("update", "applied", infos, Vec::new()),
    })
}

fn sign_pack(path: &Path) -> RainyResult<CommandOutput> {
    validate_pack_source(path)?;
    let signature = calculate_pack_signature(path)?;
    let signature_path = path.join(".rainy-pack-signature.json");
    std::fs::write(
        &signature_path,
        format!("{}\n", serde_json::to_string_pretty(&signature)?),
    )?;
    let publisher_signed = if let Some(key) = std::env::var_os("RAINY_PACK_SIGNING_KEY") {
        let detached_signature = path.join(".rainy-pack-signature.sig");
        let output = std::process::Command::new("cosign")
            .args(["sign-blob", "--yes", "--key"])
            .arg(key)
            .arg("--output-signature")
            .arg(&detached_signature)
            .arg(&signature_path)
            .output()
            .map_err(|err| {
                RainyError::registry(
                    "PACK_PUBLISHER_SIGNING_FAILED",
                    format!("failed to run cosign: {err}"),
                )
            })?;
        if !output.status.success() {
            return Err(RainyError::registry(
                "PACK_PUBLISHER_SIGNING_FAILED",
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }
        true
    } else {
        false
    };
    Ok(CommandOutput::message(format!(
        "Created pack integrity manifest for {} with sha256 {}{}",
        path.display(),
        signature.digest,
        if publisher_signed {
            " and a cosign publisher signature"
        } else {
            ""
        }
    )))
}

fn verify_pack_signature(path: &Path) -> RainyResult<CommandOutput> {
    let signature_path = path.join(".rainy-pack-signature.json");
    if !signature_path.exists() {
        return Err(RainyError::registry(
            "PACK_SIGNATURE_NOT_FOUND",
            format!("signature not found: {}", signature_path.display()),
        ));
    }
    let expected: PackSignature = serde_json::from_str(&std::fs::read_to_string(&signature_path)?)?;
    let actual = calculate_pack_signature(path)?;
    if expected.digest != actual.digest || expected.files.len() != actual.files.len() {
        return Err(RainyError::registry(
            "PACK_SIGNATURE_INVALID",
            format!(
                "pack signature mismatch: expected {}, actual {}",
                expected.digest, actual.digest
            ),
        ));
    }
    for (expected_file, actual_file) in expected.files.iter().zip(actual.files.iter()) {
        if expected_file.path != actual_file.path || expected_file.digest != actual_file.digest {
            return Err(RainyError::registry(
                "PACK_SIGNATURE_INVALID",
                format!("pack signature mismatch at {}", expected_file.path),
            ));
        }
    }
    let publisher_verified = if let Some(key) = std::env::var_os("RAINY_PACK_TRUSTED_PUBLIC_KEY") {
        let detached_signature = path.join(".rainy-pack-signature.sig");
        if !detached_signature.exists() {
            return Err(RainyError::registry(
                "PACK_PUBLISHER_SIGNATURE_NOT_FOUND",
                format!(
                    "publisher signature not found: {}",
                    detached_signature.display()
                ),
            ));
        }
        let output = std::process::Command::new("cosign")
            .args(["verify-blob", "--key"])
            .arg(key)
            .arg("--signature")
            .arg(&detached_signature)
            .arg(&signature_path)
            .output()
            .map_err(|err| {
                RainyError::registry(
                    "PACK_PUBLISHER_SIGNATURE_INVALID",
                    format!("failed to run cosign: {err}"),
                )
            })?;
        if !output.status.success() {
            return Err(RainyError::registry(
                "PACK_PUBLISHER_SIGNATURE_INVALID",
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }
        true
    } else {
        false
    };
    Ok(CommandOutput::message(format!(
        "Pack integrity verified: {}{}",
        expected.digest,
        if publisher_verified {
            " with trusted publisher signature"
        } else {
            ""
        }
    )))
}

fn calculate_pack_signature(path: &Path) -> RainyResult<PackSignature> {
    let mut files = Vec::new();
    collect_pack_files(path, path, &mut files)?;
    files.sort_by(|left, right| left.path.cmp(&right.path));
    let mut root = Sha256::new();
    for file in &files {
        root.update(file.path.as_bytes());
        root.update(b"\0");
        root.update(file.digest.as_bytes());
        root.update(b"\0");
    }
    Ok(PackSignature {
        protocol_version: "rainy.pack-signature.v1".to_string(),
        algorithm: "sha256".to_string(),
        digest: hex(&root.finalize()),
        signed_at: chrono::Utc::now().to_rfc3339(),
        files,
    })
}

pub fn pack_digest(path: &Path) -> RainyResult<String> {
    Ok(calculate_pack_signature(path)?.digest)
}

fn collect_pack_files(
    root: &Path,
    current: &Path,
    files: &mut Vec<PackFileDigest>,
) -> RainyResult<()> {
    for entry in std::fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            collect_pack_files(root, &path, files)?;
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if matches!(
            rel.as_str(),
            ".rainy-pack-signature.json" | ".rainy-pack-signature.sig"
        ) {
            continue;
        }
        let bytes = std::fs::read(&path)?;
        let mut digest = Sha256::new();
        digest.update(&bytes);
        files.push(PackFileDigest {
            path: rel,
            digest: hex(&digest.finalize()),
        });
    }
    Ok(())
}

fn sync_http_registry_modules(
    name: &str,
    source: &RegistrySourceConfig,
    url: &str,
    modules: &[String],
) -> RainyResult<PathBuf> {
    let index_text = http_get(url)?;
    let index: HttpRegistryIndex = serde_yaml::from_str(&index_text)?;
    if index.protocol_version != "rainy.registry.v1" {
        return Err(RainyError::registry(
            "HTTP_REGISTRY_INVALID",
            format!("unsupported registry protocol: {}", index.protocol_version),
        ));
    }
    let available = index
        .packs
        .iter()
        .map(|pack| pack.name.clone())
        .collect::<Vec<_>>();
    let missing = modules
        .iter()
        .filter(|module| !available.contains(module))
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(RainyError::registry(
            "REGISTRY_MODULE_NOT_FOUND",
            format!(
                "registry modules not found: {}; available: {}",
                missing.join(", "),
                available.join(", ")
            ),
        ));
    }
    let cache_root = registry_cache_path(name, source)?;
    let _cache_lock = lock_registry_cache(&cache_root)?;
    let temporary = cache_root.with_extension(format!("tmp.{}", std::process::id()));
    let backup = cache_root.with_extension(format!("backup.{}", std::process::id()));
    if temporary.exists() {
        std::fs::remove_dir_all(&temporary)?;
    }
    if let Some(parent) = cache_root.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::create_dir_all(&temporary)?;
    let download_result = (|| -> RainyResult<()> {
        for pack in index
            .packs
            .into_iter()
            .filter(|pack| modules.is_empty() || modules.contains(&pack.name))
        {
            let pack_root = temporary.join(&pack.name);
            std::fs::create_dir_all(&pack_root)?;
            for file in &pack.files {
                validate_relative_registry_file(file)?;
                let expected = pack.digests.get(file).ok_or_else(|| {
                    RainyError::registry(
                        "HTTP_REGISTRY_CHECKSUM_MISSING",
                        format!("registry pack {} has no checksum for {file}", pack.name),
                    )
                })?;
                validate_sha256(expected)?;
                let file_url = join_url(&pack.base_url, file);
                let content = http_get(&file_url)?;
                let actual = hex(&Sha256::digest(content.as_bytes()));
                if !actual.eq_ignore_ascii_case(expected) {
                    return Err(RainyError::registry(
                        "HTTP_REGISTRY_CHECKSUM_INVALID",
                        format!("registry checksum mismatch for {}/{file}", pack.name),
                    ));
                }
                let target = pack_root.join(file);
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(target, content)?;
            }
            validate_pack_source(&pack_root)?;
            let manifest: CapabilityPack =
                serde_yaml::from_str(&std::fs::read_to_string(pack_root.join("pack.yaml"))?)?;
            if manifest.metadata.name != pack.name || manifest.metadata.version != pack.version {
                return Err(RainyError::registry(
                    "HTTP_REGISTRY_IDENTITY_MISMATCH",
                    format!(
                        "registry identity does not match downloaded pack {}",
                        pack.name
                    ),
                ));
            }
        }
        Ok(())
    })();
    if let Err(error) = download_result {
        let _ = std::fs::remove_dir_all(&temporary);
        return Err(error);
    }

    if backup.exists() {
        std::fs::remove_dir_all(&backup)?;
    }
    if cache_root.exists() {
        std::fs::rename(&cache_root, &backup)?;
    }
    if let Err(error) = std::fs::rename(&temporary, &cache_root) {
        if backup.exists() {
            let _ = std::fs::rename(&backup, &cache_root);
        }
        return Err(error.into());
    }
    let _ = std::fs::remove_dir_all(backup);
    Ok(cache_root)
}

fn validate_sha256(digest: &str) -> RainyResult<()> {
    if digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Ok(());
    }
    Err(RainyError::registry(
        "HTTP_REGISTRY_CHECKSUM_INVALID",
        "registry checksum must be a 64-character SHA-256 digest",
    ))
}

fn http_get(url: &str) -> RainyResult<String> {
    validate_network_url(url, "HTTP_REGISTRY_UNSUPPORTED_URL")?;
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(3))
        .timeout_read(Duration::from_secs(10))
        .timeout_write(Duration::from_secs(10))
        .redirects(3)
        .build();
    let response = agent
        .get(url)
        .set("User-Agent", "rainy-cli")
        .call()
        .map_err(|err| {
            RainyError::registry(
                "HTTP_REGISTRY_FETCH_FAILED",
                format!("request failed: {err}"),
            )
        })?;
    let mut body = String::new();
    response
        .into_reader()
        .take(MAX_REGISTRY_RESPONSE_BYTES + 1)
        .read_to_string(&mut body)
        .map_err(|err| RainyError::registry("HTTP_REGISTRY_FETCH_FAILED", err.to_string()))?;
    if body.len() as u64 > MAX_REGISTRY_RESPONSE_BYTES {
        return Err(RainyError::registry(
            "HTTP_REGISTRY_RESPONSE_TOO_LARGE",
            "registry response exceeds 5 MiB limit",
        ));
    }
    Ok(body)
}

fn validate_network_url(url: &str, code: &'static str) -> RainyResult<()> {
    if url.starts_with("https://") {
        return Ok(());
    }
    if let Some(rest) = url.strip_prefix("http://") {
        let host = rest
            .split_once('/')
            .map(|(host, _)| host)
            .unwrap_or(rest)
            .split(':')
            .next()
            .unwrap_or_default();
        if matches!(host, "127.0.0.1" | "localhost" | "::1") {
            return Ok(());
        }
    }
    Err(RainyError::registry(
        code,
        format!("only HTTPS or loopback HTTP URLs are allowed: {url}"),
    ))
}

fn validate_relative_registry_file(path: &str) -> RainyResult<()> {
    let path = Path::new(path);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(RainyError::registry(
            "HTTP_REGISTRY_INVALID",
            format!("registry file path is unsafe: {}", path.display()),
        ));
    }
    Ok(())
}

fn join_url(base: &str, file: &str) -> String {
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        file.trim_start_matches('/')
    )
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn validate_git_url(url: &str) -> RainyResult<()> {
    if url.starts_with("https://")
        || url.starts_with("ssh://")
        || url.starts_with("git@")
        || url.starts_with("file://")
    {
        return Ok(());
    }
    Err(RainyError::registry(
        "PACK_SOURCE_UNSUPPORTED_URL",
        format!("git source must use HTTPS or SSH: {url}"),
    ))
}

fn validate_pack_source(source: &Path) -> RainyResult<()> {
    if source.join("pack.yaml").exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(source).map_err(|err| {
        RainyError::registry(
            "PACK_SOURCE_INVALID",
            format!("cannot read pack source {}: {err}", source.display()),
        )
    })? {
        let entry = entry?;
        if entry.path().join("pack.yaml").exists() {
            return Ok(());
        }
    }
    Err(RainyError::registry(
        "PACK_SOURCE_INVALID",
        format!("no pack.yaml found in {}", source.display()),
    ))
}

fn normalize_path(workspace: &Path, path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else {
        workspace.join(path)
    }
}

fn slugify(input: &str) -> String {
    input
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn generated_registry_name(source: &str) -> String {
    let normalized = source
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("registry")
        .trim_end_matches(".tar.gz")
        .trim_end_matches(".tgz")
        .trim_end_matches(".zip")
        .trim_end_matches(".git");
    let name = slugify(normalized);
    let name = if name.is_empty() { "registry" } else { &name };
    let digest = hex(&Sha256::digest(source.as_bytes()));
    format!("{name}-{}", &digest[..8])
}
