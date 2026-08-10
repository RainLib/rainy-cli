use crate::error::{RainyError, RainyResult};
use std::path::PathBuf;

pub fn rainy_home() -> RainyResult<PathBuf> {
    if let Some(path) = std::env::var_os("RAINY_HOME") {
        let path = PathBuf::from(path);
        if path.is_absolute() {
            return Ok(path);
        }
        return Err(RainyError::config(
            "RAINY_HOME_INVALID",
            "RAINY_HOME must be an absolute path",
        ));
    }
    user_home().map(|home| home.join(".rainy")).ok_or_else(|| {
        RainyError::config(
            "RAINY_HOME_NOT_FOUND",
            "cannot determine Rainy home; set RAINY_HOME to an absolute directory",
        )
    })
}

pub fn user_home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .or_else(|| std::env::var_os("USERPROFILE").filter(|value| !value.is_empty()))
        .map(PathBuf::from)
}

pub fn system_policy_path() -> PathBuf {
    if cfg!(windows) {
        std::env::var_os("ProgramData")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
            .join("Rainy/policy.yaml")
    } else {
        PathBuf::from("/etc/rainy/policy.yaml")
    }
}

pub fn system_plugin_bin() -> PathBuf {
    if cfg!(windows) {
        std::env::var_os("ProgramData")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
            .join("Rainy/plugins/bin")
    } else {
        PathBuf::from("/opt/rainy/plugins/bin")
    }
}
