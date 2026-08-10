use crate::error::{RainyError, RainyResult};
use crate::output::CommandOutput;
use crate::progress::ProgressReporter;
use handlebars::Handlebars;
use serde::Deserialize;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const MAX_TEMPLATE_ENTRIES: usize = 10_000;
const MAX_TEMPLATE_BYTES: u64 = 512 * 1024 * 1024;

pub struct ProjectTemplateOptions<'a> {
    pub base_dir: PathBuf,
    pub name: String,
    pub package: String,
    pub template: String,
    pub catalog_path: Option<PathBuf>,
    pub git_url: Option<String>,
    pub dry_run: bool,
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
    #[serde(rename = "description", default)]
    _description: Option<String>,
    source: GitTemplateSource,
    #[serde(default)]
    subdirectory: Option<String>,
    #[serde(default)]
    repository: RepositoryGuidance,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GitTemplateSource {
    #[serde(rename = "type")]
    source_type: String,
    url: String,
    #[serde(rename = "ref")]
    reference: String,
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

pub fn create_project(options: ProjectTemplateOptions<'_>) -> RainyResult<CommandOutput> {
    validate_project_name(&options.name)?;
    let catalog_path = resolve_catalog_path(options.catalog_path.clone())?;
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
        options.template, template.source.reference
    ));
    clone_template(&template.source, &checkout)?;
    let resolved_ref = resolve_git_commit(&checkout)?;
    let source_root = resolve_template_root(&checkout, template.subdirectory.as_deref())?;
    let rendered = staging.path().join("rendered");
    options
        .progress
        .detail("Validating and rendering template files");
    let files = render_template_tree(&source_root, &rendered, &options.name, &options.package)?;
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
        project_dir,
        Some(resolved_ref),
        true,
        files,
        remote_url,
        next_commands,
    ))
}

#[allow(clippy::too_many_arguments)]
fn template_output(
    status: &'static str,
    options: &ProjectTemplateOptions<'_>,
    template: &ProjectTemplate,
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
        source: template.source.url.clone(),
        requested_ref: template.source.reference.clone(),
        resolved_ref,
        source_git_removed,
        files,
        default_branch: template.repository.default_branch.clone(),
        remote_url,
        next_commands,
    }
}

fn resolve_catalog_path(explicit: Option<PathBuf>) -> RainyResult<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path);
    }
    if let Some(path) = std::env::var_os("RAINY_TEMPLATE_CONFIG") {
        return Ok(PathBuf::from(path));
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
    validate_source_git_url(&template.source.url)?;
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
    Ok(())
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

fn validate_source_git_url(url: &str) -> RainyResult<()> {
    crate::security::validate_git(url, true).map_err(|reason| {
        RainyError::config(
            "PROJECT_TEMPLATE_SOURCE_INVALID",
            format!("template Git source is not allowed: {reason}"),
        )
    })
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

fn clone_template(source: &GitTemplateSource, target: &Path) -> RainyResult<()> {
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
        Err(RainyError::config(
            "PROJECT_TEMPLATE_GIT_FAILED",
            output.stderr.trim().to_string(),
        ))
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
