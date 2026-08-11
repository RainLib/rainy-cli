use crate::cli::{
    SourceAddArgs, SourceChangeArgs, SourceCommand, SourceInspectArgs, SourceRemoveArgs,
    SourceResolveArgs, SourceSelectArgs, SourceSubcommand,
};
use crate::error::{RainyError, RainyResult};
use crate::output::CommandOutput;
use crate::progress::ProgressReporter;
use chrono::{DateTime, Utc};
use flate2::read::GzDecoder;
use fs2::FileExt;
use handlebars::Handlebars;
use inquire::{MultiSelect, Select};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use tempfile::TempDir;
use walkdir::WalkDir;

const SOURCE_MANIFEST: &str = "rainy-source.yaml";
const CATALOG_FILE: &str = "sources.yaml";
const LOCK_FILE: &str = "sources.lock";
const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_SOURCE_ENTRIES: usize = 10_000;
const MAX_SOURCE_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RainySourceManifest {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    pub metadata: SourceMetadata,
    pub requires: SourceRequirements,
    pub contents: Vec<SourceContent>,
    #[serde(default)]
    pub extensions: BTreeMap<String, serde_yaml::Value>,
    #[serde(flatten)]
    pub extension_fields: BTreeMap<String, serde_yaml::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceMetadata {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRequirements {
    pub rainy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SourceContentType {
    ProjectTemplateCatalog,
    ProjectTemplate,
    WorkspaceModule,
    CapabilityPack,
    Skill,
    Plugin,
    Defaults,
}

impl SourceContentType {
    fn as_str(&self) -> &'static str {
        match self {
            Self::ProjectTemplateCatalog => "project-template-catalog",
            Self::ProjectTemplate => "project-template",
            Self::WorkspaceModule => "workspace-module",
            Self::CapabilityPack => "capability-pack",
            Self::Skill => "skill",
            Self::Plugin => "plugin",
            Self::Defaults => "defaults",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceContent {
    pub id: String,
    #[serde(rename = "type")]
    pub content_type: SourceContentType,
    pub path: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "defaultTarget", default)]
    pub default_target: Option<String>,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceCatalog {
    #[serde(rename = "apiVersion")]
    api_version: String,
    kind: String,
    #[serde(default)]
    sources: BTreeMap<String, SourceConfig>,
}

impl Default for SourceCatalog {
    fn default() -> Self {
        Self {
            api_version: "rainy.dev/v1".to_string(),
            kind: "RainySourceCatalog".to_string(),
            sources: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceConfig {
    source: String,
    #[serde(rename = "ref", default, skip_serializing_if = "Option::is_none")]
    reference: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sha256: Option<String>,
    #[serde(default = "default_channel")]
    channel: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    version: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SourceLock {
    #[serde(default = "source_lock_version")]
    lockfile_version: u32,
    #[serde(default)]
    sources: BTreeMap<String, LockedSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LockedSource {
    source_type: String,
    source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    requested_ref: Option<String>,
    resolved_ref: String,
    version: String,
    digest: String,
    cache_path: String,
    contents: Vec<LockedContent>,
    synced_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LockedContent {
    id: String,
    #[serde(rename = "type")]
    content_type: String,
    path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RainySourceIndex {
    #[serde(rename = "apiVersion")]
    api_version: String,
    kind: String,
    metadata: SourceIndexMetadata,
    releases: Vec<SourceRelease>,
    #[serde(default)]
    extensions: BTreeMap<String, serde_yaml::Value>,
    #[serde(flatten)]
    extension_fields: BTreeMap<String, serde_yaml::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceIndexMetadata {
    name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SourceRelease {
    version: String,
    url: String,
    sha256: String,
    #[serde(default = "default_channel")]
    channel: String,
    #[serde(default)]
    notes_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceReport {
    pub protocol_version: String,
    pub operation: String,
    pub status: String,
    pub sources: Vec<SourceInfo>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    pub name: String,
    pub source_type: String,
    pub source: String,
    pub requested_ref: Option<String>,
    pub resolved_ref: Option<String>,
    pub current_version: Option<String>,
    pub latest_version: Option<String>,
    pub digest: Option<String>,
    pub cache_path: Option<String>,
    pub update_available: Option<bool>,
    pub state: String,
    pub message: Option<String>,
    pub contents: Vec<SourceContentInfo>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceContentInfo {
    pub id: String,
    #[serde(rename = "type")]
    pub content_type: String,
    pub path: String,
    pub version: Option<String>,
    pub default_target: Option<String>,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_path: Option<String>,
}

pub struct SourceProjectOptions<'a> {
    pub base_dir: PathBuf,
    pub name: String,
    pub package: String,
    pub source: String,
    pub template: Option<String>,
    pub modules: Vec<String>,
    pub git_url: Option<String>,
    pub dry_run: bool,
    pub interactive: bool,
    pub no_color: bool,
    pub progress: &'a ProgressReporter,
}

pub struct ProjectSourceChoice {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
}

pub struct CachedProjectTemplateCatalogChoice {
    pub source_name: String,
    pub source_version: String,
    pub path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectSourceLock {
    lockfile_version: u32,
    source: ProjectSourceIdentity,
    template: String,
    modules: Vec<String>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectSourceIdentity {
    name: String,
    version: String,
    resolved_ref: String,
    digest: String,
    origin: ProjectSourceOrigin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectSourceOrigin {
    source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reference: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sha256: Option<String>,
    channel: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    selected_version: Option<String>,
}

struct PreparedSource {
    _temp: Option<TempDir>,
    root: PathBuf,
    source_type: String,
    resolved_ref: String,
    release_version: Option<String>,
    release_name: Option<String>,
}

struct RemoteObservation {
    resolved_ref: Option<String>,
    latest_version: Option<String>,
    update_available: bool,
    message: String,
}

fn default_channel() -> String {
    "stable".to_string()
}

fn source_lock_version() -> u32 {
    1
}

pub fn handle_source_command(
    workspace: &Path,
    command: SourceCommand,
    progress: &ProgressReporter,
) -> RainyResult<CommandOutput> {
    let report = match command.command {
        SourceSubcommand::Inspect(args) => inspect_source(workspace, args, progress)?,
        SourceSubcommand::Add(args) => add_source(workspace, args, progress)?,
        SourceSubcommand::List => list_sources()?,
        SourceSubcommand::Resolve(args) => resolve_content(args)?,
        SourceSubcommand::Check(args) => check_sources(workspace, args, "check")?,
        SourceSubcommand::Sync(args) => sync_sources(workspace, args, false, progress)?,
        SourceSubcommand::Update(args) => sync_sources(workspace, args, true, progress)?,
        SourceSubcommand::Remove(args) => remove_source(args)?,
    };
    Ok(CommandOutput::Source { report })
}

fn resolve_content(args: SourceResolveArgs) -> RainyResult<SourceReport> {
    validate_source_name(&args.name)?;
    validate_source_name(&args.content)?;
    let catalog = load_catalog()?;
    let config = catalog.sources.get(&args.name).ok_or_else(|| {
        RainyError::registry(
            "SOURCE_NOT_FOUND",
            format!("Source is not configured: {}", args.name),
        )
    })?;
    let lock = load_lock()?;
    let locked = lock.sources.get(&args.name).ok_or_else(|| {
        RainyError::registry(
            "SOURCE_NOT_SYNCHRONIZED",
            format!(
                "Source '{}' has no verified cache; run rainy source sync {} --apply",
                args.name, args.name
            ),
        )
    })?;
    let cache = PathBuf::from(&locked.cache_path);
    if !cache.is_dir() || digest_tree(&cache)? != locked.digest {
        return Err(RainyError::registry(
            "SOURCE_CACHE_DIGEST_MISMATCH",
            format!(
                "verified Source cache is missing or changed; run rainy source sync {} --apply",
                args.name
            ),
        ));
    }
    let content = locked
        .contents
        .iter()
        .find(|content| content.id == args.content)
        .ok_or_else(|| {
            RainyError::registry(
                "SOURCE_CONTENT_NOT_FOUND",
                format!(
                    "Source '{}' does not declare content '{}'",
                    args.name, args.content
                ),
            )
        })?;
    let mut info = info_from_lock(&args.name, config, Some(locked));
    info.state = "current".to_string();
    info.message = Some("Content path belongs to the verified immutable Source cache".to_string());
    info.contents.retain(|item| item.id == content.id);
    Ok(report("resolve", "passed", vec![info]))
}

fn inspect_source(
    workspace: &Path,
    args: SourceInspectArgs,
    progress: &ProgressReporter,
) -> RainyResult<SourceReport> {
    let config = config_from_args(args);
    progress.detail(format!("Downloading and validating {}", config.source));
    let prepared = prepare_source(workspace, &config)?;
    let (manifest, _) = validate_source_root(&prepared.root)?;
    validate_release_identity(&prepared, &manifest)?;
    Ok(report(
        "inspect",
        "passed",
        vec![info_from_prepared(
            &manifest.metadata.name,
            &config,
            &prepared,
            &manifest,
            "available",
        )?],
    ))
}

fn add_source(
    workspace: &Path,
    args: SourceAddArgs,
    progress: &ProgressReporter,
) -> RainyResult<SourceReport> {
    let apply = resolve_apply(args.dry_run, args.apply)?;
    validate_source_name(&args.name)?;
    let mut config = config_from_args(args.source);
    normalize_local_config(workspace, &mut config)?;
    if !apply {
        return Ok(report(
            "add",
            "dry-run",
            vec![preview_info(&args.name, &config)],
        ));
    }

    progress.detail(format!("Validating Source {}", args.name));
    let _state_lock = lock_source_state()?;
    let (locked, info) = synchronize_one(workspace, &args.name, &config)?;
    let mut catalog = load_catalog()?;
    let mut lock = load_lock()?;
    catalog.sources.insert(args.name.clone(), config);
    lock.sources.insert(args.name, locked);
    save_catalog(&catalog)?;
    save_lock(&lock)?;
    Ok(report("add", "applied", vec![info]))
}

fn list_sources() -> RainyResult<SourceReport> {
    let catalog = load_catalog()?;
    let lock = load_lock()?;
    let sources = catalog
        .sources
        .iter()
        .map(|(name, config)| info_from_lock(name, config, lock.sources.get(name)))
        .collect();
    Ok(report("list", "passed", sources))
}

pub fn available_project_sources() -> RainyResult<Vec<ProjectSourceChoice>> {
    let catalog = load_catalog()?;
    let lock = load_lock()?;
    let mut choices = Vec::new();
    for (name, config) in &catalog.sources {
        let Some(locked) = lock
            .sources
            .get(name)
            .filter(|locked| lock_matches_config(locked, config))
        else {
            continue;
        };
        let cache = PathBuf::from(&locked.cache_path);
        if !cache.is_dir() {
            continue;
        }
        let Ok(manifest_text) = std::fs::read_to_string(cache.join(SOURCE_MANIFEST)) else {
            continue;
        };
        let Ok(manifest) = serde_yaml::from_str::<RainySourceManifest>(&manifest_text) else {
            continue;
        };
        if manifest.api_version != "rainy.dev/v1" || manifest.kind != "RainySource" {
            continue;
        }
        if !manifest.contents.iter().any(|content| {
            content.content_type == SourceContentType::ProjectTemplate
                && safe_relative_path(&content.path, "SOURCE_CONTENT_PATH_INVALID")
                    .is_ok_and(|path| cache.join(path).is_dir())
        }) {
            continue;
        }
        choices.push(ProjectSourceChoice {
            name: name.clone(),
            version: locked.version.clone(),
            description: manifest.metadata.description,
        });
    }
    Ok(choices)
}

pub fn available_project_template_catalogs() -> RainyResult<Vec<CachedProjectTemplateCatalogChoice>>
{
    let catalog = load_catalog()?;
    let lock = load_lock()?;
    let mut choices = Vec::new();
    for (name, config) in &catalog.sources {
        let Some(locked) = lock
            .sources
            .get(name)
            .filter(|locked| lock_matches_config(locked, config))
        else {
            continue;
        };
        let cache = PathBuf::from(&locked.cache_path);
        if !cache.is_dir() {
            continue;
        }
        let catalog_contents = locked
            .contents
            .iter()
            .filter(|content| {
                content.content_type == SourceContentType::ProjectTemplateCatalog.as_str()
            })
            .collect::<Vec<_>>();
        if catalog_contents.is_empty() {
            continue;
        }
        if digest_tree(&cache)? != locked.digest {
            return Err(RainyError::registry(
                "SOURCE_CACHE_DIGEST_MISMATCH",
                format!(
                    "verified Source cache for '{name}' changed; run rainy source sync {name} --apply"
                ),
            ));
        }
        for content in catalog_contents {
            let relative = safe_relative_path(&content.path, "SOURCE_CONTENT_PATH_INVALID")?;
            let path = cache.join(relative).join("project-templates.yaml");
            crate::project_template::inspect_template_catalog(path.clone())?;
            choices.push(CachedProjectTemplateCatalogChoice {
                source_name: name.clone(),
                source_version: locked.version.clone(),
                path,
            });
        }
    }
    choices.sort_by(|left, right| {
        left.source_name
            .cmp(&right.source_name)
            .then_with(|| left.path.cmp(&right.path))
    });
    Ok(choices)
}

fn check_sources(
    workspace: &Path,
    args: SourceSelectArgs,
    operation: &str,
) -> RainyResult<SourceReport> {
    let catalog = load_catalog()?;
    let lock = load_lock()?;
    let project_lock = selected_project_lock(workspace, &args)?;
    let selected = select_configs(&catalog, &args, project_lock.as_ref())?;
    let mut infos = Vec::new();
    let mut warning = false;
    for (name, config) in selected {
        let locked = lock
            .sources
            .get(&name)
            .filter(|locked| lock_matches_config(locked, &config));
        match observe_source(&config, locked) {
            Ok(mut observation) => {
                if let Some(project_lock) = &project_lock
                    && locked.is_none()
                    && observation.resolved_ref.as_deref()
                        == Some(project_lock.source.resolved_ref.as_str())
                {
                    observation.update_available = false;
                    observation.latest_version = Some(project_lock.source.version.clone());
                    observation.message =
                        "Remote Source revision matches the project provenance lock".to_string();
                }
                let mut info = info_from_lock(&name, &config, locked);
                info.resolved_ref = observation.resolved_ref.clone().or(info.resolved_ref);
                info.latest_version = observation
                    .latest_version
                    .clone()
                    .or(info.current_version.clone());
                info.update_available = Some(observation.update_available);
                info.state = if observation.update_available {
                    "update-available"
                } else {
                    "current"
                }
                .to_string();
                info.message = Some(observation.message.clone());
                if let Some(project_lock) = &project_lock {
                    apply_project_status(&mut info, project_lock, locked, &observation);
                }
                infos.push(info);
            }
            Err(error) => {
                warning = true;
                let mut info = info_from_lock(&name, &config, locked);
                info.state = "unreachable".to_string();
                info.message = Some(format!(
                    "{}; {}",
                    error.body().message,
                    if locked.is_some() {
                        "the verified cache remains available"
                    } else {
                        "no verified cache is available"
                    }
                ));
                if let Some(project_lock) = &project_lock {
                    info.current_version = Some(project_lock.source.version.clone());
                    info.resolved_ref = Some(project_lock.source.resolved_ref.clone());
                    info.digest = Some(project_lock.source.digest.clone());
                }
                infos.push(info);
            }
        }
    }
    Ok(report(
        operation,
        if warning { "warning" } else { "passed" },
        infos,
    ))
}

fn sync_sources(
    workspace: &Path,
    args: SourceChangeArgs,
    updates_only: bool,
    progress: &ProgressReporter,
) -> RainyResult<SourceReport> {
    let apply = resolve_apply(args.dry_run, args.apply)?;
    if !apply {
        return check_sources(
            workspace,
            args.selection,
            if updates_only { "update" } else { "sync" },
        );
    }
    let _state_lock = lock_source_state()?;
    let mut catalog = load_catalog()?;
    let mut lock = load_lock()?;
    let project_lock = selected_project_lock(workspace, &args.selection)?;
    let selected = select_configs(&catalog, &args.selection, project_lock.as_ref())?;
    let mut infos = Vec::new();
    let mut warning = false;
    let mut failed = false;
    let mut catalog_changed = false;
    for (name, config) in selected {
        if updates_only {
            let matching_lock = lock
                .sources
                .get(&name)
                .filter(|locked| lock_matches_config(locked, &config));
            match observe_source(&config, matching_lock) {
                Ok(observation) if !observation.update_available => {
                    let mut info = info_from_lock(&name, &config, matching_lock);
                    info.state = "current".to_string();
                    info.update_available = Some(false);
                    info.latest_version = observation.latest_version;
                    info.message = Some(observation.message);
                    infos.push(info);
                    continue;
                }
                Ok(_) => {}
                Err(error) => {
                    warning = true;
                    let mut info = info_from_lock(&name, &config, matching_lock);
                    info.state = "unreachable".to_string();
                    info.message = Some(format!(
                        "{}; preserving the previous verified cache",
                        error.body().message
                    ));
                    infos.push(info);
                    continue;
                }
            }
        }
        progress.detail(format!("Synchronizing Source {name}"));
        match synchronize_one(workspace, &name, &config) {
            Ok((locked, mut info)) => {
                info.state = "updated".to_string();
                info.update_available = Some(false);
                lock.sources.insert(name.clone(), locked);
                if project_lock.is_some() && catalog.sources.get(&name) != Some(&config) {
                    catalog.sources.insert(name.clone(), config.clone());
                    catalog_changed = true;
                }
                if project_lock.is_some() {
                    info.message = Some(
                        "Managed Source cache was refreshed; generated project files remain pinned and were not overwritten"
                            .to_string(),
                    );
                }
                infos.push(info);
            }
            Err(error) => {
                let matching_lock = lock
                    .sources
                    .get(&name)
                    .filter(|locked| lock_matches_config(locked, &config));
                let cached = matching_lock.is_some();
                warning |= cached;
                failed |= !cached;
                let mut info = info_from_lock(&name, &config, matching_lock);
                info.state = if cached { "unreachable" } else { "invalid" }.to_string();
                info.message = Some(format!(
                    "{}; {}",
                    error.body().message,
                    if cached {
                        "preserving the previous verified cache"
                    } else {
                        "the Source was not added to managed state"
                    }
                ));
                infos.push(info);
            }
        }
    }
    if catalog_changed {
        save_catalog(&catalog)?;
    }
    save_lock(&lock)?;
    Ok(report(
        if updates_only { "update" } else { "sync" },
        if failed {
            "failed"
        } else if warning {
            "warning"
        } else {
            "applied"
        },
        infos,
    ))
}

fn remove_source(args: SourceRemoveArgs) -> RainyResult<SourceReport> {
    let apply = resolve_apply(args.dry_run, args.apply)?;
    let _state_lock = if apply {
        Some(lock_source_state()?)
    } else {
        None
    };
    let mut catalog = load_catalog()?;
    let Some(config) = catalog.sources.get(&args.name).cloned() else {
        return Err(RainyError::registry(
            "SOURCE_NOT_FOUND",
            format!("Source is not configured: {}", args.name),
        ));
    };
    let mut info = info_from_lock(&args.name, &config, load_lock()?.sources.get(&args.name));
    info.state = if apply { "removed" } else { "preview" }.to_string();
    if apply {
        let mut lock = load_lock()?;
        catalog.sources.remove(&args.name);
        lock.sources.remove(&args.name);
        save_catalog(&catalog)?;
        save_lock(&lock)?;
    }
    Ok(report(
        "remove",
        if apply { "applied" } else { "dry-run" },
        vec![info],
    ))
}

fn config_from_args(args: SourceInspectArgs) -> SourceConfig {
    SourceConfig {
        source: args.source,
        reference: args.reference,
        sha256: args.sha256,
        channel: args.channel,
        version: args.version,
    }
}

fn resolve_apply(dry_run: bool, apply: bool) -> RainyResult<bool> {
    if dry_run && apply {
        return Err(RainyError::config(
            "APPLY_MODE_CONFLICT",
            "--dry-run and --apply cannot be used together",
        ));
    }
    Ok(apply)
}

fn select_configs(
    catalog: &SourceCatalog,
    args: &SourceSelectArgs,
    project_lock: Option<&ProjectSourceLock>,
) -> RainyResult<Vec<(String, SourceConfig)>> {
    let project_source = project_lock.map(|lock| lock.source.name.as_str());
    if let Some(project_lock) = project_lock {
        return Ok(vec![(
            project_lock.source.name.clone(),
            project_lock.source.origin.to_config(),
        )]);
    }
    let selected = catalog
        .sources
        .iter()
        .filter(|(name, _)| {
            args.all || args.name.as_ref() == Some(*name) || project_source == Some(name.as_str())
        })
        .map(|(name, config)| (name.clone(), config.clone()))
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return Err(RainyError::registry(
            "SOURCE_NOT_FOUND",
            format!(
                "Source is not configured: {}",
                args.name
                    .as_deref()
                    .or(project_source)
                    .unwrap_or("no Sources")
            ),
        ));
    }
    Ok(selected)
}

fn lock_matches_config(locked: &LockedSource, config: &SourceConfig) -> bool {
    locked.source == config.source
        && locked.requested_ref == requested_ref(config, source_type(config))
}

impl ProjectSourceOrigin {
    fn from_config(config: &SourceConfig) -> Self {
        Self {
            source: config.source.clone(),
            reference: config.reference.clone(),
            sha256: config.sha256.clone(),
            channel: config.channel.clone(),
            selected_version: config.version.clone(),
        }
    }

    fn to_config(&self) -> SourceConfig {
        SourceConfig {
            source: self.source.clone(),
            reference: self.reference.clone(),
            sha256: self.sha256.clone(),
            channel: self.channel.clone(),
            version: self.selected_version.clone(),
        }
    }
}

fn selected_project_lock(
    workspace: &Path,
    args: &SourceSelectArgs,
) -> RainyResult<Option<ProjectSourceLock>> {
    if !args.project {
        return Ok(None);
    }
    let path = workspace.join(".rainy/project-source.lock");
    if !path.is_file() {
        return Err(RainyError::config(
            "SOURCE_PROJECT_LOCK_NOT_FOUND",
            format!(
                "project Source lock was not found: {}; create this project with rainy new --source or choose a configured Source name",
                path.display()
            ),
        ));
    }
    let lock: ProjectSourceLock = serde_yaml::from_str(&std::fs::read_to_string(&path)?)?;
    if lock.lockfile_version != 1 {
        return Err(RainyError::config(
            "SOURCE_PROJECT_LOCK_VERSION_UNSUPPORTED",
            format!(
                "unsupported project Source lock version: {}",
                lock.lockfile_version
            ),
        ));
    }
    validate_source_name(&lock.source.name)?;
    Version::parse(&lock.source.version).map_err(|error| {
        RainyError::config(
            "SOURCE_PROJECT_LOCK_INVALID",
            format!("project Source version is invalid: {error}"),
        )
    })?;
    validate_sha256(&lock.source.digest)?;
    if lock.source.origin.channel.trim().is_empty() {
        return Err(RainyError::config(
            "SOURCE_PROJECT_LOCK_INVALID",
            "project Source origin channel cannot be empty",
        ));
    }
    Ok(Some(lock))
}

fn apply_project_status(
    info: &mut SourceInfo,
    project: &ProjectSourceLock,
    managed: Option<&LockedSource>,
    observation: &RemoteObservation,
) {
    info.current_version = Some(project.source.version.clone());
    info.resolved_ref = Some(project.source.resolved_ref.clone());
    info.digest = Some(project.source.digest.clone());
    let managed_changed = managed.is_some_and(|locked| {
        locked.version != project.source.version || locked.digest != project.source.digest
    });
    if observation.update_available {
        info.state = "update-available".to_string();
        info.update_available = Some(true);
        info.message = Some(
            "The upstream Source changed; run rainy source update --project --apply, then check again. Project files are never overwritten automatically"
                .to_string(),
        );
    } else if managed_changed {
        info.state = "project-update-available".to_string();
        info.update_available = Some(true);
        info.latest_version = managed.map(|locked| locked.version.clone());
        info.message = Some(
            "The managed Source cache is newer than this project. Review the new template or modules and migrate explicitly; Rainy will not overwrite generated files"
                .to_string(),
        );
    } else {
        info.state = "current".to_string();
        info.update_available = Some(false);
        info.message =
            Some("Project Source version matches the verified managed cache".to_string());
    }
}

fn synchronize_one(
    workspace: &Path,
    name: &str,
    config: &SourceConfig,
) -> RainyResult<(LockedSource, SourceInfo)> {
    let prepared = prepare_source(workspace, config)?;
    let (manifest, contents) = validate_source_root(&prepared.root)?;
    validate_release_identity(&prepared, &manifest)?;
    let digest = digest_tree(&prepared.root)?;
    let cache_path = cache_source(name, &prepared.root, &digest)?;
    let locked = LockedSource {
        source_type: prepared.source_type.clone(),
        source: config.source.clone(),
        requested_ref: requested_ref(config, &prepared.source_type),
        resolved_ref: prepared.resolved_ref.clone(),
        version: manifest.metadata.version.clone(),
        digest: digest.clone(),
        cache_path: cache_path.to_string_lossy().to_string(),
        contents,
        synced_at: Utc::now(),
    };
    let info = info_from_locked(name, config, &locked, &manifest, "cached");
    Ok((locked, info))
}

fn prepare_source(workspace: &Path, config: &SourceConfig) -> RainyResult<PreparedSource> {
    if let Some(url) = config.source.strip_prefix("git+") {
        return prepare_git(url, config.reference.as_deref().unwrap_or("main"));
    }
    if config.source.starts_with("https://") || config.source.starts_with("http://") {
        if is_archive_url(&config.source) {
            return prepare_archive(&config.source, config.sha256.as_deref(), "archive", None);
        }
        return prepare_index(config);
    }
    if config.reference.is_some() || config.sha256.is_some() || config.version.is_some() {
        return Err(RainyError::config(
            "SOURCE_OPTIONS_INVALID",
            "--ref, --sha256, and --version are valid only for their corresponding remote Source types",
        ));
    }
    let path = PathBuf::from(&config.source);
    let path = if path.is_absolute() {
        path
    } else {
        workspace.join(path)
    };
    let root = path.canonicalize().map_err(|error| {
        RainyError::registry(
            "SOURCE_NOT_FOUND",
            format!("Source path is not available: {} ({error})", path.display()),
        )
    })?;
    let digest = digest_tree(&root)?;
    Ok(PreparedSource {
        _temp: None,
        root,
        source_type: "local".to_string(),
        resolved_ref: format!("sha256:{digest}"),
        release_version: None,
        release_name: None,
    })
}

fn prepare_git(url: &str, reference: &str) -> RainyResult<PreparedSource> {
    crate::security::validate_git(url, false).map_err(|reason| {
        RainyError::registry(
            "SOURCE_GIT_URL_INVALID",
            format!("Git Source URL is not allowed: {reason}"),
        )
    })?;
    validate_git_ref(reference)?;
    let temp = tempfile::Builder::new()
        .prefix("rainy-source-git-")
        .tempdir()?;
    let checkout = temp.path().join("checkout");
    let mut command = Command::new("git");
    command.args(["clone", "--depth", "1", "--no-tags", "--branch", reference]);
    command.arg(url).arg(&checkout);
    let output = crate::process::run_command(
        command,
        "git",
        Duration::from_secs(900),
        crate::process::DEFAULT_OUTPUT_LIMIT,
    )?;
    if !output.success() {
        std::fs::create_dir_all(&checkout)?;
        run_git(&checkout, &["init", "--quiet"])?;
        run_git(&checkout, &["remote", "add", "origin", url])?;
        run_git(
            &checkout,
            &["fetch", "--depth", "1", "--no-tags", "origin", reference],
        )?;
        run_git(
            &checkout,
            &["checkout", "--quiet", "--detach", "FETCH_HEAD"],
        )?;
    }
    let resolved = git_output(&checkout, &["rev-parse", "HEAD"])?;
    Ok(PreparedSource {
        _temp: Some(temp),
        root: checkout,
        source_type: "git".to_string(),
        resolved_ref: resolved,
        release_version: None,
        release_name: None,
    })
}

fn prepare_index(config: &SourceConfig) -> RainyResult<PreparedSource> {
    let index = load_index(&config.source)?;
    let release = select_release(&index, &config.channel, config.version.as_deref())?;
    let mut prepared = prepare_archive(
        &resolve_index_url(&config.source, &release.url)?,
        Some(&release.sha256),
        "index",
        Some(release.version.clone()),
    )?;
    prepared.release_version = Some(release.version.clone());
    prepared.release_name = Some(index.metadata.name.clone());
    Ok(prepared)
}

fn prepare_archive(
    url: &str,
    configured_sha256: Option<&str>,
    source_type: &str,
    release_version: Option<String>,
) -> RainyResult<PreparedSource> {
    crate::security::validate_http(url, true).map_err(|reason| {
        RainyError::registry(
            "SOURCE_ARCHIVE_URL_INVALID",
            format!("archive Source URL is not allowed: {reason}"),
        )
    })?;
    let bytes = http_get_bytes(url, MAX_ARCHIVE_BYTES)?;
    let actual = hex(&Sha256::digest(&bytes));
    let expected = match configured_sha256 {
        Some(value) => value.to_string(),
        None => http_get_text(&format!("{url}.sha256"))?
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_string(),
    };
    validate_sha256(&expected)?;
    if !actual.eq_ignore_ascii_case(&expected) {
        return Err(RainyError::registry(
            "SOURCE_ARCHIVE_CHECKSUM_INVALID",
            format!("archive checksum mismatch: expected {expected}, got {actual}"),
        ));
    }
    let temp = tempfile::Builder::new()
        .prefix("rainy-source-archive-")
        .tempdir()?;
    let extracted = temp.path().join("extracted");
    std::fs::create_dir_all(&extracted)?;
    if url_path(url).ends_with(".zip") {
        extract_zip(&bytes, &extracted)?;
    } else {
        extract_tar_gz(&bytes, &extracted)?;
    }
    let root = normalize_source_root(&extracted)?;
    Ok(PreparedSource {
        _temp: Some(temp),
        root,
        source_type: source_type.to_string(),
        resolved_ref: format!("sha256:{actual}"),
        release_version,
        release_name: None,
    })
}

fn validate_release_identity(
    prepared: &PreparedSource,
    manifest: &RainySourceManifest,
) -> RainyResult<()> {
    if prepared
        .release_version
        .as_ref()
        .is_some_and(|version| version != &manifest.metadata.version)
    {
        return Err(RainyError::config(
            "SOURCE_RELEASE_IDENTITY_MISMATCH",
            format!(
                "Source Index release version {} does not match rainy-source.yaml version {}",
                prepared.release_version.as_deref().unwrap_or_default(),
                manifest.metadata.version
            ),
        ));
    }
    if prepared
        .release_name
        .as_ref()
        .is_some_and(|name| name != &manifest.metadata.name)
    {
        return Err(RainyError::config(
            "SOURCE_RELEASE_IDENTITY_MISMATCH",
            format!(
                "Source Index name {} does not match rainy-source.yaml name {}",
                prepared.release_name.as_deref().unwrap_or_default(),
                manifest.metadata.name
            ),
        ));
    }
    Ok(())
}

fn validate_source_root(root: &Path) -> RainyResult<(RainySourceManifest, Vec<LockedContent>)> {
    let manifest_path = root.join(SOURCE_MANIFEST);
    if !manifest_path.is_file() {
        return Err(RainyError::config(
            "SOURCE_MANIFEST_NOT_FOUND",
            format!(
                "Source root must contain {SOURCE_MANIFEST}: {}",
                root.display()
            ),
        ));
    }
    let manifest: RainySourceManifest =
        serde_yaml::from_str(&std::fs::read_to_string(&manifest_path)?)?;
    if manifest.api_version != "rainy.dev/v1" || manifest.kind != "RainySource" {
        return Err(RainyError::config(
            "SOURCE_MANIFEST_INVALID",
            "rainy-source.yaml must use apiVersion rainy.dev/v1 and kind RainySource",
        ));
    }
    validate_source_name(&manifest.metadata.name)?;
    Version::parse(&manifest.metadata.version).map_err(|error| {
        RainyError::config(
            "SOURCE_VERSION_INVALID",
            format!("Source metadata.version must be SemVer: {error}"),
        )
    })?;
    let requirement = VersionReq::parse(&manifest.requires.rainy).map_err(|error| {
        RainyError::config(
            "SOURCE_COMPATIBILITY_INVALID",
            format!("requires.rainy is invalid: {error}"),
        )
    })?;
    let current = Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|error| RainyError::config("SOURCE_COMPATIBILITY_INVALID", error.to_string()))?;
    if !requirement.matches(&current) {
        return Err(RainyError::config(
            "SOURCE_INCOMPATIBLE",
            format!(
                "Source {} requires Rainy {}, current version is {}",
                manifest.metadata.version, manifest.requires.rainy, current
            ),
        ));
    }
    validate_extension_fields(&manifest.extension_fields)?;
    if manifest.contents.is_empty() {
        return Err(RainyError::config(
            "SOURCE_CONTENT_EMPTY",
            "Source must declare at least one content item",
        ));
    }
    let mut ids = BTreeSet::new();
    let mut paths = BTreeSet::new();
    let mut locked = Vec::new();
    for content in &manifest.contents {
        validate_source_name(&content.id)?;
        if !ids.insert(content.id.clone()) {
            return Err(RainyError::config(
                "SOURCE_CONTENT_DUPLICATE",
                format!("duplicate Source content id: {}", content.id),
            ));
        }
        let relative = safe_relative_path(&content.path, "SOURCE_CONTENT_PATH_INVALID")?;
        if !paths.insert(relative.clone()) {
            return Err(RainyError::config(
                "SOURCE_CONTENT_DUPLICATE",
                format!("multiple Source contents use path: {}", content.path),
            ));
        }
        if let Some(target) = &content.default_target {
            safe_relative_path(target, "SOURCE_CONTENT_TARGET_INVALID")?;
        }
        if let Some(version) = &content.version {
            Version::parse(version).map_err(|error| {
                RainyError::config(
                    "SOURCE_CONTENT_VERSION_INVALID",
                    format!("content {} version is invalid: {error}", content.id),
                )
            })?;
        }
        let content_root = root.join(&relative);
        validate_content(content, &content_root)?;
        locked.push(LockedContent {
            id: content.id.clone(),
            content_type: content.content_type.as_str().to_string(),
            path: content.path.clone(),
            version: content.version.clone(),
            digest: digest_tree(&content_root)?,
        });
    }
    Ok((manifest, locked))
}

fn validate_content(content: &SourceContent, root: &Path) -> RainyResult<()> {
    if !root.is_dir() {
        return Err(RainyError::config(
            "SOURCE_CONTENT_NOT_FOUND",
            format!(
                "content {} directory was not found: {}",
                content.id,
                root.display()
            ),
        ));
    }
    match content.content_type {
        SourceContentType::ProjectTemplateCatalog => {
            crate::project_template::inspect_template_catalog(root.join("project-templates.yaml"))?;
        }
        SourceContentType::ProjectTemplate | SourceContentType::WorkspaceModule => {}
        SourceContentType::CapabilityPack => {
            let path = root.join("pack.yaml");
            if !path.is_file() {
                return missing_marker(content, "pack.yaml");
            }
            let pack: crate::registry::CapabilityPack =
                serde_yaml::from_str(&std::fs::read_to_string(path)?)?;
            if pack.api_version != "rainy.dev/v1" || pack.kind != "CapabilityPack" {
                return Err(RainyError::config(
                    "SOURCE_CONTENT_INVALID",
                    format!("content {} contains an invalid Capability Pack", content.id),
                ));
            }
            if pack.metadata.name != content.id {
                return Err(RainyError::config(
                    "SOURCE_CONTENT_IDENTITY_MISMATCH",
                    format!(
                        "content id {} does not match Pack name {}",
                        content.id, pack.metadata.name
                    ),
                ));
            }
            if content
                .version
                .as_ref()
                .is_some_and(|version| version != &pack.metadata.version)
            {
                return Err(RainyError::config(
                    "SOURCE_CONTENT_IDENTITY_MISMATCH",
                    format!("content {} version does not match pack.yaml", content.id),
                ));
            }
            let conformance = crate::conformance::check_path(root)?;
            if conformance.status == "failed" {
                let failures = conformance
                    .checks
                    .iter()
                    .filter(|check| check.status == "failed")
                    .map(|check| format!("{}: {}", check.id, check.message))
                    .collect::<Vec<_>>()
                    .join("; ");
                return Err(RainyError::config(
                    "SOURCE_CONTENT_INVALID",
                    format!("content {} failed Pack conformance: {failures}", content.id),
                ));
            }
        }
        SourceContentType::Skill => {
            if !root.join("SKILL.md").is_file() {
                return missing_marker(content, "SKILL.md");
            }
            crate::skills::validate_source_skill(root, &content.id)?;
        }
        SourceContentType::Plugin => {
            let path = [
                root.join("plugin.json"),
                root.join(".rainy-plugin/plugin.json"),
            ]
            .into_iter()
            .find(|path| path.is_file())
            .ok_or_else(|| {
                RainyError::config(
                    "SOURCE_CONTENT_INVALID",
                    format!("content {} must contain plugin.json", content.id),
                )
            })?;
            let plugin: crate::plugin::PluginManifest =
                serde_json::from_str(&std::fs::read_to_string(path)?)?;
            if plugin.protocol_version != "rainy.plugin.v1" || plugin.name != content.id {
                return Err(RainyError::config(
                    "SOURCE_CONTENT_IDENTITY_MISMATCH",
                    format!("content {} does not match plugin.json", content.id),
                ));
            }
            crate::plugin::validate_plugin_permissions(&plugin)?;
            crate::plugin::validate_plugin_actions(&plugin)?;
        }
        SourceContentType::Defaults => {
            let path = root.join("rainy-defaults.yaml");
            if !path.is_file() {
                return missing_marker(content, "rainy-defaults.yaml");
            }
            let value: serde_yaml::Value = serde_yaml::from_str(&std::fs::read_to_string(path)?)?;
            if value.get("kind").and_then(serde_yaml::Value::as_str) != Some("RainyDefaults") {
                return Err(RainyError::config(
                    "SOURCE_CONTENT_INVALID",
                    format!(
                        "content {} contains an invalid Rainy Defaults package",
                        content.id
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn missing_marker(content: &SourceContent, marker: &str) -> RainyResult<()> {
    Err(RainyError::config(
        "SOURCE_CONTENT_INVALID",
        format!("content {} must contain {marker}", content.id),
    ))
}

fn observe_source(
    config: &SourceConfig,
    locked: Option<&LockedSource>,
) -> RainyResult<RemoteObservation> {
    if let Some(url) = config.source.strip_prefix("git+") {
        let reference = config.reference.as_deref().unwrap_or("main");
        let resolved = git_remote_ref(url, reference)?;
        let changed = locked.is_none_or(|locked| locked.resolved_ref != resolved);
        return Ok(RemoteObservation {
            resolved_ref: Some(resolved),
            latest_version: locked
                .filter(|_| !changed)
                .map(|locked| locked.version.clone()),
            update_available: changed,
            message: if changed {
                "remote Git revision changed; update will download and validate its declared version"
                    .to_string()
            } else {
                "remote Git revision matches the verified lock".to_string()
            },
        });
    }
    if config.source.starts_with("https://") || config.source.starts_with("http://") {
        if is_archive_url(&config.source) {
            if let Some(expected) = &config.sha256 {
                validate_sha256(expected)?;
                let changed =
                    locked.is_none_or(|locked| locked.resolved_ref != format!("sha256:{expected}"));
                return Ok(RemoteObservation {
                    resolved_ref: Some(format!("sha256:{expected}")),
                    latest_version: locked.map(|locked| locked.version.clone()),
                    update_available: changed,
                    message: "direct archive is pinned by its configured SHA-256".to_string(),
                });
            }
            let digest = http_get_text(&format!("{}.sha256", config.source))?
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_string();
            validate_sha256(&digest)?;
            let changed =
                locked.is_none_or(|locked| locked.resolved_ref != format!("sha256:{digest}"));
            return Ok(RemoteObservation {
                resolved_ref: Some(format!("sha256:{digest}")),
                latest_version: locked
                    .filter(|_| !changed)
                    .map(|locked| locked.version.clone()),
                update_available: changed,
                message: if changed {
                    "archive checksum changed; update will download and validate its declared version"
                        .to_string()
                } else {
                    "archive checksum matches the verified lock".to_string()
                },
            });
        }
        let index = load_index(&config.source)?;
        let release = select_release(&index, &config.channel, config.version.as_deref())?;
        let changed = locked.is_none_or(|locked| {
            locked.version != release.version
                || locked.resolved_ref != format!("sha256:{}", release.sha256)
        });
        return Ok(RemoteObservation {
            resolved_ref: Some(format!("sha256:{}", release.sha256)),
            latest_version: Some(release.version.clone()),
            update_available: changed,
            message: if changed {
                format!(
                    "release {} is available on channel {}",
                    release.version, release.channel
                )
            } else {
                format!("release {} is current", release.version)
            },
        });
    }
    let path = PathBuf::from(&config.source).canonicalize()?;
    let (manifest, _) = validate_source_root(&path)?;
    let digest = digest_tree(&path)?;
    let changed = locked.is_none_or(|locked| locked.digest != digest);
    Ok(RemoteObservation {
        resolved_ref: Some(format!("sha256:{digest}")),
        latest_version: Some(manifest.metadata.version),
        update_available: changed,
        message: if changed {
            "local Source content changed".to_string()
        } else {
            "local Source content matches the verified lock".to_string()
        },
    })
}

fn load_index(url: &str) -> RainyResult<RainySourceIndex> {
    let index: RainySourceIndex = serde_yaml::from_str(&http_get_text(url)?)?;
    if index.api_version != "rainy.dev/v1" || index.kind != "RainySourceIndex" {
        return Err(RainyError::config(
            "SOURCE_INDEX_INVALID",
            "Source index must use apiVersion rainy.dev/v1 and kind RainySourceIndex",
        ));
    }
    validate_source_name(&index.metadata.name)?;
    validate_extension_fields(&index.extension_fields)?;
    if index.releases.is_empty() {
        return Err(RainyError::config(
            "SOURCE_INDEX_INVALID",
            "Source index must declare at least one release",
        ));
    }
    for release in &index.releases {
        Version::parse(&release.version).map_err(|error| {
            RainyError::config(
                "SOURCE_INDEX_INVALID",
                format!("release version {} is invalid: {error}", release.version),
            )
        })?;
        validate_sha256(&release.sha256)?;
        resolve_index_url(url, &release.url)?;
    }
    Ok(index)
}

fn select_release<'a>(
    index: &'a RainySourceIndex,
    channel: &str,
    wanted_version: Option<&str>,
) -> RainyResult<&'a SourceRelease> {
    let mut releases = index
        .releases
        .iter()
        .filter(|release| {
            wanted_version.map_or(release.channel == channel, |version| {
                release.version == version
            })
        })
        .collect::<Vec<_>>();
    releases.sort_by(|left, right| {
        Version::parse(&right.version)
            .expect("validated release")
            .cmp(&Version::parse(&left.version).expect("validated release"))
    });
    releases.into_iter().next().ok_or_else(|| {
        RainyError::registry(
            "SOURCE_RELEASE_NOT_FOUND",
            wanted_version.map_or_else(
                || format!("Source index has no release on channel {channel}"),
                |version| format!("Source index has no release version {version}"),
            ),
        )
    })
}

fn resolve_index_url(index_url: &str, release_url: &str) -> RainyResult<String> {
    let base = url::Url::parse(index_url).map_err(|error| {
        RainyError::config(
            "SOURCE_INDEX_INVALID",
            format!("invalid index URL: {error}"),
        )
    })?;
    let resolved = base.join(release_url).map_err(|error| {
        RainyError::config(
            "SOURCE_INDEX_INVALID",
            format!("invalid release URL {release_url}: {error}"),
        )
    })?;
    crate::security::validate_http(resolved.as_str(), true).map_err(|reason| {
        RainyError::config(
            "SOURCE_INDEX_INVALID",
            format!("release URL is not allowed: {reason}"),
        )
    })?;
    if !is_archive_url(resolved.as_str()) {
        return Err(RainyError::config(
            "SOURCE_INDEX_INVALID",
            "Source Index releases must point to .zip, .tar.gz, or .tgz archives",
        ));
    }
    Ok(resolved.to_string())
}

fn normalize_local_config(workspace: &Path, config: &mut SourceConfig) -> RainyResult<()> {
    if source_type(config) != "local" {
        return Ok(());
    }
    let path = PathBuf::from(&config.source);
    let path = if path.is_absolute() {
        path
    } else {
        workspace.join(path)
    };
    config.source = path
        .canonicalize()
        .map_err(|error| {
            RainyError::registry(
                "SOURCE_NOT_FOUND",
                format!("Source path is not available: {} ({error})", path.display()),
            )
        })?
        .to_string_lossy()
        .to_string();
    Ok(())
}

fn cache_source(name: &str, root: &Path, digest: &str) -> RainyResult<PathBuf> {
    let target = source_cache_root()?.join(name).join(&digest[..16]);
    if target.is_dir() {
        if digest_tree(&target)? == digest {
            return Ok(target);
        }
        return Err(RainyError::registry(
            "SOURCE_CACHE_DIGEST_MISMATCH",
            format!(
                "existing Source cache does not match its immutable digest: {}",
                target.display()
            ),
        ));
    }
    let parent = target.parent().ok_or_else(|| {
        RainyError::config(
            "SOURCE_CACHE_INVALID",
            "cannot determine Source cache parent",
        )
    })?;
    std::fs::create_dir_all(parent)?;
    let staging = parent.join(format!(".{}.tmp.{}", &digest[..16], std::process::id()));
    if staging.exists() {
        std::fs::remove_dir_all(&staging)?;
    }
    copy_tree(root, &staging)?;
    if digest_tree(&staging)? != digest {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(RainyError::registry(
            "SOURCE_CACHE_DIGEST_MISMATCH",
            "Source cache digest changed while copying",
        ));
    }
    match std::fs::rename(&staging, &target) {
        Ok(()) => Ok(target),
        Err(error) if target.is_dir() => {
            let _ = std::fs::remove_dir_all(&staging);
            if digest_tree(&target)? == digest {
                Ok(target)
            } else {
                Err(error.into())
            }
        }
        Err(error) => Err(error.into()),
    }
}

fn source_cache_root() -> RainyResult<PathBuf> {
    Ok(crate::paths::rainy_home()?.join("sources"))
}

fn catalog_path() -> RainyResult<PathBuf> {
    Ok(crate::paths::rainy_home()?.join(CATALOG_FILE))
}

fn lock_path() -> RainyResult<PathBuf> {
    Ok(crate::paths::rainy_home()?.join(LOCK_FILE))
}

fn lock_source_state() -> RainyResult<std::fs::File> {
    let root = crate::paths::rainy_home()?;
    std::fs::create_dir_all(&root)?;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(root.join(".sources.state.lock"))?;
    file.lock_exclusive()?;
    Ok(file)
}

fn load_catalog() -> RainyResult<SourceCatalog> {
    let path = catalog_path()?;
    if !path.is_file() {
        return Ok(SourceCatalog::default());
    }
    let catalog: SourceCatalog = serde_yaml::from_str(&std::fs::read_to_string(path)?)?;
    if catalog.api_version != "rainy.dev/v1" || catalog.kind != "RainySourceCatalog" {
        return Err(RainyError::config(
            "SOURCE_CATALOG_INVALID",
            "sources.yaml must use apiVersion rainy.dev/v1 and kind RainySourceCatalog",
        ));
    }
    Ok(catalog)
}

fn load_lock() -> RainyResult<SourceLock> {
    let path = lock_path()?;
    if !path.is_file() {
        return Ok(SourceLock {
            lockfile_version: source_lock_version(),
            sources: BTreeMap::new(),
        });
    }
    let lock: SourceLock = serde_yaml::from_str(&std::fs::read_to_string(path)?)?;
    if lock.lockfile_version != source_lock_version() {
        return Err(RainyError::config(
            "SOURCE_LOCK_VERSION_UNSUPPORTED",
            format!("unsupported Source lock version: {}", lock.lockfile_version),
        ));
    }
    Ok(lock)
}

fn save_catalog(catalog: &SourceCatalog) -> RainyResult<()> {
    save_yaml(&catalog_path()?, catalog)
}

fn save_lock(lock: &SourceLock) -> RainyResult<()> {
    save_yaml(&lock_path()?, lock)
}

fn save_yaml(path: &Path, value: &impl Serialize) -> RainyResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_yaml::to_string(value)?;
    let temporary = path.with_extension(format!("tmp.{}", std::process::id()));
    std::fs::write(&temporary, content)?;
    if path.exists() {
        let backup = path.with_extension(format!("backup.{}", std::process::id()));
        std::fs::rename(path, &backup)?;
        match std::fs::rename(&temporary, path) {
            Ok(()) => {
                std::fs::remove_file(backup)?;
                Ok(())
            }
            Err(error) => {
                let _ = std::fs::rename(&backup, path);
                Err(error.into())
            }
        }
    } else {
        std::fs::rename(temporary, path)?;
        Ok(())
    }
}

fn report(operation: &str, status: &str, sources: Vec<SourceInfo>) -> SourceReport {
    SourceReport {
        protocol_version: "rainy.source-report.v1".to_string(),
        operation: operation.to_string(),
        status: status.to_string(),
        sources,
    }
}

fn preview_info(name: &str, config: &SourceConfig) -> SourceInfo {
    SourceInfo {
        name: name.to_string(),
        source_type: source_type(config).to_string(),
        source: config.source.clone(),
        requested_ref: requested_ref(config, source_type(config)),
        resolved_ref: None,
        current_version: None,
        latest_version: config.version.clone(),
        digest: config.sha256.clone(),
        cache_path: None,
        update_available: None,
        state: "preview".to_string(),
        message: Some(
            "Source will be downloaded and fully validated before registration".to_string(),
        ),
        contents: Vec::new(),
    }
}

fn info_from_prepared(
    name: &str,
    config: &SourceConfig,
    prepared: &PreparedSource,
    manifest: &RainySourceManifest,
    state: &str,
) -> RainyResult<SourceInfo> {
    Ok(SourceInfo {
        name: name.to_string(),
        source_type: prepared.source_type.clone(),
        source: config.source.clone(),
        requested_ref: requested_ref(config, &prepared.source_type),
        resolved_ref: Some(prepared.resolved_ref.clone()),
        current_version: Some(manifest.metadata.version.clone()),
        latest_version: prepared
            .release_version
            .clone()
            .or_else(|| Some(manifest.metadata.version.clone())),
        digest: Some(digest_tree(&prepared.root)?),
        cache_path: None,
        update_available: Some(false),
        state: state.to_string(),
        message: Some("Source manifest and all declared contents are valid".to_string()),
        contents: content_infos(manifest),
    })
}

fn info_from_locked(
    name: &str,
    config: &SourceConfig,
    locked: &LockedSource,
    manifest: &RainySourceManifest,
    state: &str,
) -> SourceInfo {
    SourceInfo {
        name: name.to_string(),
        source_type: locked.source_type.clone(),
        source: config.source.clone(),
        requested_ref: locked.requested_ref.clone(),
        resolved_ref: Some(locked.resolved_ref.clone()),
        current_version: Some(locked.version.clone()),
        latest_version: Some(locked.version.clone()),
        digest: Some(locked.digest.clone()),
        cache_path: Some(locked.cache_path.clone()),
        update_available: Some(false),
        state: state.to_string(),
        message: Some("Source was validated and stored in the immutable cache".to_string()),
        contents: content_infos(manifest),
    }
}

fn info_from_lock(name: &str, config: &SourceConfig, locked: Option<&LockedSource>) -> SourceInfo {
    SourceInfo {
        name: name.to_string(),
        source_type: locked
            .map(|locked| locked.source_type.clone())
            .unwrap_or_else(|| source_type(config).to_string()),
        source: config.source.clone(),
        requested_ref: locked
            .and_then(|locked| locked.requested_ref.clone())
            .or_else(|| requested_ref(config, source_type(config))),
        resolved_ref: locked.map(|locked| locked.resolved_ref.clone()),
        current_version: locked.map(|locked| locked.version.clone()),
        latest_version: locked.map(|locked| locked.version.clone()),
        digest: locked.map(|locked| locked.digest.clone()),
        cache_path: locked.map(|locked| locked.cache_path.clone()),
        update_available: None,
        state: if locked.is_some() {
            "cached"
        } else {
            "preview"
        }
        .to_string(),
        message: None,
        contents: locked
            .map(|locked| {
                locked
                    .contents
                    .iter()
                    .map(|content| SourceContentInfo {
                        id: content.id.clone(),
                        content_type: content.content_type.clone(),
                        path: content.path.clone(),
                        version: content.version.clone(),
                        default_target: None,
                        required: false,
                        resolved_path: Some(
                            PathBuf::from(&locked.cache_path)
                                .join(&content.path)
                                .to_string_lossy()
                                .to_string(),
                        ),
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
}

fn content_infos(manifest: &RainySourceManifest) -> Vec<SourceContentInfo> {
    manifest
        .contents
        .iter()
        .map(|content| SourceContentInfo {
            id: content.id.clone(),
            content_type: content.content_type.as_str().to_string(),
            path: content.path.clone(),
            version: content.version.clone(),
            default_target: content.default_target.clone(),
            required: content.required,
            resolved_path: None,
        })
        .collect()
}

fn source_type(config: &SourceConfig) -> &'static str {
    if config.source.starts_with("git+") {
        "git"
    } else if config.source.starts_with("https://") || config.source.starts_with("http://") {
        if is_archive_url(&config.source) {
            "archive"
        } else {
            "index"
        }
    } else {
        "local"
    }
}

fn requested_ref(config: &SourceConfig, source_type: &str) -> Option<String> {
    match source_type {
        "git" => Some(
            config
                .reference
                .clone()
                .unwrap_or_else(|| "main".to_string()),
        ),
        "index" => Some(
            config
                .version
                .clone()
                .unwrap_or_else(|| format!("channel:{}", config.channel)),
        ),
        _ => None,
    }
}

fn validate_source_name(name: &str) -> RainyResult<()> {
    let mut bytes = name.bytes();
    if name.len() <= 64
        && bytes
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Ok(());
    }
    Err(RainyError::config(
        "SOURCE_NAME_INVALID",
        "Source and content names must contain 1-64 ASCII letters, digits, '-' or '_'",
    ))
}

fn validate_extension_fields(fields: &BTreeMap<String, serde_yaml::Value>) -> RainyResult<()> {
    if let Some(field) = fields.keys().find(|field| !field.starts_with("x-")) {
        return Err(RainyError::config(
            "SOURCE_MANIFEST_INVALID",
            format!("unknown Source field {field}; enterprise extensions must use x-*"),
        ));
    }
    Ok(())
}

fn validate_git_ref(reference: &str) -> RainyResult<()> {
    if reference.trim().is_empty()
        || reference.starts_with('-')
        || reference.chars().any(char::is_control)
    {
        return Err(RainyError::config(
            "SOURCE_GIT_REF_INVALID",
            "Git Source ref must be a safe non-empty branch, tag, or commit",
        ));
    }
    Ok(())
}

fn git_remote_ref(url: &str, reference: &str) -> RainyResult<String> {
    crate::security::validate_git(url, false)
        .map_err(|reason| RainyError::registry("SOURCE_GIT_URL_INVALID", reason))?;
    validate_git_ref(reference)?;
    if reference.len() == 40 && reference.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Ok(reference.to_ascii_lowercase());
    }
    let mut command = Command::new("git");
    command.arg("ls-remote").arg(url).args([
        reference,
        &format!("refs/heads/{reference}"),
        &format!("refs/tags/{reference}"),
        &format!("refs/tags/{reference}^{{}}"),
    ]);
    let output = crate::process::run_command(
        command,
        "git",
        Duration::from_secs(15),
        crate::process::DEFAULT_OUTPUT_LIMIT,
    )?;
    if !output.success() {
        return Err(RainyError::registry(
            "SOURCE_GIT_REMOTE_FAILED",
            output.stderr.trim().to_string(),
        ));
    }
    let lines = output.stdout.lines().collect::<Vec<_>>();
    let selected = lines
        .iter()
        .find(|line| line.ends_with("^{}"))
        .or_else(|| lines.first())
        .and_then(|line| line.split_whitespace().next())
        .filter(|value| value.len() == 40)
        .ok_or_else(|| {
            RainyError::registry(
                "SOURCE_GIT_REF_NOT_FOUND",
                format!("remote Git ref was not found: {reference}"),
            )
        })?;
    Ok(selected.to_string())
}

fn run_git(repository: &Path, args: &[&str]) -> RainyResult<()> {
    let mut command = Command::new("git");
    command.arg("-C").arg(repository).args(args);
    let output = crate::process::run_command(
        command,
        "git",
        Duration::from_secs(900),
        crate::process::DEFAULT_OUTPUT_LIMIT,
    )?;
    if output.success() {
        Ok(())
    } else {
        Err(RainyError::registry(
            "SOURCE_GIT_FETCH_FAILED",
            output.stderr.trim().to_string(),
        ))
    }
}

fn git_output(repository: &Path, args: &[&str]) -> RainyResult<String> {
    let mut command = Command::new("git");
    command.arg("-C").arg(repository).args(args);
    let output = crate::process::run_command(
        command,
        "git",
        Duration::from_secs(30),
        crate::process::DEFAULT_OUTPUT_LIMIT,
    )?;
    if output.success() {
        Ok(output.stdout.trim().to_string())
    } else {
        Err(RainyError::registry(
            "SOURCE_GIT_FETCH_FAILED",
            output.stderr.trim().to_string(),
        ))
    }
}

fn http_get_text(url: &str) -> RainyResult<String> {
    let bytes = http_get_bytes(url, 5 * 1024 * 1024)?;
    String::from_utf8(bytes).map_err(|error| {
        RainyError::registry(
            "SOURCE_DOWNLOAD_INVALID",
            format!("Source response is not UTF-8: {error}"),
        )
    })
}

fn http_get_bytes(url: &str, limit: u64) -> RainyResult<Vec<u8>> {
    crate::security::validate_http(url, true).map_err(|reason| {
        RainyError::registry(
            "SOURCE_DOWNLOAD_URL_INVALID",
            format!("Source URL is not allowed: {reason}"),
        )
    })?;
    let response = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(3))
        .timeout_read(Duration::from_secs(30))
        .timeout_write(Duration::from_secs(10))
        .redirects(3)
        .build()
        .get(url)
        .set("User-Agent", "rainy-cli")
        .call()
        .map_err(|error| {
            RainyError::registry(
                "SOURCE_DOWNLOAD_FAILED",
                format!("Source download failed: {error}"),
            )
        })?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(RainyError::registry(
            "SOURCE_DOWNLOAD_LIMIT_EXCEEDED",
            format!("Source download exceeds the {limit}-byte limit"),
        ));
    }
    Ok(bytes)
}

fn extract_zip(bytes: &[u8], target: &Path) -> RainyResult<()> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|error| {
        RainyError::registry("SOURCE_ARCHIVE_INVALID", format!("invalid ZIP: {error}"))
    })?;
    if archive.len() > MAX_SOURCE_ENTRIES {
        return Err(RainyError::registry(
            "SOURCE_ARCHIVE_LIMIT_EXCEEDED",
            "archive contains too many entries",
        ));
    }
    let mut total = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| RainyError::registry("SOURCE_ARCHIVE_INVALID", error.to_string()))?;
        let relative = entry.enclosed_name().ok_or_else(|| {
            RainyError::registry(
                "SOURCE_ARCHIVE_UNSAFE_ENTRY",
                format!("unsafe ZIP path: {}", entry.name()),
            )
        })?;
        let path = target.join(relative);
        if entry.is_dir() {
            std::fs::create_dir_all(path)?;
            continue;
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(RainyError::registry(
                "SOURCE_ARCHIVE_UNSAFE_ENTRY",
                "symbolic links are not allowed in Source archives",
            ));
        }
        total = total.saturating_add(entry.size());
        if total > MAX_SOURCE_BYTES {
            return Err(RainyError::registry(
                "SOURCE_ARCHIVE_LIMIT_EXCEEDED",
                "archive expands beyond the Source size limit",
            ));
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut output = std::fs::File::create(path)?;
        std::io::copy(&mut entry, &mut output)?;
    }
    Ok(())
}

fn extract_tar_gz(bytes: &[u8], target: &Path) -> RainyResult<()> {
    let decoder = GzDecoder::new(Cursor::new(bytes));
    let mut archive = tar::Archive::new(decoder);
    let mut count = 0_usize;
    let mut total = 0_u64;
    for entry in archive.entries().map_err(|error| {
        RainyError::registry("SOURCE_ARCHIVE_INVALID", format!("invalid tar.gz: {error}"))
    })? {
        let mut entry = entry
            .map_err(|error| RainyError::registry("SOURCE_ARCHIVE_INVALID", error.to_string()))?;
        count += 1;
        if count > MAX_SOURCE_ENTRIES {
            return Err(RainyError::registry(
                "SOURCE_ARCHIVE_LIMIT_EXCEEDED",
                "archive contains too many entries",
            ));
        }
        let entry_type = entry.header().entry_type();
        if !entry_type.is_file() && !entry_type.is_dir() {
            return Err(RainyError::registry(
                "SOURCE_ARCHIVE_UNSAFE_ENTRY",
                "Source archives may contain only regular files and directories",
            ));
        }
        let relative = entry
            .path()
            .map_err(|error| RainyError::registry("SOURCE_ARCHIVE_INVALID", error.to_string()))?;
        let relative = safe_path(&relative, "SOURCE_ARCHIVE_UNSAFE_ENTRY")?;
        let path = target.join(relative);
        if entry_type.is_dir() {
            std::fs::create_dir_all(path)?;
            continue;
        }
        total = total.saturating_add(entry.size());
        if total > MAX_SOURCE_BYTES {
            return Err(RainyError::registry(
                "SOURCE_ARCHIVE_LIMIT_EXCEEDED",
                "archive expands beyond the Source size limit",
            ));
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut output = std::fs::File::create(path)?;
        std::io::copy(&mut entry, &mut output)?;
    }
    Ok(())
}

fn normalize_source_root(extracted: &Path) -> RainyResult<PathBuf> {
    if extracted.join(SOURCE_MANIFEST).is_file() {
        return Ok(extracted.to_path_buf());
    }
    let mut directories = std::fs::read_dir(extracted)?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    directories.sort();
    if directories.len() == 1 && directories[0].join(SOURCE_MANIFEST).is_file() {
        return Ok(directories.remove(0));
    }
    Err(RainyError::config(
        "SOURCE_MANIFEST_NOT_FOUND",
        format!("archive must contain one root {SOURCE_MANIFEST}"),
    ))
}

fn copy_tree(source: &Path, target: &Path) -> RainyResult<Vec<String>> {
    let mut files = Vec::new();
    for entry in WalkDir::new(source).follow_links(false) {
        let entry = entry
            .map_err(|error| RainyError::registry("SOURCE_TREE_INVALID", error.to_string()))?;
        let relative = entry.path().strip_prefix(source).unwrap_or(entry.path());
        if relative.as_os_str().is_empty() || starts_with_git(relative) {
            continue;
        }
        if entry.file_type().is_symlink() {
            return Err(RainyError::registry(
                "SOURCE_TREE_UNSAFE_ENTRY",
                format!("symbolic links are not allowed: {}", relative.display()),
            ));
        }
        let destination = target.join(relative);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(destination)?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(entry.path(), &destination)?;
            files.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
    files.sort();
    Ok(files)
}

fn digest_tree(root: &Path) -> RainyResult<String> {
    if root.is_file() {
        return Ok(hex(&Sha256::digest(std::fs::read(root)?)));
    }
    let mut files = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| RainyError::registry("SOURCE_TREE_INVALID", error.to_string()))?;
    files.retain(|entry| entry.file_type().is_file());
    files.retain(|entry| {
        entry
            .path()
            .strip_prefix(root)
            .is_ok_and(|relative| !starts_with_git(relative))
    });
    files.sort_by_key(|entry| entry.path().to_path_buf());
    let mut digest = Sha256::new();
    for entry in files {
        let relative = entry.path().strip_prefix(root).unwrap_or(entry.path());
        digest.update(relative.to_string_lossy().replace('\\', "/").as_bytes());
        digest.update([0]);
        digest.update(std::fs::read(entry.path())?);
        digest.update([0]);
    }
    Ok(hex(&digest.finalize()))
}

fn starts_with_git(path: &Path) -> bool {
    path.components()
        .next()
        .is_some_and(|component| component.as_os_str() == ".git")
}

fn safe_relative_path(value: &str, code: &'static str) -> RainyResult<PathBuf> {
    safe_path(Path::new(value), code)
}

fn safe_path(path: &Path, code: &'static str) -> RainyResult<PathBuf> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err(RainyError::config(
            code,
            "path must be a non-empty relative path",
        ));
    }
    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(RainyError::config(code, "path traversal is not allowed"));
    }
    Ok(path
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value),
            Component::CurDir => None,
            _ => None,
        })
        .collect())
}

fn validate_sha256(value: &str) -> RainyResult<()> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(RainyError::registry(
            "SOURCE_ARCHIVE_CHECKSUM_INVALID",
            "Source archive checksum must be a 64-character SHA-256 digest",
        ))
    }
}

fn is_archive_url(url: &str) -> bool {
    let path = url_path(url);
    path.ends_with(".zip") || path.ends_with(".tar.gz") || path.ends_with(".tgz")
}

fn url_path(url: &str) -> String {
    url.split(['?', '#'])
        .next()
        .unwrap_or(url)
        .to_ascii_lowercase()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn create_project(options: SourceProjectOptions<'_>) -> RainyResult<CommandOutput> {
    validate_project_name(&options.name)?;
    let catalog = load_catalog()?;
    let config = catalog.sources.get(&options.source).ok_or_else(|| {
        RainyError::config(
            "SOURCE_NOT_FOUND",
            format!(
                "Source '{}' is not configured; run rainy source add --help",
                options.source
            ),
        )
    })?;
    let lock = load_lock()?;
    let locked = lock.sources.get(&options.source).ok_or_else(|| {
        RainyError::config(
            "SOURCE_NOT_SYNCHRONIZED",
            format!(
                "Source '{}' has no verified cache; run rainy source sync {} --apply",
                options.source, options.source
            ),
        )
    })?;
    let cache = PathBuf::from(&locked.cache_path);
    if !cache.is_dir() || digest_tree(&cache)? != locked.digest {
        return Err(RainyError::registry(
            "SOURCE_CACHE_DIGEST_MISMATCH",
            format!(
                "verified Source cache is missing or changed; run rainy source sync {} --apply",
                options.source
            ),
        ));
    }
    let (manifest, _) = validate_source_root(&cache)?;
    let template = select_template(
        &manifest,
        options.template.as_deref(),
        options.interactive,
        options.no_color,
        options.progress,
    )?;
    let modules = select_modules(
        &manifest,
        &options.modules,
        options.interactive,
        options.no_color,
        options.progress,
    )?;
    let destination = options.base_dir.join(&options.name);
    if destination.exists() {
        return Err(RainyError::config(
            "PROJECT_ALREADY_EXISTS",
            format!(
                "project destination already exists: {}",
                destination.display()
            ),
        ));
    }
    let next_commands = project_git_commands(&destination, options.git_url.as_deref());
    if options.dry_run {
        return Ok(CommandOutput::SourceProject {
            status: "dry-run",
            project: options.name,
            path: destination.to_string_lossy().to_string(),
            source: options.source,
            source_version: locked.version.clone(),
            resolved_ref: locked.resolved_ref.clone(),
            template: template.id.clone(),
            modules: modules.iter().map(|content| content.id.clone()).collect(),
            files: Vec::new(),
            remote_url: options.git_url,
            next_commands,
        });
    }

    options.progress.detail(format!(
        "Composing template {} with {} modules",
        template.id,
        modules.len()
    ));
    std::fs::create_dir_all(&options.base_dir)?;
    let staging = tempfile::Builder::new()
        .prefix(".rainy-source-project-")
        .tempdir_in(&options.base_dir)?;
    let rendered = staging.path().join("rendered");
    std::fs::create_dir_all(&rendered)?;
    let mut files = render_content_tree(
        &cache.join(safe_relative_path(
            &template.path,
            "SOURCE_CONTENT_PATH_INVALID",
        )?),
        &rendered,
        Path::new(""),
        &options.name,
        &options.package,
    )?;
    for module in &modules {
        let target = module.default_target.as_deref().ok_or_else(|| {
            RainyError::config(
                "SOURCE_MODULE_TARGET_REQUIRED",
                format!("workspace module {} must declare defaultTarget", module.id),
            )
        })?;
        files.extend(render_content_tree(
            &cache.join(safe_relative_path(
                &module.path,
                "SOURCE_CONTENT_PATH_INVALID",
            )?),
            &rendered,
            &safe_relative_path(target, "SOURCE_CONTENT_TARGET_INVALID")?,
            &options.name,
            &options.package,
        )?);
    }
    let source_lock = ProjectSourceLock {
        lockfile_version: 1,
        source: ProjectSourceIdentity {
            name: options.source.clone(),
            version: locked.version.clone(),
            resolved_ref: locked.resolved_ref.clone(),
            digest: locked.digest.clone(),
            origin: ProjectSourceOrigin::from_config(config),
        },
        template: template.id.clone(),
        modules: modules.iter().map(|content| content.id.clone()).collect(),
        created_at: Utc::now(),
    };
    std::fs::create_dir_all(rendered.join(".rainy"))?;
    std::fs::write(
        rendered.join(".rainy/project-source.lock"),
        serde_yaml::to_string(&source_lock)?,
    )?;
    files.push(".rainy/project-source.lock".to_string());
    crate::config::load_config(&rendered)?;
    crate::config::load_lock(&rendered)?;
    files.sort();
    files.dedup();
    std::fs::rename(&rendered, &destination).map_err(|error| {
        RainyError::config(
            "SOURCE_PROJECT_CREATE_FAILED",
            format!(
                "cannot atomically create {}: {error}",
                destination.display()
            ),
        )
    })?;
    Ok(CommandOutput::SourceProject {
        status: "created",
        project: options.name,
        path: destination.to_string_lossy().to_string(),
        source: options.source,
        source_version: locked.version.clone(),
        resolved_ref: locked.resolved_ref.clone(),
        template: template.id.clone(),
        modules: modules.iter().map(|content| content.id.clone()).collect(),
        files,
        remote_url: options.git_url,
        next_commands,
    })
}

fn select_template<'a>(
    manifest: &'a RainySourceManifest,
    selected: Option<&str>,
    interactive: bool,
    no_color: bool,
    progress: &ProgressReporter,
) -> RainyResult<&'a SourceContent> {
    let templates = manifest
        .contents
        .iter()
        .filter(|content| content.content_type == SourceContentType::ProjectTemplate)
        .collect::<Vec<_>>();
    if templates.is_empty() {
        return Err(RainyError::config(
            "SOURCE_TEMPLATE_NOT_FOUND",
            "Source does not declare a project-template",
        ));
    }
    if let Some(selected) = selected {
        return templates
            .into_iter()
            .find(|content| content.id == selected)
            .ok_or_else(|| {
                RainyError::config(
                    "SOURCE_TEMPLATE_NOT_FOUND",
                    format!("Source template was not found: {selected}"),
                )
            });
    }
    if templates.len() == 1 {
        return Ok(templates[0]);
    }
    if !interactive {
        return Err(RainyError::config(
            "SOURCE_TEMPLATE_SELECTION_REQUIRED",
            "non-interactive creation requires --template <TEMPLATE_ID>",
        ));
    }
    let _suspension = progress.suspend();
    let labels = templates
        .iter()
        .map(|content| {
            format!(
                "{}  {}",
                content.id,
                content.description.as_deref().unwrap_or("")
            )
        })
        .collect::<Vec<_>>();
    let prompt = Select::new("Select the base project template", labels.clone())
        .with_help_message("Type to search; Up/Down move; Enter confirms; Esc cancels");
    let selected = if no_color {
        prompt
            .with_render_config(inquire::ui::RenderConfig::empty())
            .prompt()
    } else {
        prompt.prompt()
    }
    .map_err(prompt_error)?;
    let index = labels
        .iter()
        .position(|label| label == &selected)
        .unwrap_or(0);
    Ok(templates[index])
}

fn select_modules<'a>(
    manifest: &'a RainySourceManifest,
    selected: &[String],
    interactive: bool,
    no_color: bool,
    progress: &ProgressReporter,
) -> RainyResult<Vec<&'a SourceContent>> {
    let modules = manifest
        .contents
        .iter()
        .filter(|content| content.content_type == SourceContentType::WorkspaceModule)
        .collect::<Vec<_>>();
    if !selected.is_empty() {
        let mut resolved = Vec::new();
        for id in selected {
            let module = modules
                .iter()
                .find(|module| module.id == *id)
                .ok_or_else(|| {
                    RainyError::config(
                        "SOURCE_MODULE_NOT_FOUND",
                        format!("Source workspace module was not found: {id}"),
                    )
                })?;
            resolved.push(*module);
        }
        for module in &modules {
            if module.required && !resolved.iter().any(|selected| selected.id == module.id) {
                resolved.push(*module);
            }
        }
        return Ok(resolved);
    }
    if modules.is_empty() {
        return Ok(Vec::new());
    }
    if !interactive {
        return Ok(modules
            .into_iter()
            .filter(|module| module.required)
            .collect());
    }
    let _suspension = progress.suspend();
    let labels = modules
        .iter()
        .map(|content| {
            format!(
                "{}  {}",
                content.id,
                content.description.as_deref().unwrap_or("")
            )
        })
        .collect::<Vec<_>>();
    let defaults = modules
        .iter()
        .enumerate()
        .filter_map(|(index, module)| module.required.then_some(index))
        .collect::<Vec<_>>();
    let prompt = MultiSelect::new("Select workspace modules", labels.clone())
        .with_default(&defaults)
        .with_page_size(12)
        .with_help_message("Type to search; Space toggles; Right all; Left clear; Enter confirms");
    let selected = if no_color {
        prompt
            .with_render_config(inquire::ui::RenderConfig::empty())
            .prompt()
    } else {
        prompt.prompt()
    }
    .map_err(prompt_error)?;
    let mut resolved = selected
        .iter()
        .filter_map(|label| labels.iter().position(|candidate| candidate == label))
        .map(|index| modules[index])
        .collect::<Vec<_>>();
    for module in modules {
        if module.required && !resolved.iter().any(|selected| selected.id == module.id) {
            resolved.push(module);
        }
    }
    Ok(resolved)
}

fn prompt_error(error: inquire::InquireError) -> RainyError {
    RainyError::action(
        "CANCELLED",
        match error {
            inquire::InquireError::OperationCanceled
            | inquire::InquireError::OperationInterrupted => {
                "Source selection cancelled".to_string()
            }
            other => format!("Source selection failed: {other}"),
        },
    )
}

fn render_content_tree(
    source: &Path,
    destination_root: &Path,
    target: &Path,
    project_name: &str,
    package: &str,
) -> RainyResult<Vec<String>> {
    let context = serde_json::json!({
        "project": { "name": project_name },
        "package": { "java": package },
        "packagePath": package.replace('.', "/")
    });
    let handlebars = Handlebars::new();
    let mut files = Vec::new();
    for entry in WalkDir::new(source).follow_links(false) {
        let entry = entry
            .map_err(|error| RainyError::config("SOURCE_TEMPLATE_INVALID", error.to_string()))?;
        let relative = entry.path().strip_prefix(source).unwrap_or(entry.path());
        if relative.as_os_str().is_empty() || starts_with_git(relative) {
            continue;
        }
        if entry.file_type().is_symlink() {
            return Err(RainyError::config(
                "SOURCE_TEMPLATE_UNSAFE_ENTRY",
                format!("symbolic links are not allowed: {}", relative.display()),
            ));
        }
        if entry.file_type().is_dir() {
            continue;
        }
        let relative_text = relative.to_string_lossy().replace('\\', "/");
        let rendered_path = handlebars
            .render_template(&relative_text, &context)
            .map_err(|error| {
                RainyError::config("SOURCE_TEMPLATE_RENDER_FAILED", error.to_string())
            })?;
        let rendered_path = rendered_path
            .strip_suffix(".hbs")
            .unwrap_or(&rendered_path)
            .to_string();
        let rendered_relative = target.join(safe_relative_path(
            &rendered_path,
            "SOURCE_TEMPLATE_PATH_INVALID",
        )?);
        let destination = destination_root.join(&rendered_relative);
        if destination.exists() {
            return Err(RainyError::config(
                "SOURCE_MODULE_CONFLICT",
                format!(
                    "multiple Source contents target the same path: {}",
                    rendered_relative.display()
                ),
            ));
        }
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let bytes = std::fs::read(entry.path())?;
        if relative_text.ends_with(".hbs") {
            let text = std::str::from_utf8(&bytes).map_err(|error| {
                RainyError::config(
                    "SOURCE_TEMPLATE_RENDER_FAILED",
                    format!("template {} is not UTF-8: {error}", relative.display()),
                )
            })?;
            let rendered = handlebars
                .render_template(text, &context)
                .map_err(|error| {
                    RainyError::config("SOURCE_TEMPLATE_RENDER_FAILED", error.to_string())
                })?;
            std::fs::write(&destination, rendered)?;
        } else {
            std::fs::write(&destination, bytes)?;
        }
        files.push(rendered_relative.to_string_lossy().replace('\\', "/"));
    }
    Ok(files)
}

fn validate_project_name(name: &str) -> RainyResult<()> {
    let path = Path::new(name);
    if name.trim().is_empty()
        || name == "."
        || name == ".."
        || path.components().count() != 1
        || !matches!(path.components().next(), Some(Component::Normal(_)))
    {
        return Err(RainyError::config(
            "PROJECT_NAME_INVALID",
            "project name must be one directory name without path separators",
        ));
    }
    Ok(())
}

fn project_git_commands(path: &Path, remote: Option<&str>) -> Vec<String> {
    let quoted_path = shell_quote(&path.to_string_lossy());
    let mut commands = vec![format!("cd {quoted_path}"), "git init -b main".to_string()];
    if let Some(remote) = remote {
        commands.push(format!("git remote add origin {}", shell_quote(remote)));
    }
    commands.extend([
        "git add .".to_string(),
        "git commit -m 'Initial commit'".to_string(),
    ]);
    if remote.is_some() {
        commands.push("git push -u origin main".to_string());
    }
    commands
}

fn shell_quote(value: &str) -> String {
    if value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'_' | b'-'))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}
