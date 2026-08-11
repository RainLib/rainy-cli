use std::io;
use thiserror::Error;

pub type RainyResult<T> = Result<T, RainyError>;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorBody {
    pub code: String,
    pub category: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    pub retryable: bool,
    pub next_steps: Vec<String>,
}

#[derive(Debug, Error)]
pub enum RainyError {
    #[error("config error: {message}")]
    Config { code: String, message: String },
    #[error("registry error: {message}")]
    Registry { code: String, message: String },
    #[error("plan error: {message}")]
    Plan { code: String, message: String },
    #[error("policy denied: {message}")]
    Policy { code: String, message: String },
    #[error("action failed: {message}")]
    Action { code: String, message: String },
    #[error("doctor failed: {message}")]
    Doctor { code: String, message: String },
    #[error("verify failed: {message}")]
    Verify { code: String, message: String },
    #[error("plugin error: {message}")]
    Plugin { code: String, message: String },
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Yaml(#[from] serde_yaml::Error),
    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}

impl RainyError {
    pub fn config(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Config {
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn registry(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Registry {
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn plan(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Plan {
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn policy(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Policy {
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn action(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Action {
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn doctor(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Doctor {
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn verify(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Verify {
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn plugin(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Plugin {
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn body(&self) -> ErrorBody {
        let (code, message) = match self {
            Self::Config { code, message }
            | Self::Registry { code, message }
            | Self::Plan { code, message }
            | Self::Policy { code, message }
            | Self::Action { code, message }
            | Self::Doctor { code, message }
            | Self::Verify { code, message }
            | Self::Plugin { code, message } => (code.clone(), message.clone()),
            Self::Io(err) => ("IO_ERROR".to_string(), err.to_string()),
            Self::Json(err) => ("JSON_INVALID".to_string(), err.to_string()),
            Self::Yaml(err) => ("YAML_INVALID".to_string(), err.to_string()),
            Self::Anyhow(err) => ("RAINY_ERROR".to_string(), err.to_string()),
        };
        ErrorBody {
            category: self.category().to_string(),
            retryable: is_retryable(&code),
            next_steps: next_steps(&code)
                .iter()
                .map(|step| (*step).to_string())
                .collect(),
            code,
            message: crate::redaction::text(&message),
            details: None,
        }
    }

    pub fn exit_code(&self) -> i32 {
        let code = self.body().code;
        if code == "CANCELLED" {
            return 130;
        }
        if is_integrity_code(&code) {
            return 6;
        }
        if is_network_code(&code) {
            return 5;
        }
        match self {
            Self::Policy { .. } => 3,
            Self::Config { .. }
            | Self::Registry { .. }
            | Self::Plan { .. }
            | Self::Json(_)
            | Self::Yaml(_) => 2,
            _ => 1,
        }
    }

    pub fn category(&self) -> &'static str {
        let code = match self {
            Self::Config { code, .. }
            | Self::Registry { code, .. }
            | Self::Plan { code, .. }
            | Self::Policy { code, .. }
            | Self::Action { code, .. }
            | Self::Doctor { code, .. }
            | Self::Verify { code, .. }
            | Self::Plugin { code, .. } => Some(code.as_str()),
            _ => None,
        };
        if code.is_some_and(is_integrity_code) {
            return "integrity";
        }
        if code.is_some_and(is_network_code) {
            return "network";
        }
        match self {
            Self::Config { .. } | Self::Json(_) | Self::Yaml(_) => "configuration",
            Self::Registry { .. } => "registry",
            Self::Plan { .. } => "plan",
            Self::Policy { .. } => "policy",
            Self::Doctor { .. } => "diagnostic",
            Self::Verify { .. } => "verification",
            Self::Plugin { .. } => "plugin",
            Self::Io(_) => "io",
            Self::Action { .. } | Self::Anyhow(_) => "runtime",
        }
    }
}

fn is_network_code(code: &str) -> bool {
    code.contains("NETWORK")
        || code.contains("AUTH")
        || code.contains("DOWNLOAD")
        || code.contains("FETCH_FAILED")
        || code.contains("REMOTE_FAILED")
}

fn is_integrity_code(code: &str) -> bool {
    code.contains("CHECKSUM") || code.contains("DIGEST") || code.contains("SIGNATURE")
}

fn is_retryable(code: &str) -> bool {
    is_network_code(code) || code.contains("TIMEOUT") || code.contains("LOCKED")
}

fn next_steps(code: &str) -> &'static [&'static str] {
    match code {
        "CONFIG_NOT_FOUND" => &["rainy new --help", "rainy doctor --scope auto"],
        "CLI_ARGUMENT_INVALID" => &["rainy --help"],
        "VERIFY_PROFILE_NOT_FOUND" => &["rainy verify --help"],
        "SKILL_PROFILE_NOT_FOUND" => &["rainy skill install", "rainy skill install --help"],
        "DEFAULTS_GIT_FETCH_FAILED" | "DEFAULTS_GIT_REF_INVALID" => {
            &["rainy defaults status", "rainy defaults install --help"]
        }
        "SOURCE_NOT_FOUND" => &["rainy source list", "rainy source add --help"],
        "SOURCE_NOT_SYNCHRONIZED" | "SOURCE_CACHE_DIGEST_MISMATCH" => {
            &["rainy source list --verbose", "rainy source sync --help"]
        }
        "SOURCE_PROJECT_LOCK_NOT_FOUND" | "SOURCE_PROJECT_LOCK_VERSION_UNSUPPORTED" => {
            &["rainy new --help", "rainy source check --help"]
        }
        "SOURCE_MANIFEST_NOT_FOUND"
        | "SOURCE_MANIFEST_INVALID"
        | "SOURCE_CONTENT_INVALID"
        | "SOURCE_CONTENT_IDENTITY_MISMATCH"
        | "SOURCE_RELEASE_IDENTITY_MISMATCH" => &[
            "rainy schema validate --help",
            "rainy source inspect --help",
        ],
        "SOURCE_GIT_FETCH_FAILED" | "SOURCE_GIT_REMOTE_FAILED" | "SOURCE_DOWNLOAD_FAILED" => {
            &["rainy source check --help", "rainy source add --help"]
        }
        "PROJECT_TEMPLATE_GIT_FETCH_FAILED" | "PROJECT_TEMPLATE_REMOTE_FAILED" => {
            &["rainy template status", "rainy template check --help"]
        }
        "CANCELLED" => &[],
        _ => &[],
    }
}
