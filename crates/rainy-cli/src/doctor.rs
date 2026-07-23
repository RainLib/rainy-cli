use crate::config;
use crate::error::{RainyError, RainyResult};
use crate::output::CommandOutput;
use crate::progress::ProgressReporter;
use crate::registry::{CapabilityDefinition, DoctorCheckSpec, RegistryClient};
use handlebars::Handlebars;
use serde::Serialize;
use std::path::Path;

const SENSITIVE_KEYS: &[&str] = &[
    "password",
    "secret",
    "token",
    "accesskey",
    "secretkey",
    "privatekey",
    "authorization",
    "cookie",
];

const DEFAULT_SECRET_VALUES: &[&str] = &[
    "admin",
    "changeme",
    "default",
    "minioadmin",
    "password",
    "secret",
    "test",
];

#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    pub status: String,
    pub workspace: String,
    pub checks: Vec<DoctorCheckResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorCheckResult {
    pub id: String,
    pub status: String,
    pub message: String,
}

pub fn doctor_command(
    workspace: &Path,
    capability: Option<&str>,
    progress: &ProgressReporter,
) -> RainyResult<CommandOutput> {
    progress.detail("Checking project files, locks, secrets, and capability health");
    let report = run_doctor(workspace, capability)?;
    if report.status == "failed" {
        return Err(RainyError::doctor(
            "DOCTOR_FAILED",
            serde_json::to_string(&report)?,
        ));
    }
    Ok(CommandOutput::Doctor { report })
}

pub fn run_doctor(workspace: &Path, capability: Option<&str>) -> RainyResult<DoctorReport> {
    let mut checks = Vec::new();
    checks.push(check_exists(
        workspace,
        "rainy.yaml",
        "project config exists",
    ));
    checks.push(check_exists(
        workspace,
        "capability.lock",
        "capability lock exists",
    ));

    let project_config = config::load_config(workspace)?;
    let lock = config::load_lock(workspace)?;
    checks.extend(default_secret_checks(workspace, &project_config)?);
    let registry = RegistryClient::load(workspace).ok();
    for (id, locked) in lock.capabilities {
        if capability.is_some_and(|wanted| wanted != id) {
            continue;
        }
        for artifact in locked.artifacts {
            checks.push(check_exists(
                workspace,
                &artifact,
                format!("{id} artifact exists: {artifact}"),
            ));
        }
        if let Some(registry) = &registry {
            match registry.get_capability(&id) {
                Ok(definition) => {
                    checks.extend(run_capability_checks(
                        workspace,
                        &project_config,
                        &definition,
                    )?);
                }
                Err(err) => checks.push(DoctorCheckResult {
                    id: format!("capability.definition:{id}"),
                    status: "warning".to_string(),
                    message: err.to_string(),
                }),
            }
        }
    }

    let status = if checks.iter().any(|check| check.status == "failed") {
        "failed"
    } else if checks.iter().any(|check| check.status == "warning") {
        "warning"
    } else {
        "passed"
    };

    Ok(DoctorReport {
        protocol_version: "rainy.doctor.v1".to_string(),
        status: status.to_string(),
        workspace: workspace.to_string_lossy().to_string(),
        checks,
    })
}

fn default_secret_checks(
    workspace: &Path,
    config: &config::ProjectConfig,
) -> RainyResult<Vec<DoctorCheckResult>> {
    let candidates = [
        format!(
            "{}/src/main/resources/application.yml",
            config.paths.backend
        ),
        format!(
            "{}/src/main/resources/application.yaml",
            config.paths.backend
        ),
    ];
    let mut checks = Vec::new();
    for rel_path in candidates {
        let path = workspace.join(&rel_path);
        if !path.exists() {
            continue;
        }
        let content = std::fs::read_to_string(&path)?;
        let yaml: serde_yaml::Value = serde_yaml::from_str(&content)?;
        collect_default_secret_checks(&rel_path, &yaml, &mut Vec::new(), &mut checks);
    }
    Ok(checks)
}

fn collect_default_secret_checks(
    rel_path: &str,
    value: &serde_yaml::Value,
    path: &mut Vec<String>,
    checks: &mut Vec<DoctorCheckResult>,
) {
    match value {
        serde_yaml::Value::Mapping(mapping) => {
            for (key, child) in mapping {
                path.push(yaml_key_to_string(key));
                collect_default_secret_checks(rel_path, child, path, checks);
                path.pop();
            }
        }
        serde_yaml::Value::Sequence(items) => {
            for (index, child) in items.iter().enumerate() {
                path.push(index.to_string());
                collect_default_secret_checks(rel_path, child, path, checks);
                path.pop();
            }
        }
        serde_yaml::Value::String(text)
            if path_is_sensitive(path) && value_is_default_secret(text) =>
        {
            let yaml_path = path.join(".");
            checks.push(DoctorCheckResult {
                id: format!("default-secret:{rel_path}:{yaml_path}"),
                status: "warning".to_string(),
                message: format!(
                    "DEFAULT_SECRET_VALUE: {rel_path} uses a development default at {yaml_path}"
                ),
            });
        }
        _ => {}
    }
}

fn yaml_key_to_string(value: &serde_yaml::Value) -> String {
    match value {
        serde_yaml::Value::String(text) => text.clone(),
        serde_yaml::Value::Number(number) => number.to_string(),
        serde_yaml::Value::Bool(value) => value.to_string(),
        serde_yaml::Value::Null => "null".to_string(),
        other => serde_yaml::to_string(other)
            .unwrap_or_default()
            .trim()
            .to_string(),
    }
}

fn path_is_sensitive(path: &[String]) -> bool {
    let Some(key) = path.last() else {
        return false;
    };
    let normalized = key
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect::<String>();
    SENSITIVE_KEYS
        .iter()
        .any(|sensitive| normalized.contains(sensitive))
}

fn value_is_default_secret(value: &str) -> bool {
    let normalized = value.trim().to_ascii_lowercase();
    DEFAULT_SECRET_VALUES
        .iter()
        .any(|default| normalized == *default)
}

fn check_exists(
    workspace: &Path,
    rel_path: impl AsRef<str>,
    message: impl Into<String>,
) -> DoctorCheckResult {
    let rel_path = rel_path.as_ref();
    let exists = workspace.join(rel_path).exists();
    DoctorCheckResult {
        id: format!("file.exists:{rel_path}"),
        status: if exists { "passed" } else { "failed" }.to_string(),
        message: if exists {
            message.into()
        } else {
            format!("missing {rel_path}")
        },
    }
}

fn run_capability_checks(
    workspace: &Path,
    config: &config::ProjectConfig,
    capability: &CapabilityDefinition,
) -> RainyResult<Vec<DoctorCheckResult>> {
    capability
        .doctor
        .checks
        .iter()
        .map(|check| run_capability_check(workspace, config, capability, check))
        .collect()
}

fn run_capability_check(
    workspace: &Path,
    config: &config::ProjectConfig,
    capability: &CapabilityDefinition,
    check: &DoctorCheckSpec,
) -> RainyResult<DoctorCheckResult> {
    let input = render_yaml_value(config, &capability.inputs, &check.with_value)?;
    match check.uses.as_str() {
        "file.exists" => {
            let path = required_string(&input, "path")?;
            Ok(check_exists(
                workspace,
                &path,
                format!("{} doctor check {} passed", capability.id, check.id),
            ))
        }
        "yaml.hasPath" => {
            let file = required_string(&input, "file")?;
            let yaml_path = required_string(&input, "path")?;
            let file_path = workspace.join(&file);
            if !file_path.exists() {
                return Ok(failed_check(&check.id, format!("missing YAML file {file}")));
            }
            let content = std::fs::read_to_string(file_path)?;
            let yaml: serde_yaml::Value = serde_yaml::from_str(&content)?;
            if yaml_has_path(&yaml, &yaml_path) {
                Ok(passed_check(
                    &check.id,
                    format!("{file} has path {yaml_path}"),
                ))
            } else {
                Ok(failed_check(
                    &check.id,
                    format!("{file} does not have path {yaml_path}"),
                ))
            }
        }
        "maven.hasDependency" => {
            let module_path = required_string(&input, "modulePath")?;
            let group_id = required_string(&input, "groupId")?;
            let artifact_id = required_string(&input, "artifactId")?;
            let pom = workspace.join(module_path).join("pom.xml");
            if !pom.exists() {
                return Ok(failed_check(
                    &check.id,
                    format!("missing {}", pom.display()),
                ));
            }
            let content = std::fs::read_to_string(&pom)?;
            let has_dependency = content.contains(&format!("<groupId>{group_id}</groupId>"))
                && content.contains(&format!("<artifactId>{artifact_id}</artifactId>"));
            if has_dependency {
                Ok(passed_check(
                    &check.id,
                    format!("dependency {group_id}:{artifact_id} exists"),
                ))
            } else {
                Ok(failed_check(
                    &check.id,
                    format!("dependency {group_id}:{artifact_id} missing"),
                ))
            }
        }
        other => Ok(DoctorCheckResult {
            id: check.id.clone(),
            status: "warning".to_string(),
            message: format!("unknown doctor check type: {other}"),
        }),
    }
}

fn passed_check(id: &str, message: impl Into<String>) -> DoctorCheckResult {
    DoctorCheckResult {
        id: id.to_string(),
        status: "passed".to_string(),
        message: message.into(),
    }
}

fn failed_check(id: &str, message: impl Into<String>) -> DoctorCheckResult {
    DoctorCheckResult {
        id: id.to_string(),
        status: "failed".to_string(),
        message: message.into(),
    }
}

fn render_yaml_value(
    config: &config::ProjectConfig,
    inputs: &std::collections::BTreeMap<String, crate::registry::CapabilityInput>,
    value: &serde_yaml::Value,
) -> RainyResult<serde_yaml::Value> {
    match value {
        serde_yaml::Value::String(text) => Ok(serde_yaml::Value::String(render_string(
            config, inputs, text,
        )?)),
        serde_yaml::Value::Sequence(items) => Ok(serde_yaml::Value::Sequence(
            items
                .iter()
                .map(|item| render_yaml_value(config, inputs, item))
                .collect::<RainyResult<Vec<_>>>()?,
        )),
        serde_yaml::Value::Mapping(mapping) => {
            let mut output = serde_yaml::Mapping::new();
            for (key, value) in mapping {
                output.insert(
                    render_yaml_value(config, inputs, key)?,
                    render_yaml_value(config, inputs, value)?,
                );
            }
            Ok(serde_yaml::Value::Mapping(output))
        }
        other => Ok(other.clone()),
    }
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
        .map_err(|err| RainyError::doctor("DOCTOR_RENDER_FAILED", err.to_string()))
}

fn required_string(value: &serde_yaml::Value, key: &str) -> RainyResult<String> {
    value
        .as_mapping()
        .and_then(|mapping| mapping.get(serde_yaml::Value::String(key.to_string())))
        .and_then(|value| match value {
            serde_yaml::Value::String(text) => Some(text.clone()),
            serde_yaml::Value::Number(number) => Some(number.to_string()),
            serde_yaml::Value::Bool(value) => Some(value.to_string()),
            _ => None,
        })
        .ok_or_else(|| RainyError::doctor("DOCTOR_INPUT_INVALID", format!("missing input: {key}")))
}

fn yaml_has_path(value: &serde_yaml::Value, path: &str) -> bool {
    let mut current = value;
    for segment in path.split('.') {
        let Some(mapping) = current.as_mapping() else {
            return false;
        };
        let key = serde_yaml::Value::String(segment.to_string());
        let Some(next) = mapping.get(&key) else {
            return false;
        };
        current = next;
    }
    true
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
