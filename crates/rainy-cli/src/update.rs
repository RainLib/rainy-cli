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
    last_checked: Option<DateTime<Utc>>,
    latest_version: Option<String>,
    skip_version: Option<String>,
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
        SelfSubcommand::Check => check_command(),
        SelfSubcommand::Update(args) => update_command(args.force),
        SelfSubcommand::Skip(args) => skip_command(args.version),
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
    let Ok(report) = check_latest_with_state(&mut state) else {
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

fn check_command() -> RainyResult<CommandOutput> {
    let mut state = load_state().unwrap_or_default();
    let report = check_latest_with_state(&mut state)?;
    save_state(&state)?;
    Ok(CommandOutput::Update { report })
}

fn update_command(force: bool) -> RainyResult<CommandOutput> {
    if !force {
        let mut state = load_state().unwrap_or_default();
        let report = check_latest_with_state(&mut state)?;
        save_state(&state)?;
        if !report.update_available {
            return Ok(CommandOutput::Update { report });
        }
    }

    run_install_script()?;
    let mut state = load_state().unwrap_or_default();
    state.skip_version = None;
    state.last_checked = Some(Utc::now());
    save_state(&state)?;
    Ok(CommandOutput::message(
        "Rainy update installer completed. Run `rainy --version` to confirm the installed version.",
    ))
}

fn skip_command(version: Option<String>) -> RainyResult<CommandOutput> {
    let mut state = load_state().unwrap_or_default();
    let version = match version {
        Some(version) => normalize_version(&version),
        None => {
            let report = check_latest_with_state(&mut state)?;
            report.latest_version.unwrap_or_else(current_version)
        }
    };
    state.skip_version = Some(version.clone());
    save_state(&state)?;
    Ok(CommandOutput::message(format!(
        "Skipped Rainy update version {version}."
    )))
}

fn check_latest_with_state(state: &mut UpdateState) -> RainyResult<UpdateReport> {
    let repository = repository_slug()?;
    let latest = latest_release_version(&repository)?;
    state.last_checked = Some(Utc::now());
    state.latest_version = Some(latest.clone());
    let current = current_version();
    let update_available = version_gt(&latest, &current);
    let skipped = state
        .skip_version
        .as_deref()
        .is_some_and(|skipped| normalize_version(skipped) == latest);
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
    let release: GitHubRelease = serde_json::from_slice(&output.stdout)?;
    if release.draft || release.prerelease {
        return Err(RainyError::config(
            "UPDATE_CHECK_FAILED",
            "latest GitHub release is a draft or prerelease",
        ));
    }
    Ok(normalize_version(&release.tag_name))
}

fn run_install_script() -> RainyResult<()> {
    let repository = repository_slug()?;
    let install_url = format!("https://github.com/{repository}/releases/latest/download");
    let status = if cfg!(windows) {
        let command = format!(
            "$ErrorActionPreference='Stop'; $script=Join-Path $env:TEMP 'rainy-install.ps1'; Invoke-WebRequest -UseBasicParsing '{install_url}/install.ps1' -OutFile $script; & $script"
        );
        std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                &command,
            ])
            .status()
    } else {
        let command = format!("curl -fsSL {install_url}/install.sh | sh");
        std::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .status()
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

fn env_truthy(key: &str) -> bool {
    std::env::var(key)
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

fn repository_slug() -> RainyResult<String> {
    if let Ok(repository) = std::env::var("RAINY_UPDATE_REPO")
        && !repository.trim().is_empty()
    {
        return Ok(repository.trim().trim_end_matches('/').to_string());
    }
    let repository = env!("CARGO_PKG_REPOSITORY")
        .trim()
        .trim_end_matches('/')
        .to_string();
    repository
        .strip_prefix("https://github.com/")
        .or_else(|| repository.strip_prefix("git@github.com:"))
        .map(|slug| slug.trim_end_matches(".git").to_string())
        .filter(|slug| slug.split('/').count() == 2)
        .ok_or_else(|| {
            RainyError::config(
                "UPDATE_REPOSITORY_INVALID",
                "set RAINY_UPDATE_REPO=owner/repo to enable update checks",
            )
        })
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
}
