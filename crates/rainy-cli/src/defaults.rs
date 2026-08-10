use crate::cli::{DefaultsChangeArgs, DefaultsCommand, DefaultsSubcommand};
use crate::error::{RainyError, RainyResult};
use crate::output::CommandOutput;
use fs2::FileExt;
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::Duration;
use walkdir::WalkDir;

const DEFAULT_SOURCE: &str = "https://github.com/RainLib/rainy-cli.git";
const MANIFEST: &str = "rainy-defaults.yaml";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DefaultsManifest {
    api_version: String,
    kind: String,
    metadata: DefaultsMetadata,
    requires: DefaultsRequires,
    paths: DefaultsPaths,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DefaultsMetadata {
    name: String,
    version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DefaultsRequires {
    rainy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DefaultsPaths {
    packs: String,
    skills: String,
    templates: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DefaultsLock {
    lockfile_version: u32,
    source: String,
    requested_ref: String,
    resolved_ref: String,
    cache_path: String,
    package_version: String,
    digest: String,
    installed_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DefaultsReport {
    pub protocol_version: String,
    pub status: String,
    pub operation: String,
    pub source: String,
    pub requested_ref: String,
    pub resolved_ref: Option<String>,
    pub package_version: Option<String>,
    pub cache_path: Option<String>,
    pub packs_path: Option<String>,
    pub skills_path: Option<String>,
}

pub fn handle_defaults_command(command: DefaultsCommand) -> RainyResult<CommandOutput> {
    let report = match command.command {
        DefaultsSubcommand::Status => status_report("status")?,
        DefaultsSubcommand::Install(args) => change("install", args, false)?,
        DefaultsSubcommand::Update(args) => change("update", args, true)?,
        DefaultsSubcommand::Doctor => doctor_report()?,
    };
    Ok(CommandOutput::Defaults { report })
}

pub fn packs_path() -> RainyResult<PathBuf> {
    let root = ensure_available()?;
    Ok(root.join(load_manifest(&root)?.paths.packs))
}

pub fn skills_path() -> RainyResult<PathBuf> {
    let root = ensure_available()?;
    Ok(root.join(load_manifest(&root)?.paths.skills))
}

pub fn template_path(name: &str) -> RainyResult<PathBuf> {
    validate_relative_path(name)?;
    let root = ensure_available()?;
    let path = root.join(load_manifest(&root)?.paths.templates).join(name);
    if !path.is_file() {
        return Err(RainyError::registry(
            "DEFAULTS_TEMPLATE_MISSING",
            format!("default template is missing: {name}"),
        ));
    }
    Ok(path)
}

pub fn implicit_fetch_required() -> bool {
    if development_source().is_some() {
        return false;
    }
    if defaults_override_is_set() {
        let source = configured_source();
        let reference = configured_ref();
        return cache_path(&source, &reference)
            .map(|path| !path.is_dir() || validate_distribution(&path).is_err())
            .unwrap_or(true);
    }
    !load_lock().is_ok_and(|lock| {
        let cache = Path::new(&lock.cache_path);
        cache.is_dir() && validate_distribution(cache).is_ok()
    })
}

fn ensure_available() -> RainyResult<PathBuf> {
    if let Some(root) = development_source() {
        validate_distribution(&root)?;
        return Ok(root);
    }
    if defaults_override_is_set() {
        let source = configured_source();
        let reference = configured_ref();
        let target = cache_path(&source, &reference)?;
        if target.is_dir() && validate_distribution(&target).is_ok() {
            return Ok(target);
        }
        if offline() {
            return Err(RainyError::registry(
                "DEFAULTS_OFFLINE_MISSING",
                "the configured default package is not cached; reconnect and run 'rainy defaults install --apply'",
            ));
        }
        return install_distribution(&source, &reference, false);
    }
    if let Ok(lock) = load_lock()
        && Path::new(&lock.cache_path).is_dir()
        && validate_distribution(Path::new(&lock.cache_path)).is_ok()
        && distribution_digest(Path::new(&lock.cache_path))
            .is_ok_and(|digest| digest == lock.digest)
    {
        return Ok(PathBuf::from(lock.cache_path));
    }
    if offline() {
        return Err(RainyError::registry(
            "DEFAULTS_OFFLINE_MISSING",
            "official defaults are not cached; reconnect and run 'rainy defaults install --apply'",
        ));
    }
    install_distribution(&configured_source(), &configured_ref(), false)
}

fn change(
    operation: &str,
    args: DefaultsChangeArgs,
    force_refresh: bool,
) -> RainyResult<DefaultsReport> {
    if args.apply && args.dry_run {
        return Err(RainyError::config(
            "APPLY_MODE_CONFLICT",
            "--dry-run and --apply cannot be used together",
        ));
    }
    let source = args.source.unwrap_or_else(configured_source);
    let reference = args.reference.unwrap_or_else(configured_ref);
    if !args.apply {
        let version = reference
            .strip_prefix('v')
            .and_then(|value| semver::Version::parse(value).ok())
            .map(|version| version.to_string());
        let mut preview = report(operation, "dry-run", source, reference, None, None);
        preview.package_version = version;
        return Ok(preview);
    }
    let root = install_distribution(&source, &reference, force_refresh)?;
    let lock = load_lock()?;
    let package_version = lock.package_version.clone();
    Ok(report(
        operation,
        "applied",
        lock.source,
        lock.requested_ref,
        Some(lock.resolved_ref),
        Some(root),
    )
    .with_package_version(package_version))
}

fn status_report(operation: &str) -> RainyResult<DefaultsReport> {
    if let Some(root) = development_source() {
        let manifest = validate_distribution(&root)?;
        return Ok(report(
            operation,
            "available",
            root.to_string_lossy().to_string(),
            "development".to_string(),
            None,
            Some(root),
        )
        .with_package_version(manifest.metadata.version));
    }
    match load_lock() {
        Ok(lock)
            if Path::new(&lock.cache_path).is_dir()
                && (!defaults_override_is_set()
                    || (lock.source == configured_source()
                        && lock.requested_ref == configured_ref())) =>
        {
            Ok(report(
                operation,
                "available",
                lock.source,
                lock.requested_ref,
                Some(lock.resolved_ref),
                Some(PathBuf::from(lock.cache_path)),
            )
            .with_package_version(lock.package_version))
        }
        _ => Ok(report(
            operation,
            "missing",
            configured_source(),
            configured_ref(),
            None,
            None,
        )),
    }
}

pub fn doctor_report() -> RainyResult<DefaultsReport> {
    let mut report = status_report("doctor")?;
    let Some(cache) = report.cache_path.as_deref() else {
        report.status = "failed".to_string();
        return Ok(report);
    };
    let manifest = validate_distribution(Path::new(cache))?;
    let digest_matches = development_source().is_some()
        || load_lock().is_ok_and(|lock| {
            lock.cache_path == cache
                && distribution_digest(Path::new(cache)).is_ok_and(|digest| digest == lock.digest)
        });
    report.status = if digest_matches { "passed" } else { "failed" }.to_string();
    report.package_version = Some(manifest.metadata.version);
    Ok(report)
}

fn install_distribution(source: &str, reference: &str, force: bool) -> RainyResult<PathBuf> {
    crate::progress::detail_current(format!(
        "Preparing default content package at immutable reference {reference}"
    ));
    if let Some(path) = local_source(source) {
        let source_root = path.canonicalize().map_err(|error| {
            RainyError::registry(
                "DEFAULTS_SOURCE_NOT_FOUND",
                format!(
                    "default package source not found: {} ({error})",
                    path.display()
                ),
            )
        })?;
        let manifest = validate_distribution(&source_root)?;
        let source_identity = source_root.to_string_lossy().to_string();
        let target = cache_path(&source_identity, reference)?;
        let _lock = cache_lock(&target)?;
        if target.is_dir() && !force {
            let cached_manifest = validate_distribution(&target)?;
            save_lock(&DefaultsLock {
                lockfile_version: 1,
                source: source_identity,
                requested_ref: reference.to_string(),
                resolved_ref: "local".to_string(),
                cache_path: target.to_string_lossy().to_string(),
                package_version: cached_manifest.metadata.version,
                digest: distribution_digest(&target)?,
                installed_at: chrono::Utc::now(),
            })?;
            return Ok(target);
        }
        let staging = target.with_extension(format!("tmp.{}", std::process::id()));
        if staging.exists() {
            std::fs::remove_dir_all(&staging)?;
        }
        snapshot_local_distribution(&source_root, &staging, &manifest)?;
        reject_unsafe_entries(&staging)?;
        let installed_manifest = validate_distribution(&staging)?;
        replace_directory(&staging, &target)?;
        save_lock(&DefaultsLock {
            lockfile_version: 1,
            source: source_identity,
            requested_ref: reference.to_string(),
            resolved_ref: "local".to_string(),
            cache_path: target.to_string_lossy().to_string(),
            package_version: installed_manifest.metadata.version,
            digest: distribution_digest(&target)?,
            installed_at: chrono::Utc::now(),
        })?;
        return Ok(target);
    }
    validate_git_source(source)?;
    crate::progress::detail_current("Downloading the default content package");
    let target = cache_path(source, reference)?;
    let _lock = cache_lock(&target)?;
    if target.is_dir() && !force {
        let manifest = validate_distribution(&target)?;
        let resolved_ref = git_output(&target, &["rev-parse", "HEAD"])?;
        save_lock(&DefaultsLock {
            lockfile_version: 1,
            source: source.to_string(),
            requested_ref: reference.to_string(),
            resolved_ref,
            cache_path: target.to_string_lossy().to_string(),
            package_version: manifest.metadata.version,
            digest: distribution_digest(&target)?,
            installed_at: chrono::Utc::now(),
        })?;
        return Ok(target);
    }
    let staging = target.with_extension(format!("tmp.{}", std::process::id()));
    if staging.exists() {
        std::fs::remove_dir_all(&staging)?;
    }
    if let Some(parent) = staging.parent() {
        std::fs::create_dir_all(parent)?;
    }
    clone_ref(source, reference, &staging)?;
    reject_unsafe_entries(&staging)?;
    let manifest = validate_distribution(&staging)?;
    let resolved_ref = git_output(&staging, &["rev-parse", "HEAD"])?;
    replace_directory(&staging, &target)?;
    save_lock(&DefaultsLock {
        lockfile_version: 1,
        source: source.to_string(),
        requested_ref: reference.to_string(),
        resolved_ref,
        cache_path: target.to_string_lossy().to_string(),
        package_version: manifest.metadata.version,
        digest: distribution_digest(&target)?,
        installed_at: chrono::Utc::now(),
    })?;
    Ok(target)
}

fn clone_ref(source: &str, reference: &str, target: &Path) -> RainyResult<()> {
    let mut command = std::process::Command::new("git");
    command
        .args(["clone", "--depth", "1", "--no-tags", "--branch", reference])
        .arg(source)
        .arg(target);
    let output = crate::process::run_command(
        command,
        "git",
        Duration::from_secs(900),
        crate::process::DEFAULT_OUTPUT_LIMIT,
    )?;
    if output.success() {
        return Ok(());
    }
    if target.exists() {
        std::fs::remove_dir_all(target)?;
    }
    std::fs::create_dir_all(target)?;
    run_git(target, &["init", "--quiet"])?;
    run_git(target, &["remote", "add", "origin", source])?;
    run_git(
        target,
        &["fetch", "--depth", "1", "--no-tags", "origin", reference],
    )?;
    run_git(target, &["checkout", "--quiet", "--detach", "FETCH_HEAD"])
}

fn run_git(root: &Path, args: &[&str]) -> RainyResult<()> {
    let mut command = std::process::Command::new("git");
    command.arg("-C").arg(root).args(args);
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
            "DEFAULTS_GIT_FETCH_FAILED",
            output.stderr.trim().to_string(),
        ))
    }
}

fn git_output(root: &Path, args: &[&str]) -> RainyResult<String> {
    let mut command = std::process::Command::new("git");
    command.arg("-C").arg(root).args(args);
    let output = crate::process::run_command(
        command,
        "git",
        Duration::from_secs(900),
        crate::process::DEFAULT_OUTPUT_LIMIT,
    )?;
    if !output.success() {
        return Err(RainyError::registry(
            "DEFAULTS_GIT_REF_INVALID",
            output.stderr.trim().to_string(),
        ));
    }
    Ok(output.stdout.trim().to_string())
}

fn validate_distribution(root: &Path) -> RainyResult<DefaultsManifest> {
    let manifest = load_manifest(root)?;
    if manifest.api_version != "rainy.dev/v1" || manifest.kind != "RainyDefaults" {
        return Err(RainyError::registry(
            "DEFAULTS_MANIFEST_INVALID",
            "rainy-defaults.yaml must use rainy.dev/v1 and kind RainyDefaults",
        ));
    }
    let requirement = VersionReq::parse(&manifest.requires.rainy).map_err(|error| {
        RainyError::registry(
            "DEFAULTS_COMPATIBILITY_INVALID",
            format!("invalid Rainy version requirement: {error}"),
        )
    })?;
    let current = Version::parse(env!("CARGO_PKG_VERSION")).map_err(|error| {
        RainyError::registry(
            "DEFAULTS_COMPATIBILITY_INVALID",
            format!("invalid Rainy CLI version: {error}"),
        )
    })?;
    Version::parse(&manifest.metadata.version).map_err(|error| {
        RainyError::registry(
            "DEFAULTS_MANIFEST_INVALID",
            format!("invalid default package version: {error}"),
        )
    })?;
    if manifest.metadata.name.trim().is_empty() {
        return Err(RainyError::registry(
            "DEFAULTS_MANIFEST_INVALID",
            "default package name cannot be empty",
        ));
    }
    if !requirement.matches(&current) {
        return Err(RainyError::registry(
            "DEFAULTS_INCOMPATIBLE",
            format!(
                "default package {} requires Rainy {}, current version is {}",
                manifest.metadata.version, manifest.requires.rainy, current
            ),
        ));
    }
    for (name, relative) in [
        ("packs", manifest.paths.packs.as_str()),
        ("skills", manifest.paths.skills.as_str()),
        ("templates", manifest.paths.templates.as_str()),
    ] {
        validate_relative_path(relative)?;
        if !root.join(relative).is_dir() {
            return Err(RainyError::registry(
                "DEFAULTS_CONTENT_MISSING",
                format!("default package {name} directory is missing: {relative}"),
            ));
        }
    }
    Ok(manifest)
}

fn load_manifest(root: &Path) -> RainyResult<DefaultsManifest> {
    let path = root.join(MANIFEST);
    let content = std::fs::read_to_string(&path).map_err(|error| {
        RainyError::registry(
            "DEFAULTS_MANIFEST_NOT_FOUND",
            format!("cannot read {}: {error}", path.display()),
        )
    })?;
    Ok(serde_yaml::from_str(&content)?)
}

fn validate_relative_path(value: &str) -> RainyResult<()> {
    let path = Path::new(value);
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
            "DEFAULTS_PATH_UNSAFE",
            format!("default package path is unsafe: {value}"),
        ));
    }
    Ok(())
}

fn reject_unsafe_entries(root: &Path) -> RainyResult<()> {
    for entry in walkdir::WalkDir::new(root).follow_links(false) {
        let entry = entry.map_err(|error| {
            RainyError::registry(
                "DEFAULTS_SOURCE_INVALID",
                format!("cannot inspect default package: {error}"),
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
                "DEFAULTS_SOURCE_UNSAFE_ENTRY",
                format!("symbolic links are not allowed: {}", entry.path().display()),
            ));
        }
    }
    Ok(())
}

fn snapshot_local_distribution(
    source: &Path,
    target: &Path,
    manifest: &DefaultsManifest,
) -> RainyResult<()> {
    std::fs::create_dir_all(target)?;
    copy_regular_file(&source.join(MANIFEST), &target.join(MANIFEST))?;
    for relative in [
        manifest.paths.packs.as_str(),
        manifest.paths.skills.as_str(),
        manifest.paths.templates.as_str(),
    ] {
        copy_directory_secure(&source.join(relative), &target.join(relative))?;
    }
    Ok(())
}

fn copy_regular_file(source: &Path, target: &Path) -> RainyResult<()> {
    let metadata = std::fs::symlink_metadata(source)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(RainyError::registry(
            "DEFAULTS_SOURCE_UNSAFE_ENTRY",
            format!(
                "default package file must be a regular file: {}",
                source.display()
            ),
        ));
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(source, target)?;
    Ok(())
}

fn copy_directory_secure(source: &Path, target: &Path) -> RainyResult<()> {
    for entry in WalkDir::new(source).follow_links(false) {
        let entry = entry.map_err(|error| {
            RainyError::registry(
                "DEFAULTS_SOURCE_INVALID",
                format!("cannot copy default package content: {error}"),
            )
        })?;
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|error| RainyError::registry("DEFAULTS_SOURCE_INVALID", error.to_string()))?;
        let destination = target.join(relative);
        if entry.file_type().is_symlink() {
            return Err(RainyError::registry(
                "DEFAULTS_SOURCE_UNSAFE_ENTRY",
                format!("symbolic links are not allowed: {}", entry.path().display()),
            ));
        }
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(destination)?;
        } else if entry.file_type().is_file() {
            copy_regular_file(entry.path(), &destination)?;
        } else {
            return Err(RainyError::registry(
                "DEFAULTS_SOURCE_UNSAFE_ENTRY",
                format!("unsupported filesystem entry: {}", entry.path().display()),
            ));
        }
    }
    Ok(())
}

fn report(
    operation: &str,
    status: &str,
    source: String,
    requested_ref: String,
    resolved_ref: Option<String>,
    root: Option<PathBuf>,
) -> DefaultsReport {
    let (cache_path, packs_path, skills_path) = root.map_or((None, None, None), |root| {
        let manifest = load_manifest(&root).ok();
        (
            Some(root.to_string_lossy().to_string()),
            manifest.as_ref().map(|manifest| {
                root.join(&manifest.paths.packs)
                    .to_string_lossy()
                    .to_string()
            }),
            manifest.as_ref().map(|manifest| {
                root.join(&manifest.paths.skills)
                    .to_string_lossy()
                    .to_string()
            }),
        )
    });
    DefaultsReport {
        protocol_version: "rainy.defaults-report.v1".to_string(),
        status: status.to_string(),
        operation: operation.to_string(),
        source,
        requested_ref,
        resolved_ref,
        package_version: None,
        cache_path,
        packs_path,
        skills_path,
    }
}

impl DefaultsReport {
    fn with_package_version(mut self, version: String) -> Self {
        self.package_version = Some(version);
        self
    }
}

fn configured_source() -> String {
    std::env::var("RAINY_DEFAULTS_SOURCE")
        .ok()
        .or_else(|| load_lock().ok().map(|lock| lock.source))
        .unwrap_or_else(|| DEFAULT_SOURCE.to_string())
}

fn configured_ref() -> String {
    std::env::var("RAINY_DEFAULTS_REF")
        .ok()
        .or_else(|| load_lock().ok().map(|lock| lock.requested_ref))
        .unwrap_or_else(|| format!("v{}", env!("CARGO_PKG_VERSION")))
}

fn local_source(source: &str) -> Option<PathBuf> {
    if source.contains("://") || source.starts_with("git@") {
        return None;
    }
    let path = PathBuf::from(source);
    path.exists().then_some(path)
}

fn development_source() -> Option<PathBuf> {
    if std::env::var_os("RAINY_FORCE_REMOTE_DEFAULTS").is_some() {
        return None;
    }
    if let Some(source) = std::env::var_os("RAINY_DEFAULTS_SOURCE") {
        return local_source(&source.to_string_lossy());
    }
    if !cfg!(debug_assertions) && std::env::var_os("RAINY_DEVELOPMENT_DEFAULTS").is_none() {
        return None;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)?
        .to_path_buf();
    root.join(MANIFEST).is_file().then_some(root)
}

fn defaults_override_is_set() -> bool {
    std::env::var_os("RAINY_DEFAULTS_SOURCE").is_some()
        || std::env::var_os("RAINY_DEFAULTS_REF").is_some()
}

fn defaults_home() -> RainyResult<PathBuf> {
    crate::paths::rainy_home()
}

fn cache_path(source: &str, reference: &str) -> RainyResult<PathBuf> {
    let digest = hex(&Sha256::digest(format!("{source}\0{reference}").as_bytes()));
    Ok(defaults_home()?
        .join("defaults/rainy-official")
        .join(&digest[..12]))
}

fn lock_path() -> RainyResult<PathBuf> {
    Ok(defaults_home()?.join("defaults.lock"))
}

fn load_lock() -> RainyResult<DefaultsLock> {
    let lock: DefaultsLock = serde_yaml::from_str(&std::fs::read_to_string(lock_path()?)?)?;
    if lock.lockfile_version != 1 {
        return Err(RainyError::config(
            "DEFAULTS_LOCK_VERSION_UNSUPPORTED",
            format!(
                "unsupported defaults.lock lockfileVersion: {}",
                lock.lockfile_version
            ),
        ));
    }
    validate_digest(&lock.digest)?;
    Ok(lock)
}

fn distribution_digest(root: &Path) -> RainyResult<String> {
    let mut files = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| entry.file_name() != ".git")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .collect::<Vec<_>>();
    files.sort();
    let mut digest = Sha256::new();
    for path in files {
        let relative = path.strip_prefix(root).unwrap_or(&path);
        digest.update(relative.to_string_lossy().replace('\\', "/").as_bytes());
        digest.update([0]);
        digest.update(std::fs::read(path)?);
        digest.update([0]);
    }
    Ok(hex(&digest.finalize()))
}

fn validate_digest(digest: &str) -> RainyResult<()> {
    if digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Ok(());
    }
    Err(RainyError::config(
        "DEFAULTS_LOCK_DIGEST_INVALID",
        "defaults.lock digest must be a 64-character SHA-256 value",
    ))
}

fn save_lock(lock: &DefaultsLock) -> RainyResult<()> {
    let path = lock_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension(format!("tmp.{}", std::process::id()));
    std::fs::write(&temporary, serde_yaml::to_string(lock)?)?;
    let backup = path.with_extension(format!("backup.{}", std::process::id()));
    if backup.exists() {
        std::fs::remove_file(&backup)?;
    }
    if path.exists() {
        std::fs::rename(&path, &backup)?;
    }
    if let Err(error) = std::fs::rename(&temporary, &path) {
        if backup.exists() {
            let _ = std::fs::rename(&backup, &path);
        }
        return Err(error.into());
    }
    if backup.exists() {
        std::fs::remove_file(backup)?;
    }
    Ok(())
}

struct CacheLock(File);

impl Drop for CacheLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.0);
    }
}

fn cache_lock(target: &Path) -> RainyResult<CacheLock> {
    let parent = target.parent().ok_or_else(|| {
        RainyError::registry("DEFAULTS_CACHE_INVALID", "default cache has no parent")
    })?;
    std::fs::create_dir_all(parent)?;
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(target.with_extension("lock"))?;
    file.lock_exclusive()?;
    Ok(CacheLock(file))
}

fn replace_directory(staging: &Path, target: &Path) -> RainyResult<()> {
    let backup = target.with_extension(format!("backup.{}", std::process::id()));
    if backup.exists() {
        std::fs::remove_dir_all(&backup)?;
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

fn validate_git_source(source: &str) -> RainyResult<()> {
    crate::security::validate_git(source, true).map_err(|reason| {
        RainyError::registry(
            "DEFAULTS_SOURCE_UNSUPPORTED",
            format!("default package source is not allowed: {reason}"),
        )
    })
}

fn offline() -> bool {
    std::env::var("RAINY_OFFLINE")
        .is_ok_and(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
