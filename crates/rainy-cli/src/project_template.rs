use crate::cli::{TemplateCommand, TemplateSubcommand};
use crate::error::{RainyError, RainyResult};
use crate::output::CommandOutput;
use crate::progress::ProgressReporter;
use chrono::{DateTime, Utc};
use handlebars::Handlebars;
use inquire::Select;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const MAX_TEMPLATE_ENTRIES: usize = 10_000;
const MAX_TEMPLATE_BYTES: u64 = 512 * 1024 * 1024;
const PROJECT_TEMPLATE_LOCK: &str = ".rainy/project-template.lock";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectTemplateLock {
    lockfile_version: u32,
    template: String,
    source: ProjectTemplateLockSource,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectTemplateLockSource {
    #[serde(skip_serializing_if = "Option::is_none")]
    remote: Option<String>,
    url: String,
    requested_ref: String,
    resolved_ref: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTemplateStatusReport {
    pub protocol_version: &'static str,
    pub status: &'static str,
    pub template: String,
    pub remote: Option<String>,
    pub source: String,
    pub requested_ref: String,
    pub resolved_ref: String,
    pub latest_ref: Option<String>,
    pub update_available: Option<bool>,
    pub created_at: DateTime<Utc>,
    pub message: String,
}

pub struct ProjectTemplateOptions<'a> {
    pub base_dir: PathBuf,
    pub name: String,
    pub package: String,
    pub template: String,
    pub catalog_path: Option<PathBuf>,
    pub template_remote: Option<String>,
    pub git_url: Option<String>,
    pub dry_run: bool,
    pub interactive: bool,
    pub no_color: bool,
    pub progress: &'a ProgressReporter,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectTemplateCatalog {
    #[serde(rename = "apiVersion")]
    api_version: String,
    kind: String,
    templates: BTreeMap<String, ProjectTemplate>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectTemplate {
    #[serde(default)]
    description: Option<String>,
    source: GitTemplateSource,
    #[serde(default)]
    subdirectory: Option<String>,
    #[serde(default)]
    overlay: Option<String>,
    #[serde(rename = "textReplacements", default)]
    text_replacements: Vec<TextReplacement>,
    #[serde(default)]
    repository: RepositoryGuidance,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TextReplacement {
    path: String,
    find: String,
    replace: String,
    #[serde(rename = "expectedMatches", default = "one_match")]
    expected_matches: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GitTemplateSource {
    #[serde(rename = "type")]
    source_type: String,
    #[serde(default)]
    url: Option<String>,
    #[serde(rename = "ref")]
    reference: String,
    #[serde(rename = "defaultRemote", default)]
    default_remote: Option<String>,
    #[serde(default)]
    remotes: BTreeMap<String, GitTemplateRemote>,
    #[serde(rename = "allowInsecureHttp", default)]
    allow_insecure_http: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GitTemplateRemote {
    #[serde(default)]
    description: Option<String>,
    url: String,
    #[serde(rename = "allowInsecureHttp", default)]
    allow_insecure_http: bool,
}

struct ResolvedGitTemplateSource {
    remote_id: Option<String>,
    url: String,
    reference: String,
}

pub struct ProjectTemplateChoice {
    pub id: String,
    pub description: Option<String>,
}

pub struct DiscoveredProjectTemplateCatalog {
    pub path: PathBuf,
    pub templates: Vec<ProjectTemplateChoice>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RepositoryGuidance {
    #[serde(rename = "defaultBranch", default = "default_branch")]
    default_branch: String,
    #[serde(rename = "remoteUrl", default)]
    remote_url: Option<String>,
}

impl Default for RepositoryGuidance {
    fn default() -> Self {
        Self {
            default_branch: default_branch(),
            remote_url: None,
        }
    }
}

fn default_branch() -> String {
    "main".to_string()
}

fn one_match() -> usize {
    1
}

pub fn create_project(options: ProjectTemplateOptions<'_>) -> RainyResult<CommandOutput> {
    validate_project_name(&options.name)?;
    let catalog_path = resolve_catalog_path(options.catalog_path.clone(), &options.base_dir)?;
    options.progress.detail(format!(
        "Loading template catalog {}",
        catalog_path.display()
    ));
    let catalog = load_catalog(&catalog_path)?;
    let template = catalog.templates.get(&options.template).ok_or_else(|| {
        RainyError::config(
            "PROJECT_TEMPLATE_NOT_FOUND",
            format!(
                "template '{}' is not declared in {}; available templates: {}",
                options.template,
                catalog_path.display(),
                catalog
                    .templates
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )
    })?;
    validate_template(&options.template, template)?;
    let source = select_template_source(
        template,
        options.template_remote.as_deref(),
        options.interactive,
        options.no_color,
        options.progress,
    )?;
    let overlay_root = resolve_overlay_root(&catalog_path, template.overlay.as_deref())?;

    let project_dir = options.base_dir.join(&options.name);
    if project_dir.exists() {
        return Err(RainyError::config(
            "PROJECT_ALREADY_EXISTS",
            format!(
                "project destination already exists: {}",
                project_dir.display()
            ),
        ));
    }
    let remote_url = options
        .git_url
        .as_deref()
        .or(template.repository.remote_url.as_deref())
        .map(|value| render_project_value(value, &options.name, &options.package))
        .transpose()?;
    if let Some(url) = &remote_url {
        validate_target_git_url(url)?;
    }
    let next_commands = git_next_commands(
        &project_dir,
        &template.repository.default_branch,
        remote_url.as_deref(),
    );

    if options.dry_run {
        return Ok(template_output(
            "dry-run",
            &options,
            template,
            &source,
            project_dir,
            None,
            false,
            Vec::new(),
            remote_url,
            next_commands,
        ));
    }

    std::fs::create_dir_all(&options.base_dir)?;
    let staging = tempfile::Builder::new()
        .prefix(".rainy-project-template-")
        .tempdir_in(&options.base_dir)?;
    let checkout = staging.path().join("checkout");
    options.progress.detail(format!(
        "Cloning template {} at {}",
        options.template, source.reference
    ));
    clone_template(&source, &checkout)?;
    let resolved_ref = resolve_git_commit(&checkout)?;
    let source_root = resolve_template_root(&checkout, template.subdirectory.as_deref())?;
    let rendered = staging.path().join("rendered");
    options
        .progress
        .detail("Validating and rendering template files");
    let mut files = render_template_tree(&source_root, &rendered, &options.name, &options.package)?;
    if let Some(overlay_root) = overlay_root {
        options.progress.detail(format!(
            "Applying enterprise overlay {}",
            overlay_root.display()
        ));
        files.extend(render_template_tree(
            &overlay_root,
            &rendered,
            &options.name,
            &options.package,
        )?);
        files.sort();
        files.dedup();
    }
    files.extend(apply_text_replacements(
        &rendered,
        &template.text_replacements,
        &options.name,
        &options.package,
    )?);
    write_project_template_lock(&rendered, &options.template, &source, &resolved_ref)?;
    files.push(PROJECT_TEMPLATE_LOCK.to_string());
    files.sort();
    files.dedup();
    validate_rendered_project(&rendered, &options.name)?;
    std::fs::rename(&rendered, &project_dir).map_err(|error| {
        RainyError::config(
            "PROJECT_TEMPLATE_INSTALL_FAILED",
            format!(
                "cannot atomically create project {}: {error}",
                project_dir.display()
            ),
        )
    })?;

    Ok(template_output(
        "created",
        &options,
        template,
        &source,
        project_dir,
        Some(resolved_ref),
        true,
        files,
        remote_url,
        next_commands,
    ))
}

pub fn handle_template_command(
    workspace: &Path,
    command: TemplateCommand,
) -> RainyResult<CommandOutput> {
    let lock = load_project_template_lock(workspace)?;
    let report = match command.command {
        TemplateSubcommand::Status => ProjectTemplateStatusReport {
            protocol_version: "rainy.project-template-status.v1",
            status: "passed",
            template: lock.template,
            remote: lock.source.remote,
            source: lock.source.url,
            requested_ref: lock.source.requested_ref,
            resolved_ref: lock.source.resolved_ref,
            latest_ref: None,
            update_available: None,
            created_at: lock.created_at,
            message: "Template provenance lock is valid; no network request was made".to_string(),
        },
        TemplateSubcommand::Check => check_project_template(lock),
    };
    Ok(CommandOutput::Template { report })
}

fn check_project_template(lock: ProjectTemplateLock) -> ProjectTemplateStatusReport {
    match resolve_remote_ref(&lock.source.url, &lock.source.requested_ref) {
        Ok(latest_ref) => {
            let update_available = latest_ref != lock.source.resolved_ref;
            ProjectTemplateStatusReport {
                protocol_version: "rainy.project-template-status.v1",
                status: if update_available {
                    "update-available"
                } else {
                    "passed"
                },
                template: lock.template,
                remote: lock.source.remote,
                source: lock.source.url,
                requested_ref: lock.source.requested_ref,
                resolved_ref: lock.source.resolved_ref,
                latest_ref: Some(latest_ref),
                update_available: Some(update_available),
                created_at: lock.created_at,
                message: if update_available {
                    "The upstream template ref changed; review its release notes and create a new project or migrate explicitly"
                        .to_string()
                } else {
                    "The upstream template ref still resolves to the recorded commit".to_string()
                },
            }
        }
        Err(error) => ProjectTemplateStatusReport {
            protocol_version: "rainy.project-template-status.v1",
            status: "warning",
            template: lock.template,
            remote: lock.source.remote,
            source: lock.source.url,
            requested_ref: lock.source.requested_ref,
            resolved_ref: lock.source.resolved_ref,
            latest_ref: None,
            update_available: None,
            created_at: lock.created_at,
            message: format!(
                "Upstream could not be checked; the project remains usable from its recorded provenance: {}",
                error.body().message
            ),
        },
    }
}

fn write_project_template_lock(
    project: &Path,
    template: &str,
    source: &ResolvedGitTemplateSource,
    resolved_ref: &str,
) -> RainyResult<()> {
    let path = project.join(PROJECT_TEMPLATE_LOCK);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let lock = ProjectTemplateLock {
        lockfile_version: 1,
        template: template.to_string(),
        source: ProjectTemplateLockSource {
            remote: source.remote_id.clone(),
            url: source.url.clone(),
            requested_ref: source.reference.clone(),
            resolved_ref: resolved_ref.to_string(),
        },
        created_at: Utc::now(),
    };
    std::fs::write(path, serde_yaml::to_string(&lock)?)?;
    Ok(())
}

fn load_project_template_lock(workspace: &Path) -> RainyResult<ProjectTemplateLock> {
    let path = workspace.join(PROJECT_TEMPLATE_LOCK);
    if !path.is_file() {
        return Err(RainyError::config(
            "PROJECT_TEMPLATE_LOCK_NOT_FOUND",
            format!(
                "{} was not found; this project was not created from a provenance-aware enterprise template",
                path.display()
            ),
        ));
    }
    let lock: ProjectTemplateLock = serde_yaml::from_str(&std::fs::read_to_string(&path)?)?;
    if lock.lockfile_version != 1
        || lock.template.trim().is_empty()
        || lock.source.url.trim().is_empty()
        || lock.source.requested_ref.trim().is_empty()
        || !is_git_commit(&lock.source.resolved_ref)
    {
        return Err(RainyError::config(
            "PROJECT_TEMPLATE_LOCK_INVALID",
            format!("project template provenance is invalid: {}", path.display()),
        ));
    }
    crate::security::validate_git_with_private_http(&lock.source.url, true, true)
        .map_err(|reason| RainyError::config("PROJECT_TEMPLATE_LOCK_INVALID", reason))?;
    Ok(lock)
}

pub fn validate_project_template_lock(workspace: &Path) -> RainyResult<Option<String>> {
    if !workspace.join(PROJECT_TEMPLATE_LOCK).is_file() {
        return Ok(None);
    }
    let lock = load_project_template_lock(workspace)?;
    Ok(Some(format!(
        "template {} provenance is locked to {}",
        lock.template, lock.source.resolved_ref
    )))
}

fn resolve_remote_ref(url: &str, reference: &str) -> RainyResult<String> {
    if is_git_commit(reference) {
        return Ok(reference.to_ascii_lowercase());
    }
    let peeled = format!("{reference}^{{}}");
    let mut command = Command::new("git");
    command
        .args(["ls-remote", "--exit-code"])
        .arg(url)
        .arg(reference)
        .arg(&peeled);
    let output = crate::process::run_command(
        command,
        "git",
        Duration::from_secs(30),
        crate::process::DEFAULT_OUTPUT_LIMIT,
    )?;
    if !output.success() {
        return Err(RainyError::action(
            "PROJECT_TEMPLATE_REMOTE_FAILED",
            output.stderr.trim(),
        ));
    }
    let mut resolved = None;
    for line in output.stdout.lines() {
        let mut fields = line.split_whitespace();
        let Some(commit) = fields.next() else {
            continue;
        };
        let remote_ref = fields.next().unwrap_or_default();
        if is_git_commit(commit) && (resolved.is_none() || remote_ref.ends_with("^{}")) {
            resolved = Some(commit.to_ascii_lowercase());
        }
    }
    resolved.ok_or_else(|| {
        RainyError::action(
            "PROJECT_TEMPLATE_REMOTE_FAILED",
            format!("Git ref '{reference}' did not resolve to a commit"),
        )
    })
}

fn is_git_commit(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.chars().all(|character| character.is_ascii_hexdigit())
}

#[allow(clippy::too_many_arguments)]
fn template_output(
    status: &'static str,
    options: &ProjectTemplateOptions<'_>,
    template: &ProjectTemplate,
    source: &ResolvedGitTemplateSource,
    project_dir: PathBuf,
    resolved_ref: Option<String>,
    source_git_removed: bool,
    files: Vec<String>,
    remote_url: Option<String>,
    next_commands: Vec<String>,
) -> CommandOutput {
    CommandOutput::ProjectTemplate {
        status,
        project: options.name.clone(),
        path: project_dir.to_string_lossy().to_string(),
        template: options.template.clone(),
        source: source.url.clone(),
        source_remote: source.remote_id.clone(),
        requested_ref: source.reference.clone(),
        resolved_ref,
        source_git_removed,
        files,
        default_branch: template.repository.default_branch.clone(),
        remote_url,
        next_commands,
    }
}

pub fn discover_template_catalog(
    base_dir: &Path,
) -> RainyResult<Option<DiscoveredProjectTemplateCatalog>> {
    let environment_configured = std::env::var_os("RAINY_TEMPLATE_CONFIG").is_some();
    let path = resolve_catalog_path(None, base_dir)?;
    if !path.is_file() {
        if environment_configured {
            load_catalog(&path)?;
        }
        return Ok(None);
    }
    inspect_template_catalog(path).map(Some)
}

pub fn inspect_template_catalog(path: PathBuf) -> RainyResult<DiscoveredProjectTemplateCatalog> {
    let catalog = load_catalog(&path)?;
    let mut templates = Vec::new();
    for (id, template) in catalog.templates {
        validate_template(&id, &template)?;
        templates.push(ProjectTemplateChoice {
            id,
            description: template.description,
        });
    }
    Ok(DiscoveredProjectTemplateCatalog { path, templates })
}

fn resolve_catalog_path(explicit: Option<PathBuf>, base_dir: &Path) -> RainyResult<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path);
    }
    if let Some(path) = std::env::var_os("RAINY_TEMPLATE_CONFIG") {
        return Ok(PathBuf::from(path));
    }
    let local = base_dir.join("project-templates.yaml");
    if local.is_file() {
        return Ok(local);
    }
    crate::paths::rainy_home()
        .map(|home| home.join("templates.yaml"))
        .map_err(|_| {
            RainyError::config(
                "PROJECT_TEMPLATE_CONFIG_NOT_FOUND",
                "cannot determine the template catalog; pass --template-config or set RAINY_TEMPLATE_CONFIG",
            )
        })
}

fn load_catalog(path: &Path) -> RainyResult<ProjectTemplateCatalog> {
    if !path.is_file() {
        return Err(RainyError::config(
            "PROJECT_TEMPLATE_CONFIG_NOT_FOUND",
            format!(
                "template catalog not found: {}; pass --template-config or set RAINY_TEMPLATE_CONFIG",
                path.display()
            ),
        ));
    }
    let catalog: ProjectTemplateCatalog = serde_yaml::from_str(&std::fs::read_to_string(path)?)?;
    if catalog.api_version != "rainy.dev/v1" || catalog.kind != "ProjectTemplateCatalog" {
        return Err(RainyError::config(
            "PROJECT_TEMPLATE_CONFIG_INVALID",
            "template catalog must use apiVersion rainy.dev/v1 and kind ProjectTemplateCatalog",
        ));
    }
    if catalog.templates.is_empty() {
        return Err(RainyError::config(
            "PROJECT_TEMPLATE_CONFIG_INVALID",
            "template catalog must declare at least one template",
        ));
    }
    Ok(catalog)
}

fn validate_template(id: &str, template: &ProjectTemplate) -> RainyResult<()> {
    if template.source.source_type != "git" {
        return Err(RainyError::config(
            "PROJECT_TEMPLATE_SOURCE_UNSUPPORTED",
            format!("template {id} source.type must be git"),
        ));
    }
    match (&template.source.url, template.source.remotes.is_empty()) {
        (Some(url), true) => {
            validate_source_git_url(url, template.source.allow_insecure_http)?;
            if template.source.default_remote.is_some() {
                return Err(RainyError::config(
                    "PROJECT_TEMPLATE_SOURCE_INVALID",
                    format!("template {id} source.defaultRemote requires source.remotes"),
                ));
            }
        }
        (None, false) => {
            for (remote_id, remote) in &template.source.remotes {
                validate_remote_id(remote_id)?;
                validate_source_git_url(&remote.url, remote.allow_insecure_http)?;
            }
            if let Some(default_remote) = &template.source.default_remote
                && !template.source.remotes.contains_key(default_remote)
            {
                return Err(RainyError::config(
                    "PROJECT_TEMPLATE_DEFAULT_REMOTE_INVALID",
                    format!(
                        "template {id} source.defaultRemote '{default_remote}' is not declared in source.remotes"
                    ),
                ));
            }
        }
        _ => {
            return Err(RainyError::config(
                "PROJECT_TEMPLATE_SOURCE_INVALID",
                format!(
                    "template {id} source must declare either url or one or more named remotes"
                ),
            ));
        }
    }
    if template.source.reference.trim().is_empty()
        || template.source.reference.starts_with('-')
        || contains_control(&template.source.reference)
    {
        return Err(RainyError::config(
            "PROJECT_TEMPLATE_REF_INVALID",
            format!("template {id} must declare a safe non-empty Git ref"),
        ));
    }
    if template.repository.default_branch.trim().is_empty()
        || template.repository.default_branch.starts_with('-')
        || template
            .repository
            .default_branch
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return Err(RainyError::config(
            "PROJECT_TEMPLATE_BRANCH_INVALID",
            format!("template {id} repository.defaultBranch is invalid"),
        ));
    }
    if let Some(subdirectory) = &template.subdirectory {
        safe_relative_path(subdirectory, "PROJECT_TEMPLATE_SUBDIRECTORY_INVALID")?;
    }
    if let Some(overlay) = &template.overlay {
        safe_relative_path(overlay, "PROJECT_TEMPLATE_OVERLAY_INVALID")?;
    }
    for replacement in &template.text_replacements {
        safe_relative_path(&replacement.path, "PROJECT_TEMPLATE_REPLACEMENT_INVALID")?;
        if replacement.find.is_empty()
            || replacement.find.len() > 64 * 1024
            || replacement.replace.len() > 64 * 1024
            || replacement.expected_matches == 0
            || replacement.expected_matches > 1_000
        {
            return Err(RainyError::config(
                "PROJECT_TEMPLATE_REPLACEMENT_INVALID",
                format!(
                    "template {id} has an invalid text replacement for {}",
                    replacement.path
                ),
            ));
        }
    }
    Ok(())
}

fn validate_remote_id(value: &str) -> RainyResult<()> {
    if value.trim().is_empty()
        || value.starts_with('-')
        || value.chars().any(|character| {
            !(character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.'))
        })
    {
        return Err(RainyError::config(
            "PROJECT_TEMPLATE_REMOTE_ID_INVALID",
            format!("template remote ID is invalid: {value}"),
        ));
    }
    Ok(())
}

fn select_template_source(
    template: &ProjectTemplate,
    requested_remote: Option<&str>,
    interactive: bool,
    no_color: bool,
    progress: &ProgressReporter,
) -> RainyResult<ResolvedGitTemplateSource> {
    if let Some(url) = &template.source.url {
        if requested_remote.is_some() {
            return Err(RainyError::config(
                "PROJECT_TEMPLATE_REMOTE_NOT_SUPPORTED",
                "--template-remote requires a template with named source.remotes",
            ));
        }
        return Ok(ResolvedGitTemplateSource {
            remote_id: None,
            url: url.clone(),
            reference: template.source.reference.clone(),
        });
    }

    let selected_id = if let Some(remote_id) = requested_remote {
        remote_id.to_string()
    } else if template.source.remotes.len() == 1 {
        template.source.remotes.keys().next().cloned().unwrap()
    } else if interactive {
        let _suspension = progress.suspend();
        let remote_ids = template.source.remotes.keys().cloned().collect::<Vec<_>>();
        let labels = remote_ids
            .iter()
            .map(|remote_id| {
                let remote = &template.source.remotes[remote_id];
                match remote.description.as_deref() {
                    Some(description) if !description.trim().is_empty() => {
                        format!("{remote_id}  {description}")
                    }
                    _ => remote_id.clone(),
                }
            })
            .collect::<Vec<_>>();
        let starting_cursor = template
            .source
            .default_remote
            .as_ref()
            .and_then(|default_remote| {
                remote_ids
                    .iter()
                    .position(|remote_id| remote_id == default_remote)
            })
            .unwrap_or(0);
        let prompt = Select::new("Select the template download method", labels.clone())
            .with_starting_cursor(starting_cursor)
            .with_help_message("Type to search; Up/Down move; Enter confirms; Esc cancels");
        let selected = if no_color {
            prompt
                .with_render_config(inquire::ui::RenderConfig::empty())
                .prompt()
        } else {
            prompt.prompt()
        }
        .map_err(template_prompt_error)?;
        let index = labels
            .iter()
            .position(|label| label == &selected)
            .unwrap_or(0);
        remote_ids[index].clone()
    } else {
        template.source.default_remote.clone().ok_or_else(|| {
            RainyError::config(
                "PROJECT_TEMPLATE_REMOTE_SELECTION_REQUIRED",
                format!(
                    "non-interactive creation requires --template-remote <REMOTE_ID>; available remotes: {}",
                    template
                        .source
                        .remotes
                        .keys()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )
        })?
    };
    let remote = template.source.remotes.get(&selected_id).ok_or_else(|| {
        RainyError::config(
            "PROJECT_TEMPLATE_REMOTE_NOT_FOUND",
            format!(
                "template remote '{}' is not declared; available remotes: {}",
                selected_id,
                template
                    .source
                    .remotes
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )
    })?;
    Ok(ResolvedGitTemplateSource {
        remote_id: Some(selected_id),
        url: remote.url.clone(),
        reference: template.source.reference.clone(),
    })
}

fn template_prompt_error(error: inquire::InquireError) -> RainyError {
    RainyError::action(
        "CANCELLED",
        match error {
            inquire::InquireError::OperationCanceled
            | inquire::InquireError::OperationInterrupted => {
                "Template download method selection cancelled".to_string()
            }
            other => format!("Template download method selection failed: {other}"),
        },
    )
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

fn contains_control(value: &str) -> bool {
    value.chars().any(char::is_control)
}

fn validate_source_git_url(url: &str, allow_insecure_http: bool) -> RainyResult<()> {
    crate::security::validate_git_with_private_http(url, true, allow_insecure_http).map_err(
        |reason| {
            RainyError::config(
                "PROJECT_TEMPLATE_SOURCE_INVALID",
                format!("template Git source is not allowed: {reason}"),
            )
        },
    )
}

fn validate_target_git_url(url: &str) -> RainyResult<()> {
    crate::security::validate_git(url, false).map_err(|reason| {
        RainyError::config(
            "PROJECT_GIT_URL_INVALID",
            format!("target Git URL is not allowed: {reason}"),
        )
    })
}

fn safe_relative_path(value: &str, code: &str) -> RainyResult<PathBuf> {
    let path = PathBuf::from(value);
    if value.trim().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return Err(RainyError::config(
            code,
            format!("path must be a safe relative path: {value}"),
        ));
    }
    Ok(path)
}

fn resolve_overlay_root(
    catalog_path: &Path,
    overlay: Option<&str>,
) -> RainyResult<Option<PathBuf>> {
    let Some(overlay) = overlay else {
        return Ok(None);
    };
    let relative = safe_relative_path(overlay, "PROJECT_TEMPLATE_OVERLAY_INVALID")?;
    let catalog_parent = catalog_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let canonical_parent = std::fs::canonicalize(catalog_parent).map_err(|error| {
        RainyError::config(
            "PROJECT_TEMPLATE_OVERLAY_INVALID",
            format!(
                "cannot resolve template catalog directory {}: {error}",
                catalog_parent.display()
            ),
        )
    })?;
    let candidate = canonical_parent.join(relative);
    let canonical_overlay = std::fs::canonicalize(&candidate).map_err(|error| {
        RainyError::config(
            "PROJECT_TEMPLATE_OVERLAY_NOT_FOUND",
            format!(
                "template overlay not found at {}: {error}",
                candidate.display()
            ),
        )
    })?;
    if !canonical_overlay.starts_with(&canonical_parent) || !canonical_overlay.is_dir() {
        return Err(RainyError::config(
            "PROJECT_TEMPLATE_OVERLAY_INVALID",
            format!(
                "template overlay must be a directory inside {}",
                canonical_parent.display()
            ),
        ));
    }
    Ok(Some(canonical_overlay))
}

fn clone_template(source: &ResolvedGitTemplateSource, target: &Path) -> RainyResult<()> {
    let mut command = Command::new("git");
    command
        .args(["clone", "--depth", "1", "--no-tags", "--branch"])
        .arg(&source.reference)
        .arg(&source.url)
        .arg(target);
    let output = crate::process::run_command(
        command,
        "git",
        Duration::from_secs(900),
        crate::process::DEFAULT_OUTPUT_LIMIT,
    )
    .map_err(|error| {
        RainyError::config(
            "PROJECT_TEMPLATE_GIT_NOT_AVAILABLE",
            format!("cannot execute git: {}", error.body().message),
        )
    })?;
    if output.success() {
        return Ok(());
    }

    if target.exists() {
        std::fs::remove_dir_all(target)?;
    }
    std::fs::create_dir_all(target)?;
    run_git(target, &["init", "--quiet"])?;
    run_git(target, &["remote", "add", "origin", &source.url])?;
    run_git(
        target,
        &[
            "fetch",
            "--depth",
            "1",
            "--no-tags",
            "origin",
            &source.reference,
        ],
    )?;
    run_git(target, &["checkout", "--quiet", "--detach", "FETCH_HEAD"])
}

fn run_git(repository: &Path, args: &[&str]) -> RainyResult<()> {
    let mut command = Command::new("git");
    command.arg("-C").arg(repository).args(args);
    let output = crate::process::run_command(
        command,
        "git",
        Duration::from_secs(900),
        crate::process::DEFAULT_OUTPUT_LIMIT,
    )
    .map_err(|error| {
        RainyError::config(
            "PROJECT_TEMPLATE_GIT_NOT_AVAILABLE",
            format!("cannot execute git: {}", error.body().message),
        )
    })?;
    if output.success() {
        Ok(())
    } else {
        let code = if args.first() == Some(&"fetch") {
            "PROJECT_TEMPLATE_GIT_FETCH_FAILED"
        } else {
            "PROJECT_TEMPLATE_GIT_FAILED"
        };
        Err(RainyError::action(code, output.stderr.trim().to_string()))
    }
}

fn resolve_git_commit(repository: &Path) -> RainyResult<String> {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(repository)
        .args(["rev-parse", "HEAD"]);
    let output = crate::process::run_command(
        command,
        "git",
        Duration::from_secs(900),
        crate::process::DEFAULT_OUTPUT_LIMIT,
    )?;
    if !output.success() {
        return Err(RainyError::config(
            "PROJECT_TEMPLATE_GIT_FAILED",
            output.stderr.trim().to_string(),
        ));
    }
    Ok(output.stdout.trim().to_string())
}

fn resolve_template_root(checkout: &Path, subdirectory: Option<&str>) -> RainyResult<PathBuf> {
    let root = match subdirectory {
        Some(value) => checkout.join(safe_relative_path(
            value,
            "PROJECT_TEMPLATE_SUBDIRECTORY_INVALID",
        )?),
        None => checkout.to_path_buf(),
    };
    if !root.is_dir() {
        return Err(RainyError::config(
            "PROJECT_TEMPLATE_SUBDIRECTORY_NOT_FOUND",
            format!("template subdirectory not found: {}", root.display()),
        ));
    }
    Ok(root)
}

fn render_template_tree(
    source: &Path,
    target: &Path,
    project_name: &str,
    package: &str,
) -> RainyResult<Vec<String>> {
    std::fs::create_dir_all(target)?;
    let variables = json!({
        "project": { "name": project_name },
        "package": { "java": package },
        "packagePath": package.replace('.', "/")
    });
    let mut handlebars = Handlebars::new();
    handlebars.set_strict_mode(true);
    let mut files = Vec::new();
    let mut rendered_paths = BTreeSet::new();
    let mut entries = 0usize;
    let mut bytes = 0u64;

    let walker = walkdir::WalkDir::new(source)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            entry
                .path()
                .strip_prefix(source)
                .map_or(true, |relative| !contains_git_component(relative))
        });
    for entry in walker {
        let entry = entry.map_err(|error| {
            RainyError::config(
                "PROJECT_TEMPLATE_TREE_INVALID",
                format!("cannot inspect template tree: {error}"),
            )
        })?;
        let relative = entry.path().strip_prefix(source).map_err(|error| {
            RainyError::config("PROJECT_TEMPLATE_TREE_INVALID", error.to_string())
        })?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        entries += 1;
        if entries > MAX_TEMPLATE_ENTRIES {
            return Err(RainyError::config(
                "PROJECT_TEMPLATE_LIMIT_EXCEEDED",
                format!("template contains more than {MAX_TEMPLATE_ENTRIES} entries"),
            ));
        }
        if entry.file_type().is_symlink() {
            return Err(RainyError::config(
                "PROJECT_TEMPLATE_UNSAFE_ENTRY",
                format!("template contains a symbolic link: {}", relative.display()),
            ));
        }
        let rendered_relative = render_relative_path(&handlebars, relative, &variables)?;
        if !rendered_paths.insert(rendered_relative.clone()) {
            return Err(RainyError::config(
                "PROJECT_TEMPLATE_RENDERED_PATH_CONFLICT",
                format!(
                    "multiple template entries render to {}",
                    rendered_relative.display()
                ),
            ));
        }
        let destination = target.join(&rendered_relative);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&destination)?;
            continue;
        }
        if !entry.file_type().is_file() {
            return Err(RainyError::config(
                "PROJECT_TEMPLATE_UNSAFE_ENTRY",
                format!(
                    "template contains an unsupported entry: {}",
                    relative.display()
                ),
            ));
        }
        let metadata = entry.metadata().map_err(|error| {
            RainyError::config(
                "PROJECT_TEMPLATE_TREE_INVALID",
                format!(
                    "cannot read template metadata for {}: {error}",
                    relative.display()
                ),
            )
        })?;
        bytes = bytes.saturating_add(metadata.len());
        if bytes > MAX_TEMPLATE_BYTES {
            return Err(RainyError::config(
                "PROJECT_TEMPLATE_LIMIT_EXCEEDED",
                format!("template expands beyond {MAX_TEMPLATE_BYTES} bytes"),
            ));
        }
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let source_bytes = std::fs::read(entry.path())?;
        if relative.to_string_lossy().ends_with(".hbs") {
            match String::from_utf8(source_bytes) {
                Ok(text) => {
                    let rendered =
                        handlebars
                            .render_template(&text, &variables)
                            .map_err(|error| {
                                RainyError::config(
                                    "PROJECT_TEMPLATE_RENDER_FAILED",
                                    format!("{}: {error}", relative.display()),
                                )
                            })?;
                    std::fs::write(&destination, rendered)?;
                }
                Err(_) => {
                    return Err(RainyError::config(
                        "PROJECT_TEMPLATE_RENDER_FAILED",
                        format!("{} is an .hbs file but is not UTF-8", relative.display()),
                    ));
                }
            }
        } else {
            std::fs::write(&destination, source_bytes)?;
        }
        std::fs::set_permissions(&destination, metadata.permissions())?;
        files.push(rendered_relative.to_string_lossy().replace('\\', "/"));
    }
    files.sort();
    Ok(files)
}

fn contains_git_component(path: &Path) -> bool {
    path.components()
        .any(|component| matches!(component, Component::Normal(value) if value == ".git"))
}

fn render_relative_path(
    handlebars: &Handlebars<'_>,
    path: &Path,
    variables: &serde_json::Value,
) -> RainyResult<PathBuf> {
    let mut raw = path.to_string_lossy().replace('\\', "/");
    if raw.ends_with(".hbs") {
        raw.truncate(raw.len() - 4);
    }
    let rendered = handlebars
        .render_template(&raw, variables)
        .map_err(|error| RainyError::config("PROJECT_TEMPLATE_RENDER_FAILED", error.to_string()))?;
    safe_relative_path(&rendered, "PROJECT_TEMPLATE_RENDERED_PATH_INVALID")
}

fn render_project_value(value: &str, name: &str, package: &str) -> RainyResult<String> {
    let mut handlebars = Handlebars::new();
    handlebars.set_strict_mode(true);
    handlebars
        .render_template(
            value,
            &json!({
                "project": { "name": name },
                "package": { "java": package },
                "packagePath": package.replace('.', "/")
            }),
        )
        .map_err(|error| RainyError::config("PROJECT_TEMPLATE_RENDER_FAILED", error.to_string()))
}

fn apply_text_replacements(
    root: &Path,
    replacements: &[TextReplacement],
    project_name: &str,
    package: &str,
) -> RainyResult<Vec<String>> {
    let mut changed = Vec::new();
    for replacement in replacements {
        let relative =
            safe_relative_path(&replacement.path, "PROJECT_TEMPLATE_REPLACEMENT_INVALID")?;
        let path = root.join(&relative);
        let metadata = std::fs::symlink_metadata(&path).map_err(|error| {
            RainyError::config(
                "PROJECT_TEMPLATE_REPLACEMENT_TARGET_INVALID",
                format!(
                    "cannot inspect text replacement target {}: {error}",
                    relative.display()
                ),
            )
        })?;
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            return Err(RainyError::config(
                "PROJECT_TEMPLATE_REPLACEMENT_TARGET_INVALID",
                format!(
                    "text replacement target must be a regular file: {}",
                    relative.display()
                ),
            ));
        }
        let content = std::fs::read_to_string(&path).map_err(|error| {
            RainyError::config(
                "PROJECT_TEMPLATE_REPLACEMENT_TARGET_INVALID",
                format!(
                    "text replacement target must be UTF-8 ({}): {error}",
                    relative.display()
                ),
            )
        })?;
        let matches = content.matches(&replacement.find).count();
        if matches != replacement.expected_matches {
            return Err(RainyError::config(
                "PROJECT_TEMPLATE_REPLACEMENT_MISMATCH",
                format!(
                    "text replacement for {} expected {} match(es), found {matches}; the upstream template may have changed",
                    relative.display(),
                    replacement.expected_matches
                ),
            ));
        }
        let rendered = render_project_value(&replacement.replace, project_name, package)?;
        std::fs::write(
            &path,
            content.replacen(&replacement.find, &rendered, replacement.expected_matches),
        )?;
        changed.push(relative.to_string_lossy().replace('\\', "/"));
    }
    Ok(changed)
}

fn validate_rendered_project(root: &Path, expected_name: &str) -> RainyResult<()> {
    for required in ["rainy.yaml", "capability.lock"] {
        if !root.join(required).is_file() {
            return Err(RainyError::config(
                "PROJECT_TEMPLATE_REQUIRED_FILE_MISSING",
                format!("rendered project is missing required file: {required}"),
            ));
        }
    }
    let project = crate::config::load_config(root)?;
    if project.project.name != expected_name {
        return Err(RainyError::config(
            "PROJECT_TEMPLATE_NAME_MISMATCH",
            format!(
                "rendered rainy.yaml project.name is '{}', expected '{expected_name}'",
                project.project.name
            ),
        ));
    }
    let lock = crate::config::load_lock(root)?;
    if lock.project.name != expected_name {
        return Err(RainyError::config(
            "PROJECT_TEMPLATE_NAME_MISMATCH",
            format!(
                "rendered capability.lock project.name is '{}', expected '{expected_name}'",
                lock.project.name
            ),
        ));
    }
    if root.join(".git").exists() {
        return Err(RainyError::config(
            "PROJECT_TEMPLATE_GIT_METADATA_PRESENT",
            "rendered project unexpectedly contains .git metadata",
        ));
    }
    Ok(())
}

fn git_next_commands(
    project_dir: &Path,
    default_branch: &str,
    remote_url: Option<&str>,
) -> Vec<String> {
    let remote = remote_url.unwrap_or("<PROJECT_GIT_URL>");
    vec![
        format!("cd {}", shell_quote(&project_dir.to_string_lossy())),
        format!("git init -b {}", shell_quote(default_branch)),
        format!("git remote add origin {}", shell_quote(remote)),
        "git add .".to_string(),
        "git commit -m 'Initial commit'".to_string(),
        format!("git push -u origin {}", shell_quote(default_branch)),
    ]
}

fn shell_quote(value: &str) -> String {
    if value.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'_' | b'-' | b':' | b'@')
    }) {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}
