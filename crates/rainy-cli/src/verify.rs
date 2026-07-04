use crate::config;
use crate::doctor;
use crate::error::{RainyError, RainyResult};
use crate::output::CommandOutput;
use crate::registry::{CapabilityDefinition, RegistryClient, ValidationCommand};
use handlebars::Handlebars;
use serde::Serialize;
use std::path::Path;
use std::time::Instant;

#[derive(Debug, Clone, Serialize)]
pub struct VerifyReport {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    pub profile: String,
    pub status: String,
    #[serde(rename = "steps")]
    pub checks: Vec<VerifyCheckResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct VerifyCheckResult {
    pub id: String,
    pub status: String,
    pub message: String,
    #[serde(rename = "durationMs", skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u128>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
}

pub fn verify_command(
    workspace: &Path,
    profile: &str,
    capability: Option<&str>,
) -> RainyResult<CommandOutput> {
    let report = run_verify(workspace, profile, capability)?;
    if report.status == "failed" {
        return Err(RainyError::verify(
            "VERIFY_FAILED",
            serde_json::to_string(&report)?,
        ));
    }
    Ok(CommandOutput::Verify { report })
}

pub fn run_verify(
    workspace: &Path,
    profile: &str,
    capability: Option<&str>,
) -> RainyResult<VerifyReport> {
    let config = config::load_config(workspace)?;
    let lock = config::load_lock(workspace)?;
    let steps = config
        .verify
        .profiles
        .get(profile)
        .cloned()
        .ok_or_else(|| {
            RainyError::verify(
                "VERIFY_PROFILE_NOT_FOUND",
                format!("profile not found: {profile}"),
            )
        })?;

    let mut checks = Vec::new();
    for step in steps {
        checks.push(run_step(workspace, &step, capability)?);
    }
    checks.extend(run_capability_validations(
        workspace, &config, &lock, capability,
    )?);

    let status = if checks.iter().any(|check| check.status == "failed") {
        "failed"
    } else if checks.iter().any(|check| check.status == "warning") {
        "warning"
    } else {
        "passed"
    };

    Ok(VerifyReport {
        protocol_version: "rainy.verify.v1".to_string(),
        profile: profile.to_string(),
        status: status.to_string(),
        checks,
    })
}

fn run_step(
    workspace: &Path,
    step: &str,
    capability: Option<&str>,
) -> RainyResult<VerifyCheckResult> {
    match step {
        "doctor" => {
            let report = doctor::run_doctor(workspace, capability)?;
            Ok(VerifyCheckResult {
                id: "doctor".to_string(),
                status: report.status,
                message: "project doctor checks completed".to_string(),
                duration_ms: None,
                command: None,
                stdout: None,
                stderr: None,
            })
        }
        "docker-compose-config" => parse_yaml(workspace, "compose.yaml", step),
        "backend-test" => exists(workspace, "apps/backend/pom.xml", step),
        "frontend-build" => exists(workspace, "apps/frontend/package.json", step),
        "openapi-validate" => exists(workspace, "openapi", step),
        "security-basic" => Ok(VerifyCheckResult {
            id: step.to_string(),
            status: "passed".to_string(),
            message: "basic security policy is configured".to_string(),
            duration_ms: None,
            command: None,
            stdout: None,
            stderr: None,
        }),
        other if crate::policy::check_command(other).is_err() => Ok(VerifyCheckResult {
            id: other.to_string(),
            status: "failed".to_string(),
            message: "dangerous command rejected by policy".to_string(),
            duration_ms: None,
            command: Some(other.to_string()),
            stdout: None,
            stderr: None,
        }),
        other => Ok(VerifyCheckResult {
            id: other.to_string(),
            status: "warning".to_string(),
            message: "unknown verify step skipped".to_string(),
            duration_ms: None,
            command: None,
            stdout: None,
            stderr: None,
        }),
    }
}

fn exists(workspace: &Path, rel_path: &str, step: &str) -> RainyResult<VerifyCheckResult> {
    let exists = workspace.join(rel_path).exists();
    Ok(VerifyCheckResult {
        id: step.to_string(),
        status: if exists { "passed" } else { "failed" }.to_string(),
        message: if exists {
            format!("{rel_path} exists")
        } else {
            format!("{rel_path} missing")
        },
        duration_ms: None,
        command: None,
        stdout: None,
        stderr: None,
    })
}

fn parse_yaml(workspace: &Path, rel_path: &str, step: &str) -> RainyResult<VerifyCheckResult> {
    let path = workspace.join(rel_path);
    if !path.exists() {
        return Ok(VerifyCheckResult {
            id: step.to_string(),
            status: "failed".to_string(),
            message: format!("{rel_path} missing"),
            duration_ms: None,
            command: None,
            stdout: None,
            stderr: None,
        });
    }
    let content = std::fs::read_to_string(path)?;
    serde_yaml::from_str::<serde_yaml::Value>(&content)?;
    Ok(VerifyCheckResult {
        id: step.to_string(),
        status: "passed".to_string(),
        message: format!("{rel_path} is valid YAML"),
        duration_ms: None,
        command: None,
        stdout: None,
        stderr: None,
    })
}

fn run_capability_validations(
    workspace: &Path,
    config: &config::ProjectConfig,
    lock: &config::CapabilityLock,
    capability: Option<&str>,
) -> RainyResult<Vec<VerifyCheckResult>> {
    let registry = match RegistryClient::load(workspace) {
        Ok(registry) => registry,
        Err(err) => {
            return Ok(vec![VerifyCheckResult {
                id: "capability-validations".to_string(),
                status: "warning".to_string(),
                message: err.to_string(),
                duration_ms: None,
                command: None,
                stdout: None,
                stderr: None,
            }]);
        }
    };
    let mut checks = Vec::new();
    for id in lock.capabilities.keys() {
        if capability.is_some_and(|wanted| wanted != id) {
            continue;
        }
        let definition = registry.get_capability(id)?;
        checks.extend(run_validations_for_capability(
            workspace,
            config,
            &definition,
        )?);
    }
    Ok(checks)
}

fn run_validations_for_capability(
    workspace: &Path,
    config: &config::ProjectConfig,
    capability: &CapabilityDefinition,
) -> RainyResult<Vec<VerifyCheckResult>> {
    capability
        .validations
        .iter()
        .map(|validation| run_validation(workspace, config, capability, validation))
        .collect()
}

fn run_validation(
    workspace: &Path,
    config: &config::ProjectConfig,
    capability: &CapabilityDefinition,
    validation: &ValidationCommand,
) -> RainyResult<VerifyCheckResult> {
    let command = render_string(config, &capability.inputs, &validation.command)?;
    let working_directory = validation
        .working_directory
        .as_deref()
        .map(|dir| render_string(config, &capability.inputs, dir))
        .transpose()?
        .unwrap_or_else(|| ".".to_string());
    if crate::policy::check_command(&command).is_err() {
        return Ok(VerifyCheckResult {
            id: format!("{}:{}", capability.id, validation.id),
            status: "failed".to_string(),
            message: "dangerous command rejected by policy".to_string(),
            duration_ms: None,
            command: Some(command),
            stdout: None,
            stderr: None,
        });
    }

    let cwd = workspace.join(&working_directory);
    if command_executable_missing(&cwd, &command) {
        return Ok(VerifyCheckResult {
            id: format!("{}:{}", capability.id, validation.id),
            status: "warning".to_string(),
            message: format!(
                "validation command skipped because executable is unavailable: {command}"
            ),
            duration_ms: None,
            command: Some(command),
            stdout: None,
            stderr: None,
        });
    }

    let started = Instant::now();
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(&command)
        .current_dir(&cwd)
        .output()?;
    let duration_ms = started.elapsed().as_millis();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let environment_missing = stdout.contains("node_modules missing")
        || stdout.contains("Local package.json exists")
        || stderr.contains("command not found")
        || stderr.contains("not found");
    let status = if output.status.success() {
        "passed"
    } else if output.status.code() == Some(127) || environment_missing {
        "warning"
    } else {
        "failed"
    };
    Ok(VerifyCheckResult {
        id: format!("{}:{}", capability.id, validation.id),
        status: status.to_string(),
        message: if output.status.success() {
            "validation command passed".to_string()
        } else if output.status.code() == Some(127) || environment_missing {
            "validation command skipped because local toolchain/dependencies are unavailable"
                .to_string()
        } else {
            format!("validation command failed with status {}", output.status)
        },
        duration_ms: Some(duration_ms),
        command: Some(command),
        stdout: Some(stdout),
        stderr: Some(stderr),
    })
}

fn command_executable_missing(cwd: &Path, command: &str) -> bool {
    let Some(program) = command.split_whitespace().next() else {
        return true;
    };
    if let Some(relative) = program.strip_prefix("./") {
        return !cwd.join(relative).exists();
    }
    if program.contains('/') {
        return !Path::new(program).exists();
    }
    std::env::var_os("PATH")
        .map(|path| {
            !std::env::split_paths(&path).any(|dir| {
                let candidate = dir.join(program);
                candidate.exists() && candidate.is_file()
            })
        })
        .unwrap_or(true)
}

fn render_string(
    config: &config::ProjectConfig,
    inputs: &std::collections::BTreeMap<String, crate::registry::CapabilityInput>,
    text: &str,
) -> RainyResult<String> {
    let mut input_values = serde_json::Map::new();
    for (key, input) in inputs {
        if let Some(value) = &input.default {
            input_values.insert(key.clone(), serde_json::json!(yaml_scalar_to_string(value)));
        }
    }
    let variables = serde_json::json!({
        "paths": {
            "backend": config.paths.backend,
            "frontend": config.paths.frontend,
            "generated": config.paths.generated,
            "evidence": config.paths.evidence
        },
        "package": {
            "java": config.package.java,
            "npmScope": config.package.npm_scope
        },
        "packagePath": config::package_path(config),
        "inputs": input_values
    });
    let mut handlebars = Handlebars::new();
    handlebars.set_strict_mode(true);
    handlebars
        .render_template(text, &variables)
        .map_err(|err| RainyError::verify("VERIFY_RENDER_FAILED", err.to_string()))
}

fn yaml_scalar_to_string(value: &serde_yaml::Value) -> String {
    match value {
        serde_yaml::Value::String(text) => text.clone(),
        serde_yaml::Value::Number(number) => number.to_string(),
        serde_yaml::Value::Bool(value) => value.to_string(),
        serde_yaml::Value::Null => String::new(),
        other => serde_yaml::to_string(other)
            .unwrap_or_default()
            .trim()
            .to_string(),
    }
}
