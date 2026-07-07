use crate::cli::{SelfCommand, SelfSubcommand};
use crate::error::{RainyError, RainyResult};
use crate::output::CommandOutput;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const DEFAULT_CHECK_INTERVAL_HOURS: i64 = 24;
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
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateState {
    repository: Option<String>,
    last_checked: Option<DateTime<Utc>>,
    latest_version: Option<String>,
    skip_version: Option<String>,
    skip_repository: Option<String>,
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
    let report = check_latest_with_state(&mut state, repo.as_deref())?;
    save_state(&state)?;
    Ok(CommandOutput::Update { report })
}

fn update_command(
    force: bool,
    version: Option<String>,
    repo: Option<String>,
) -> RainyResult<CommandOutput> {
    if !force && version.is_none() {
        let mut state = load_state().unwrap_or_default();
        let report = check_latest_with_state(&mut state, repo.as_deref())?;
        save_state(&state)?;
        if !report.update_available {
            return Ok(CommandOutput::Update { report });
        }
    }

    run_install_script(repo.as_deref(), version.as_deref())?;
    let mut state = load_state().unwrap_or_default();
    state.skip_version = None;
    state.skip_repository = None;
    state.last_checked = Some(Utc::now());
    save_state(&state)?;
    Ok(CommandOutput::message(
        "Rainy update installer completed. Run `rainy --version` to confirm the installed version.",
    ))
}

fn skip_command(version: Option<String>, repo: Option<String>) -> RainyResult<CommandOutput> {
    let mut state = load_state().unwrap_or_default();
    let repository = repository_slug(repo.as_deref())?;
    let version = match version {
        Some(version) => normalize_version(&version),
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
    let current = current_version();
    let update_available = version_gt(&latest, &current);
    let skipped = is_skipped(state, &repository, &latest);
    Ok(UpdateReport {
        protocol_version: UPDATE_PROTOCOL.to_string(),
        repository: repository.clone(),
        current_version: current,
        latest_version: Some(latest),
        update_available,
        skipped,
        install_command: install_command(&repository),
    })
}

fn latest_release_version(repository: &str) -> RainyResult<String> {
    let url = format!("https://api.github.com/repos/{repository}/releases/latest");
    let output = std::process::Command::new("curl")
        .args(["-fsSL", "-H", "User-Agent: rainy-cli", &url])
        .output()
        .map_err(|err| {
            RainyError::config(
                "UPDATE_CHECK_FAILED",
                format!("failed to run curl for release check: {err}"),
            )
        })?;
    if !output.status.success() {
        return Err(RainyError::config(
            "UPDATE_CHECK_FAILED",
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }
    parse_latest_release_version(&output.stdout)
}

fn parse_latest_release_version(release_json: &[u8]) -> RainyResult<String> {
    let release: GitHubRelease = serde_json::from_slice(release_json)?;
    if release.draft || release.prerelease {
        return Err(RainyError::config(
            "UPDATE_CHECK_FAILED",
            "latest GitHub release is a draft or prerelease",
        ));
    }
    Ok(normalize_version(&release.tag_name))
}

fn run_install_script(repo: Option<&str>, version: Option<&str>) -> RainyResult<()> {
    let repository = repository_slug(repo)?;
    let version = version.map(normalize_version);
    let install_url = format!("https://github.com/{repository}/releases/latest/download");
    let status = if cfg!(windows) {
        let ps_command = format!(
            "$ErrorActionPreference='Stop'; $script=Join-Path $env:TEMP 'rainy-install.ps1'; Invoke-WebRequest -UseBasicParsing '{install_url}/install.ps1' -OutFile $script; & $script"
        );
        let mut command = std::process::Command::new("powershell");
        command
            .args([
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                &ps_command,
            ])
            .env("RAINY_REPO", &repository);
        if let Some(version) = &version {
            command.env("RAINY_VERSION", version);
        }
        command.status()
    } else {
        let command = format!("curl -fsSL {install_url}/install.sh | sh");
        let mut shell = std::process::Command::new("sh");
        shell.arg("-c").arg(command).env("RAINY_REPO", &repository);
        if let Some(version) = &version {
            shell.env("RAINY_VERSION", version);
        }
        shell.status()
    }?;
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
    let interval = std::env::var("RAINY_UPDATE_CHECK_INTERVAL_HOURS")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(DEFAULT_CHECK_INTERVAL_HOURS);
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
    if normalize_version(skip_version) != normalize_version(latest) {
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
    if cfg!(windows) {
        format!(
            "powershell -ExecutionPolicy Bypass -c \"iwr https://github.com/{repository}/releases/latest/download/install.ps1 -UseB | iex\""
        )
    } else {
        format!(
            "curl -fsSL https://github.com/{repository}/releases/latest/download/install.sh | sh"
        )
    }
}

fn current_version() -> String {
    normalize_version(env!("CARGO_PKG_VERSION"))
}

fn normalize_version(version: &str) -> String {
    version.trim().trim_start_matches('v').to_string()
}

fn version_gt(left: &str, right: &str) -> bool {
    let left = parse_version(left);
    let right = parse_version(right);
    left > right
}

fn parse_version(version: &str) -> Vec<u64> {
    normalize_version(version)
        .split(['.', '-', '+'])
        .map(|part| part.parse::<u64>().unwrap_or(0))
        .collect()
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
    std::fs::write(path, format!("{}\n", serde_json::to_string_pretty(state)?))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions_numerically() {
        assert!(version_gt("0.10.0", "0.9.9"));
        assert!(version_gt("v1.0.0", "0.99.0"));
        assert!(!version_gt("0.1.0", "0.1.0"));
        assert!(!version_gt("0.1.0", "0.2.0"));
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
