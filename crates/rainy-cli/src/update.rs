use crate::cli::{SelfCommand, SelfSubcommand};
use crate::error::{RainyError, RainyResult};
use crate::output::CommandOutput;
use chrono::{DateTime, Duration, Utc};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::PathBuf;
use std::time::Duration as StdDuration;

const DEFAULT_CHECK_INTERVAL_HOURS: i64 = 24;
const FAILURE_RETRY_HOURS: i64 = 1;
const MAX_UPDATE_RESPONSE_BYTES: u64 = 1024 * 1024;
const UPDATE_PROTOCOL: &str = "rainy.update.v1";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateReport {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    pub repository: String,
    pub current_version: String,
    pub latest_version: Option<String>,
    pub update_available: bool,
    pub skipped: bool,
    pub install_command: String,
    pub checked_at: DateTime<Utc>,
    pub next_check_after: DateTime<Utc>,
    pub release_type: String,
    pub target_asset: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateState {
    repository: Option<String>,
    last_checked: Option<DateTime<Utc>>,
    latest_version: Option<String>,
    skip_version: Option<String>,
    skip_repository: Option<String>,
    #[serde(default)]
    consecutive_failures: u32,
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    draft: bool,
}

pub fn handle_self_command(command: SelfCommand) -> RainyResult<CommandOutput> {
    match command.command {
        SelfSubcommand::Check(args) => check_command(args.repo),
        SelfSubcommand::Update(args) => update_command(args.force, args.version, args.repo),
        SelfSubcommand::Skip(args) => skip_command(args.version, args.repo),
    }
}

pub fn maybe_notify(json: bool, quiet: bool, is_self_command: bool) {
    if json || quiet || is_self_command || !auto_check_enabled() {
        return;
    }
    let Ok(mut state) = load_state() else {
        return;
    };
    if !should_check(&state) {
        return;
    }
    let Ok(report) = check_latest_with_state(&mut state, None) else {
        record_check_failure(&mut state);
        let _ = save_state(&state);
        return;
    };
    let _ = save_state(&state);
    if report.update_available
        && !report.skipped
        && let Some(latest) = &report.latest_version
    {
        eprintln!(
            "Rainy CLI update available: {} -> {latest}. Run `rainy self update` to update, or `rainy self skip {latest}` to skip this version.",
            report.current_version
        );
    }
}

fn check_command(repo: Option<String>) -> RainyResult<CommandOutput> {
    let mut state = load_state().unwrap_or_default();
    match check_latest_with_state(&mut state, repo.as_deref()) {
        Ok(report) => {
            save_state(&state)?;
            Ok(CommandOutput::Update { report })
        }
        Err(error) => {
            record_check_failure(&mut state);
            save_state(&state)?;
            Err(error)
        }
    }
}

fn update_command(
    force: bool,
    version: Option<String>,
    repo: Option<String>,
) -> RainyResult<CommandOutput> {
    let mut state = load_state().unwrap_or_default();
    let target_version = match version {
        Some(version) => parse_version(&version)?.to_string(),
        None => {
            let report = check_latest_with_state(&mut state, repo.as_deref())?;
            let target = report.latest_version.clone().ok_or_else(|| {
                RainyError::config("UPDATE_VERSION_INVALID", "latest release has no version")
            })?;
            if !report.update_available && !force {
                save_state(&state)?;
                return Ok(CommandOutput::Update { report });
            }
            target
        }
    };

    run_install_script(repo.as_deref(), &target_version)?;
    verify_installed_version(&target_version)?;
    let mut state = load_state().unwrap_or_default();
    state.skip_version = None;
    state.skip_repository = None;
    state.last_checked = Some(Utc::now());
    save_state(&state)?;
    Ok(CommandOutput::message(format!(
        "Rainy CLI {target_version} installed and verified."
    )))
}

fn skip_command(version: Option<String>, repo: Option<String>) -> RainyResult<CommandOutput> {
    let mut state = load_state().unwrap_or_default();
    let repository = repository_slug(repo.as_deref())?;
    let version = match version {
        Some(version) => parse_version(&version)?.to_string(),
        None => {
            let report = check_latest_with_state(&mut state, Some(&repository))?;
            report.latest_version.unwrap_or_else(current_version)
        }
    };
    state.skip_version = Some(version.clone());
    state.skip_repository = Some(repository);
    save_state(&state)?;
    Ok(CommandOutput::message(format!(
        "Skipped Rainy update version {version}."
    )))
}

fn check_latest_with_state(
    state: &mut UpdateState,
    repo: Option<&str>,
) -> RainyResult<UpdateReport> {
    let repository = repository_slug(repo)?;
    let latest = latest_release_version(&repository)?;
    state.repository = Some(repository.clone());
    state.last_checked = Some(Utc::now());
    state.latest_version = Some(latest.clone());
    state.consecutive_failures = 0;
    let current = current_version();
    let update_available = version_gt(&latest, &current)?;
    let skipped = is_skipped(state, &repository, &latest);
    let checked_at = state.last_checked.unwrap_or_else(Utc::now);
    Ok(UpdateReport {
        protocol_version: UPDATE_PROTOCOL.to_string(),
        repository: repository.clone(),
        current_version: current,
        latest_version: Some(latest),
        update_available,
        skipped,
        install_command: install_command(&repository),
        checked_at,
        next_check_after: checked_at + Duration::hours(check_interval_hours()),
        release_type: "stable".to_string(),
        target_asset: target_asset(),
    })
}

fn latest_release_version(repository: &str) -> RainyResult<String> {
    if let Some(url) = configured_latest_version_url()? {
        let body = http_get_limited(&url, "UPDATE_CHECK_FAILED")?;
        return parse_latest_marker(&body);
    }
    let url = format!("https://api.github.com/repos/{repository}/releases/latest");
    let body = http_get_limited(&url, "UPDATE_CHECK_FAILED")?;
    parse_latest_release_version(body.as_bytes())
}

fn parse_latest_marker(marker: &str) -> RainyResult<String> {
    Ok(parse_version(marker)?.to_string())
}

fn parse_latest_release_version(release_json: &[u8]) -> RainyResult<String> {
    let release: GitHubRelease = serde_json::from_slice(release_json)?;
    if release.draft || release.prerelease {
        return Err(RainyError::config(
            "UPDATE_CHECK_FAILED",
            "latest GitHub release is a draft or prerelease",
        ));
    }
    Ok(parse_version(&release.tag_name)?.to_string())
}

fn run_install_script(repo: Option<&str>, version: &str) -> RainyResult<()> {
    let repository = repository_slug(repo)?;
    let version = parse_version(version)?.to_string();
    let install_url = configured_version_url(&repository, &version)?;
    let script_name = if cfg!(windows) {
        "install.ps1"
    } else {
        "install.sh"
    };
    let script_path = std::env::temp_dir().join(format!(
        "rainy-install-{}-{script_name}",
        std::process::id()
    ));
    let script = http_get_limited(
        &format!("{install_url}/{script_name}"),
        "UPDATE_INSTALL_FAILED",
    )?;
    let checksums = http_get_limited(
        &format!("{install_url}/installers.sha256"),
        "UPDATE_INSTALL_CHECKSUM_MISSING",
    )?;
    verify_installer_checksum(script_name, script.as_bytes(), &checksums)?;
    std::fs::write(&script_path, script)?;
    let install_dir = std::env::var_os("INSTALL_DIR")
        .map(PathBuf::from)
        .or_else(|| config_home().map(|home| home.join("bin")))
        .ok_or_else(|| {
            RainyError::config("UPDATE_INSTALL_FAILED", "cannot resolve install directory")
        })?;
    let status = if cfg!(windows) {
        let mut command = std::process::Command::new("powershell");
        command
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(&script_path)
            .env("RAINY_REPO", &repository)
            .env("RAINY_VERSION", &version)
            .env("INSTALL_DIR", &install_dir);
        command.status()
    } else {
        let mut shell = std::process::Command::new("sh");
        shell
            .arg(&script_path)
            .env("RAINY_REPO", &repository)
            .env("RAINY_VERSION", &version)
            .env("INSTALL_DIR", &install_dir);
        shell.status()
    };
    let _ = std::fs::remove_file(&script_path);
    let status = status?;
    if !status.success() {
        return Err(RainyError::config(
            "UPDATE_INSTALL_FAILED",
            format!("installer exited with status {status}"),
        ));
    }
    Ok(())
}

fn auto_check_enabled() -> bool {
    if cfg!(debug_assertions) || std::env::var_os("CI").is_some() {
        return false;
    }
    !env_truthy("RAINY_NO_UPDATE_CHECK") && !env_truthy("RAINY_SKIP_UPDATE_CHECK")
}

fn should_check(state: &UpdateState) -> bool {
    let interval = if state.consecutive_failures > 0 {
        failure_retry_hours(state.consecutive_failures)
    } else {
        check_interval_hours()
    };
    if interval <= 0 {
        return true;
    }
    state
        .last_checked
        .is_none_or(|checked| Utc::now() - checked >= Duration::hours(interval))
}

fn is_skipped(state: &UpdateState, repository: &str, latest: &str) -> bool {
    let Some(skip_version) = state.skip_version.as_deref() else {
        return false;
    };
    if parse_version(skip_version).ok() != parse_version(latest).ok() {
        return false;
    }
    state
        .skip_repository
        .as_deref()
        .is_none_or(|skip_repository| skip_repository == repository)
}

fn env_truthy(key: &str) -> bool {
    std::env::var(key)
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

fn repository_slug(repo: Option<&str>) -> RainyResult<String> {
    if let Some(repository) = repo
        && !repository.trim().is_empty()
    {
        return normalize_repository_slug(repository);
    }
    if let Ok(repository) = std::env::var("RAINY_UPDATE_REPO")
        && !repository.trim().is_empty()
    {
        return normalize_repository_slug(&repository);
    }
    normalize_repository_slug(env!("CARGO_PKG_REPOSITORY"))
}

fn normalize_repository_slug(repository: &str) -> RainyResult<String> {
    let repository = repository.trim().trim_end_matches('/').to_string();
    let slug = repository
        .strip_prefix("https://github.com/")
        .or_else(|| repository.strip_prefix("git@github.com:"))
        .map(|slug| slug.trim_end_matches(".git").to_string())
        .unwrap_or(repository);
    if slug.split('/').count() == 2 && slug.chars().all(valid_repository_char) {
        Ok(slug)
    } else {
        Err(RainyError::config(
            "UPDATE_REPOSITORY_INVALID",
            "set --repo or RAINY_UPDATE_REPO to owner/repo",
        ))
    }
}

fn valid_repository_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '/')
}

fn install_command(repository: &str) -> String {
    if let Ok(Some(base_url)) = configured_release_base_url() {
        if cfg!(windows) {
            let escaped = base_url.replace('\'', "''");
            return format!(
                "$env:RAINY_RELEASE_BASE_URL = '{escaped}'; irm \"$env:RAINY_RELEASE_BASE_URL/install.ps1\" | iex"
            );
        }
        let escaped = shell_single_quote(&base_url);
        return format!(
            "curl -fsSL '{escaped}/install.sh' | env RAINY_RELEASE_BASE_URL='{escaped}' sh"
        );
    }
    if cfg!(windows) {
        format!("irm https://github.com/{repository}/releases/latest/download/install.ps1 | iex")
    } else {
        format!(
            "curl -fsSL https://github.com/{repository}/releases/latest/download/install.sh | sh"
        )
    }
}

fn configured_latest_version_url() -> RainyResult<Option<String>> {
    if let Some(url) = non_empty_env("RAINY_LATEST_VERSION_URL") {
        validate_update_url(&url, "UPDATE_CHECK_FAILED")?;
        return Ok(Some(url));
    }
    Ok(configured_release_base_url()?.map(|base| format!("{base}/latest.txt")))
}

fn configured_release_base_url() -> RainyResult<Option<String>> {
    let url = if let Some(url) = non_empty_env("RAINY_RELEASE_BASE_URL") {
        url
    } else if let Some(path) = config_home().map(|home| home.join("release-source"))
        && path.exists()
    {
        std::fs::read_to_string(path)?.trim().to_string()
    } else {
        return Ok(None);
    };
    if url.is_empty() {
        return Ok(None);
    }
    let url = url.trim_end_matches('/').to_string();
    validate_update_url(&url, "UPDATE_REPOSITORY_INVALID")?;
    Ok(Some(url))
}

fn configured_version_url(repository: &str, version: &str) -> RainyResult<String> {
    let url = if let Some(url) = non_empty_env("RAINY_INSTALLER_BASE_URL") {
        url.trim_end_matches('/').to_string()
    } else if let Some(base) = configured_release_base_url()? {
        format!("{base}/v{version}")
    } else {
        format!("https://github.com/{repository}/releases/download/v{version}")
    };
    validate_update_url(&url, "UPDATE_INSTALL_FAILED")?;
    Ok(url)
}

fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn validate_update_url(url: &str, error_code: &'static str) -> RainyResult<()> {
    if url.starts_with("https://") || is_loopback_http_url(url) {
        return Ok(());
    }
    Err(RainyError::config(
        error_code,
        format!("only HTTPS or loopback HTTP update URLs are allowed: {url}"),
    ))
}

fn is_loopback_http_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("http://") else {
        return false;
    };
    let authority = rest.split('/').next().unwrap_or_default();
    let host = authority
        .strip_prefix('[')
        .and_then(|value| value.split(']').next())
        .unwrap_or_else(|| authority.split(':').next().unwrap_or_default());
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}

fn shell_single_quote(value: &str) -> String {
    value.replace('\'', "'\\''")
}

fn current_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn version_gt(left: &str, right: &str) -> RainyResult<bool> {
    Ok(parse_version(left)? > parse_version(right)?)
}

fn parse_version(version: &str) -> RainyResult<Version> {
    let parsed = Version::parse(version.trim().trim_start_matches('v')).map_err(|err| {
        RainyError::config(
            "UPDATE_VERSION_INVALID",
            format!("invalid semantic version {version}: {err}"),
        )
    })?;
    if !parsed.pre.is_empty() {
        return Err(RainyError::config(
            "UPDATE_PRERELEASE_UNSUPPORTED",
            format!("prerelease updates are not supported: {version}"),
        ));
    }
    Ok(parsed)
}

fn verify_installer_checksum(name: &str, bytes: &[u8], manifest: &str) -> RainyResult<()> {
    let expected = manifest
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let digest = fields.next()?;
            let file = fields.next()?.trim_start_matches('*');
            (file == name).then_some(digest)
        })
        .next()
        .ok_or_else(|| {
            RainyError::config(
                "UPDATE_INSTALL_CHECKSUM_MISSING",
                format!("installer checksum is missing for {name}"),
            )
        })?;
    if expected.len() != 64 || !expected.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(RainyError::config(
            "UPDATE_INSTALL_CHECKSUM_INVALID",
            format!("installer checksum has an invalid format for {name}"),
        ));
    }
    let actual = format!("{:x}", Sha256::digest(bytes));
    if !actual.eq_ignore_ascii_case(expected) {
        return Err(RainyError::config(
            "UPDATE_INSTALL_CHECKSUM_INVALID",
            format!("installer checksum mismatch for {name}"),
        ));
    }
    Ok(())
}

fn state_path() -> Option<PathBuf> {
    config_home().map(|home| home.join("update-check.json"))
}

fn config_home() -> Option<PathBuf> {
    std::env::var_os("RAINY_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".rainy"))
        })
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .map(PathBuf::from)
                .map(|home| home.join(".rainy"))
        })
}

fn load_state() -> RainyResult<UpdateState> {
    let Some(path) = state_path() else {
        return Ok(UpdateState::default());
    };
    if !path.exists() {
        return Ok(UpdateState::default());
    }
    Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
}

fn save_state(state: &UpdateState) -> RainyResult<()> {
    let Some(path) = state_path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension(format!("tmp.{}", std::process::id()));
    std::fs::write(
        &temporary,
        format!("{}\n", serde_json::to_string_pretty(state)?),
    )?;
    std::fs::rename(temporary, path)?;
    Ok(())
}

fn check_interval_hours() -> i64 {
    std::env::var("RAINY_UPDATE_CHECK_INTERVAL_HOURS")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(DEFAULT_CHECK_INTERVAL_HOURS)
}

fn record_check_failure(state: &mut UpdateState) {
    state.last_checked = Some(Utc::now());
    state.consecutive_failures = state.consecutive_failures.saturating_add(1).min(6);
}

fn failure_retry_hours(failures: u32) -> i64 {
    FAILURE_RETRY_HOURS * (1_i64 << failures.saturating_sub(1).min(5))
}

fn http_get_limited(url: &str, error_code: &'static str) -> RainyResult<String> {
    validate_update_url(url, error_code)?;
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(StdDuration::from_secs(3))
        .timeout_read(StdDuration::from_secs(7))
        .timeout_write(StdDuration::from_secs(7))
        .build();
    let response = agent
        .get(url)
        .set("User-Agent", "rainy-cli")
        .call()
        .map_err(|err| RainyError::config(error_code, format!("request failed: {err}")))?;
    let mut reader = response.into_reader().take(MAX_UPDATE_RESPONSE_BYTES + 1);
    let mut body = String::new();
    reader
        .read_to_string(&mut body)
        .map_err(|err| RainyError::config(error_code, format!("response read failed: {err}")))?;
    if body.len() as u64 > MAX_UPDATE_RESPONSE_BYTES {
        return Err(RainyError::config(
            error_code,
            "response exceeds 1 MiB limit",
        ));
    }
    Ok(body)
}

fn target_asset() -> Option<String> {
    let target = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu.tar.gz",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu.tar.gz",
        ("macos", "x86_64") => "x86_64-apple-darwin.tar.gz",
        ("macos", "aarch64") => "aarch64-apple-darwin.tar.gz",
        ("windows", "x86_64") => "x86_64-pc-windows-msvc.zip",
        _ => return None,
    };
    Some(format!("rainy-{target}"))
}

fn verify_installed_version(expected: &str) -> RainyResult<()> {
    let binary = std::env::var_os("INSTALL_DIR")
        .map(PathBuf::from)
        .or_else(|| config_home().map(|home| home.join("bin")))
        .map(|dir| dir.join(if cfg!(windows) { "rainy.exe" } else { "rainy" }))
        .ok_or_else(|| {
            RainyError::config("UPDATE_VERIFY_FAILED", "cannot resolve install directory")
        })?;
    let output = std::process::Command::new(&binary)
        .arg("--version")
        .output()
        .map_err(|err| {
            RainyError::config(
                "UPDATE_VERIFY_FAILED",
                format!("failed to run {}: {err}", binary.display()),
            )
        })?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() || !stdout.trim_end().ends_with(expected) {
        return Err(RainyError::config(
            "UPDATE_VERIFY_FAILED",
            format!("installed binary did not report version {expected}"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions_numerically() {
        assert!(version_gt("0.10.0", "0.9.9").expect("semver"));
        assert!(version_gt("v1.0.0", "0.99.0").expect("semver"));
        assert!(!version_gt("0.1.0", "0.1.0").expect("semver"));
        assert!(!version_gt("0.1.0", "0.2.0").expect("semver"));
        assert!(version_gt("not-a-version", "0.2.0").is_err());
    }

    #[test]
    fn update_failure_backoff_is_bounded() {
        assert_eq!(failure_retry_hours(1), 1);
        assert_eq!(failure_retry_hours(2), 2);
        assert_eq!(failure_retry_hours(6), 32);
        assert_eq!(failure_retry_hours(100), 32);
    }

    #[test]
    fn normalizes_repository_slug() {
        assert_eq!(
            repository_slug(Some("owner/repo")).expect("owner repo"),
            "owner/repo"
        );
        assert_eq!(
            repository_slug(Some("https://github.com/owner/repo.git")).expect("https repo"),
            "owner/repo"
        );
        assert!(repository_slug(Some("owner/repo;rm")).is_err());
    }

    #[test]
    fn parses_latest_release_version() {
        let version = parse_latest_release_version(
            br#"{"tag_name":"v1.2.3","draft":false,"prerelease":false}"#,
        )
        .expect("release version");
        assert_eq!(version, "1.2.3");
    }

    #[test]
    fn parses_static_mirror_latest_marker() {
        assert_eq!(parse_latest_marker("v1.2.3\n").expect("marker"), "1.2.3");
        assert!(parse_latest_marker("latest").is_err());
        assert!(parse_latest_marker("v1.2.3\nv1.2.4").is_err());
    }

    #[test]
    fn update_urls_require_https_or_loopback_http() {
        validate_update_url("https://downloads.example.com/rainy", "TEST").expect("https URL");
        validate_update_url("http://127.0.0.1:8080/rainy", "TEST").expect("loopback URL");
        validate_update_url("http://localhost/rainy", "TEST").expect("localhost URL");
        assert!(validate_update_url("http://downloads.example.com/rainy", "TEST").is_err());
        assert!(validate_update_url("http://localhost.example.com/rainy", "TEST").is_err());
    }

    #[test]
    fn quotes_mirror_urls_for_posix_install_commands() {
        assert_eq!(
            shell_single_quote("https://example.com/team's/rainy"),
            "https://example.com/team'\\''s/rainy"
        );
    }

    #[test]
    fn rejects_draft_or_prerelease_latest_release() {
        let draft = parse_latest_release_version(
            br#"{"tag_name":"v1.2.3","draft":true,"prerelease":false}"#,
        )
        .expect_err("draft rejected");
        assert!(draft.to_string().contains("draft or prerelease"));

        let prerelease = parse_latest_release_version(
            br#"{"tag_name":"v1.2.3-rc.1","draft":false,"prerelease":true}"#,
        )
        .expect_err("prerelease rejected");
        assert!(prerelease.to_string().contains("draft or prerelease"));
    }

    #[test]
    fn rejects_explicit_prerelease_versions() {
        let error = parse_version("v1.2.3-rc.1").expect_err("prerelease must be rejected");
        assert_eq!(error.body().code, "UPDATE_PRERELEASE_UNSUPPORTED");
    }

    #[test]
    fn verifies_installer_checksums() {
        let bytes = b"installer";
        let digest = format!("{:x}", Sha256::digest(bytes));
        verify_installer_checksum("install.sh", bytes, &format!("{digest}  install.sh\n"))
            .expect("valid checksum");
        let error =
            verify_installer_checksum("install.ps1", bytes, &format!("{digest}  install.sh\n"))
                .expect_err("missing checksum must fail");
        assert_eq!(error.body().code, "UPDATE_INSTALL_CHECKSUM_MISSING");
    }

    #[test]
    fn skipped_versions_are_repository_scoped() {
        let state = UpdateState {
            skip_version: Some("1.2.3".to_string()),
            skip_repository: Some("owner/repo".to_string()),
            ..UpdateState::default()
        };
        assert!(is_skipped(&state, "owner/repo", "v1.2.3"));
        assert!(!is_skipped(&state, "other/repo", "v1.2.3"));
        assert!(!is_skipped(&state, "owner/repo", "1.2.4"));
    }

    #[test]
    fn skipped_versions_without_repository_remain_backwards_compatible() {
        let state = UpdateState {
            skip_version: Some("1.2.3".to_string()),
            skip_repository: None,
            ..UpdateState::default()
        };
        assert!(is_skipped(&state, "owner/repo", "1.2.3"));
        assert!(is_skipped(&state, "other/repo", "v1.2.3"));
    }
}
