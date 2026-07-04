use std::io;
use thiserror::Error;

pub type RainyResult<T> = Result<T, RainyError>;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
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
        match self {
            Self::Config { code, message }
            | Self::Registry { code, message }
            | Self::Plan { code, message }
            | Self::Policy { code, message }
            | Self::Action { code, message }
            | Self::Doctor { code, message }
            | Self::Verify { code, message }
            | Self::Plugin { code, message } => ErrorBody {
                code: code.clone(),
                message: message.clone(),
            },
            Self::Io(err) => ErrorBody {
                code: "IO_ERROR".to_string(),
                message: err.to_string(),
            },
            Self::Json(err) => ErrorBody {
                code: "JSON_INVALID".to_string(),
                message: err.to_string(),
            },
            Self::Yaml(err) => ErrorBody {
                code: "YAML_INVALID".to_string(),
                message: err.to_string(),
            },
            Self::Anyhow(err) => ErrorBody {
                code: "RAINY_ERROR".to_string(),
                message: err.to_string(),
            },
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Policy { .. } => 3,
            Self::Doctor { .. } | Self::Verify { .. } => 2,
            _ => 1,
        }
    }
}
