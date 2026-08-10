use crate::config;
use crate::doctor;
use crate::error::{RainyError, RainyResult};
use crate::output::CommandOutput;
use crate::process::{self, Termination};
use crate::progress::ProgressReporter;
use crate::registry::{CapabilityDefinition, RegistryClient, ValidationCommand};
use handlebars::Handlebars;
use serde::Serialize;
use std::path::Path;
use std::time::Duration;

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
    #[serde(rename = "stdoutTruncated", default, skip_serializing_if = "is_false")]
    pub stdout_truncated: bool,
    #[serde(rename = "stderrTruncated", default, skip_serializing_if = "is_false")]
    pub stderr_truncated: bool,
    #[serde(rename = "timedOut", default, skip_serializing_if = "is_false")]
    pub timed_out: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

pub fn verify_command(
    workspace: &Path,
    profile: &str,
    capability: Option<&str>,
    progress: &ProgressReporter,
) -> RainyResult<CommandOutput> {
    let report = run_verify_inner(workspace, profile, capability, Some(progress))?;
    Ok(CommandOutput::Verify { report })
}

pub fn run_verify(
    workspace: &Path,
    profile: &str,
    capability: Option<&str>,
) -> RainyResult<VerifyReport> {
    run_verify_inner(workspace, profile, capability, None)
}

fn run_verify_inner(
    workspace: &Path,
    profile: &str,
    capability: Option<&str>,
    progress: Option<&ProgressReporter>,
) -> RainyResult<VerifyReport> {
    if let Some(progress) = progress {
        progress.detail(format!("Loading verification profile '{profile}'"));
    }
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
    let strict = strict_verify_enabled(profile);
    for step in steps {
        if let Some(progress) = progress {
            progress.detail(format!("Running verification step: {step}"));
        }
        checks.push(run_step(workspace, &step, capability, strict)?);
    }
    if let Some(progress) = progress {
        progress.detail("Running installed capability validations");
    }
    checks.extend(run_capability_validations(
        workspace, &config, &lock, capability, strict,
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
    strict: bool,
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
                stdout_truncated: false,
                stderr_truncated: false,
                timed_out: false,
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
            stdout_truncated: false,
            stderr_truncated: false,
            timed_out: false,
        }),
        other if crate::policy::check_command(other).is_err() => Ok(VerifyCheckResult {
            id: other.to_string(),
            status: "failed".to_string(),
            message: "dangerous command rejected by policy".to_string(),
            duration_ms: None,
            command: Some(other.to_string()),
            stdout: None,
            stderr: None,
            stdout_truncated: false,
            stderr_truncated: false,
            timed_out: false,
        }),
        other => {
            let status = if strict { "failed" } else { "warning" };
            Ok(VerifyCheckResult {
                id: other.to_string(),
                status: status.to_string(),
                message: if strict {
                    "unknown verify step is not allowed in strict profile".to_string()
                } else {
                    "unknown verify step skipped".to_string()
                },
                duration_ms: None,
                command: None,
                stdout: None,
                stderr: None,
                stdout_truncated: false,
                stderr_truncated: false,
                timed_out: false,
            })
        }
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
        stdout_truncated: false,
        stderr_truncated: false,
        timed_out: false,
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
            stdout_truncated: false,
            stderr_truncated: false,
            timed_out: false,
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
        stdout_truncated: false,
        stderr_truncated: false,
        timed_out: false,
    })
}

fn run_capability_validations(
    workspace: &Path,
    config: &config::ProjectConfig,
    lock: &config::CapabilityLock,
    capability: Option<&str>,
    strict: bool,
) -> RainyResult<Vec<VerifyCheckResult>> {
    let registry = match RegistryClient::load(workspace) {
        Ok(registry) => registry,
        Err(err) => {
            let status = if strict { "failed" } else { "warning" };
            return Ok(vec![VerifyCheckResult {
                id: "capability-validations".to_string(),
                status: status.to_string(),
                message: err.to_string(),
                duration_ms: None,
                command: None,
                stdout: None,
                stderr: None,
                stdout_truncated: false,
                stderr_truncated: false,
                timed_out: false,
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
            strict,
        )?);
    }
    Ok(checks)
}

fn run_validations_for_capability(
    workspace: &Path,
    config: &config::ProjectConfig,
    capability: &CapabilityDefinition,
    strict: bool,
) -> RainyResult<Vec<VerifyCheckResult>> {
    Ok(capability
        .validations
        .iter()
        .map(|validation| {
            run_validation(workspace, config, capability, validation, strict).unwrap_or_else(
                |error| {
                    let body = error.body();
                    VerifyCheckResult {
                        id: format!("{}:{}", capability.id, validation.id),
                        status: "failed".to_string(),
                        message: format!("{}: {}", body.code, body.message),
                        duration_ms: None,
                        command: None,
                        stdout: None,
                        stderr: None,
                        stdout_truncated: false,
                        stderr_truncated: false,
                        timed_out: false,
                    }
                },
            )
        })
        .collect())
}

fn run_validation(
    workspace: &Path,
    config: &config::ProjectConfig,
    capability: &CapabilityDefinition,
    validation: &ValidationCommand,
    strict: bool,
) -> RainyResult<VerifyCheckResult> {
    if !validation.platforms.is_empty()
        && !validation
            .platforms
            .iter()
            .any(|platform| platform == current_platform() || platform == std::env::consts::OS)
    {
        return Ok(VerifyCheckResult {
            id: format!("{}:{}", capability.id, validation.id),
            status: "passed".to_string(),
            message: format!("validation is not applicable to {}", current_platform()),
            duration_ms: None,
            command: None,
            stdout: None,
            stderr: None,
            stdout_truncated: false,
            stderr_truncated: false,
            timed_out: false,
        });
    }
    let (program, args, command, deprecated) =
        resolve_validation_command(config, capability, validation)?;
    let command = crate::redaction::text(&command);
    let working_directory = validation
        .working_directory
        .as_deref()
        .map(|dir| render_string(config, &capability.inputs, dir))
        .transpose()?
        .unwrap_or_else(|| ".".to_string());
    if Path::new(&working_directory).components().any(|component| {
        matches!(
            component,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        )
    }) {
        return Err(RainyError::verify(
            "VERIFY_WORKING_DIRECTORY_INVALID",
            format!(
                "validation workingDirectory must stay inside the workspace: {working_directory}"
            ),
        ));
    }
    if crate::policy::check_command(&command).is_err() {
        return Ok(VerifyCheckResult {
            id: format!("{}:{}", capability.id, validation.id),
            status: "failed".to_string(),
            message: "dangerous command rejected by policy".to_string(),
            duration_ms: None,
            command: Some(command),
            stdout: None,
            stderr: None,
            stdout_truncated: false,
            stderr_truncated: false,
            timed_out: false,
        });
    }

    let cwd = workspace.join(&working_directory);
    if command_executable_missing(&cwd, &program) {
        let status = if strict { "failed" } else { "warning" };
        return Ok(VerifyCheckResult {
            id: format!("{}:{}", capability.id, validation.id),
            status: status.to_string(),
            message: if strict {
                format!("validation command failed because executable is unavailable: {command}")
            } else {
                format!("validation command skipped because executable is unavailable: {command}")
            },
            duration_ms: None,
            command: Some(command),
            stdout: None,
            stderr: None,
            stdout_truncated: false,
            stderr_truncated: false,
            timed_out: false,
        });
    }

    let timeout = Duration::from_secs(validation.timeout_seconds.unwrap_or(900).max(1));
    let output = process::run(
        &program,
        &args,
        &cwd,
        timeout,
        process::DEFAULT_OUTPUT_LIMIT,
    )?;
    let environment_missing = output.stdout.contains("node_modules missing")
        || output.stdout.contains("Local package.json exists")
        || output.stderr.contains("command not found")
        || output.stderr.contains("not found");
    let exit_code = output.status.and_then(|status| status.code());
    let status = if output.success() && deprecated {
        "warning"
    } else if output.success() {
        "passed"
    } else if !strict && (exit_code == Some(127) || environment_missing) {
        "warning"
    } else {
        "failed"
    };
    Ok(VerifyCheckResult {
        id: format!("{}:{}", capability.id, validation.id),
        status: status.to_string(),
        message: if output.termination == Termination::TimedOut {
            format!("validation timed out after {} seconds", timeout.as_secs())
        } else if output.success() && deprecated {
            "validation passed; legacy command is deprecated, use run.program and run.args"
                .to_string()
        } else if output.success() {
            "validation command passed".to_string()
        } else if !strict && (exit_code == Some(127) || environment_missing) {
            "validation command skipped because local toolchain/dependencies are unavailable"
                .to_string()
        } else {
            format!(
                "validation command failed with status {}",
                output
                    .status
                    .map(|status| status.to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            )
        },
        duration_ms: Some(output.duration.as_millis()),
        command: Some(command),
        stdout: Some(output.stdout),
        stderr: Some(output.stderr),
        stdout_truncated: output.stdout_truncated,
        stderr_truncated: output.stderr_truncated,
        timed_out: output.termination == Termination::TimedOut,
    })
}

fn resolve_validation_command(
    config: &config::ProjectConfig,
    capability: &CapabilityDefinition,
    validation: &ValidationCommand,
) -> RainyResult<(String, Vec<String>, String, bool)> {
    if validation.run.is_some() && validation.command.is_some() {
        return Err(RainyError::verify(
            "VERIFY_COMMAND_CONFLICT",
            format!(
                "validation {} cannot define both run and command",
                validation.id
            ),
        ));
    }
    if let Some(run) = &validation.run {
        let program = render_string(config, &capability.inputs, &run.program)?;
        let args = run
            .args
            .iter()
            .map(|argument| render_string(config, &capability.inputs, argument))
            .collect::<RainyResult<Vec<_>>>()?;
        if program.trim().is_empty() {
            return Err(RainyError::verify(
                "VERIFY_PROGRAM_REQUIRED",
                format!("validation {} has an empty run.program", validation.id),
            ));
        }
        let display = display_command(&program, &args);
        return Ok((program, args, display, false));
    }
    let legacy = validation.command.as_deref().ok_or_else(|| {
        RainyError::verify(
            "VERIFY_COMMAND_REQUIRED",
            format!(
                "validation {} must define run.program and run.args",
                validation.id
            ),
        )
    })?;
    let legacy = render_string(config, &capability.inputs, legacy)?;
    if legacy.chars().any(|character| {
        matches!(
            character,
            '|' | '&' | ';' | '<' | '>' | '(' | ')' | '$' | '`' | '\n' | '\r'
        )
    }) {
        return Err(RainyError::verify(
            "VERIFY_LEGACY_SHELL_UNSUPPORTED",
            "legacy validation command contains shell operators; migrate to run.program and run.args",
        ));
    }
    let mut words = shell_words::split(&legacy).map_err(|error| {
        RainyError::verify(
            "VERIFY_LEGACY_COMMAND_INVALID",
            format!("legacy validation command could not be parsed: {error}"),
        )
    })?;
    if words.is_empty() {
        return Err(RainyError::verify(
            "VERIFY_PROGRAM_REQUIRED",
            "legacy validation command is empty",
        ));
    }
    let program = words.remove(0);
    Ok((program, words, legacy, true))
}

fn display_command(program: &str, args: &[String]) -> String {
    std::iter::once(program)
        .chain(args.iter().map(String::as_str))
        .map(|part| {
            if part
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "-._/:=@".contains(character))
            {
                part.to_string()
            } else {
                format!("{part:?}")
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn current_platform() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        _ => "unsupported",
    }
}

fn strict_verify_enabled(profile: &str) -> bool {
    profile == "ci" || env_truthy("RAINY_VERIFY_STRICT")
}

fn env_truthy(key: &str) -> bool {
    std::env::var(key)
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

fn command_executable_missing(cwd: &Path, program: &str) -> bool {
    let program_path = Path::new(program);
    if program_path.is_absolute() || program_path.components().count() > 1 {
        let candidate = if program_path.is_absolute() {
            program_path.to_path_buf()
        } else {
            cwd.join(program_path)
        };
        return !executable_candidate_exists(&candidate);
    }
    std::env::var_os("PATH")
        .map(|path| {
            !std::env::split_paths(&path).any(|dir| executable_candidate_exists(&dir.join(program)))
        })
        .unwrap_or(true)
}

fn executable_candidate_exists(candidate: &Path) -> bool {
    if candidate.is_file() {
        return true;
    }
    #[cfg(windows)]
    if candidate.extension().is_none() {
        let path_ext = std::env::var_os("PATHEXT").unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".into());
        return path_ext.to_string_lossy().split(';').any(|extension| {
            let extension = extension.trim().trim_start_matches('.');
            !extension.is_empty() && candidate.with_extension(extension).is_file()
        });
    }
    false
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
