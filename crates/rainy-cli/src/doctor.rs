use crate::cli::DoctorScope;
use crate::config;
use crate::error::{RainyError, RainyResult};
use crate::output::CommandOutput;
use crate::progress::ProgressReporter;
use crate::registry::{CapabilityDefinition, DoctorCheckSpec, RegistryClient};
use handlebars::Handlebars;
use serde::{Deserialize, Serialize};
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
    scope: DoctorScope,
    capability: Option<&str>,
    network: bool,
    progress: &ProgressReporter,
) -> RainyResult<CommandOutput> {
    progress.detail(format!("Checking {} health", doctor_scope_name(scope)));
    let mut checks = runtime_checks(workspace);
    let project_present = workspace.join("rainy.yaml").is_file();
    let skills_present = workspace.join("rainy-skills.yaml").is_file();
    let defaults_present =
        crate::paths::rainy_home().is_ok_and(|home| home.join("defaults.lock").is_file());

    if matches!(scope, DoctorScope::Project | DoctorScope::All)
        || (scope == DoctorScope::Auto && project_present)
    {
        if project_present {
            match run_doctor(workspace, capability) {
                Ok(report) => checks.extend(prefix_checks("project", report.checks)),
                Err(error) => {
                    let body = error.body();
                    checks.push(failed_check(
                        "project.configuration",
                        format!("{}: {}", body.code, body.message),
                    ));
                }
            }
            match crate::project_template::validate_project_template_lock(workspace) {
                Ok(Some(message)) => checks.push(DoctorCheckResult {
                    id: "project.template-provenance".to_string(),
                    status: "passed".to_string(),
                    message,
                }),
                Ok(None) => {}
                Err(error) => checks.push(failed_check(
                    "project.template-provenance",
                    format!("{}: {}", error.body().code, error.body().message),
                )),
            }
        } else {
            checks.push(failed_check("project.config", "rainy.yaml was not found"));
        }
    }
    if matches!(scope, DoctorScope::Skills | DoctorScope::All)
        || (scope == DoctorScope::Auto && skills_present)
    {
        match crate::skills::doctor_report(workspace) {
            Ok(report) => checks.extend(report.checks.into_iter().map(|check| DoctorCheckResult {
                id: format!("skills.{}", check.id),
                status: normalize_check_status(&check.status).to_string(),
                message: check.message,
            })),
            Err(error) => checks.push(failed_check("skills.config", error.body().message)),
        }
    }
    if matches!(scope, DoctorScope::Defaults | DoctorScope::All)
        || (scope == DoctorScope::Auto && defaults_present)
    {
        match crate::defaults::doctor_report() {
            Ok(report) => checks.push(DoctorCheckResult {
                id: "defaults.package".to_string(),
                status: normalize_check_status(&report.status).to_string(),
                message: report
                    .package_version
                    .map(|version| format!("default package {version} is available"))
                    .unwrap_or_else(|| "default package is not installed".to_string()),
            }),
            Err(error) => checks.push(failed_check("defaults.package", error.body().message)),
        }
    }
    if matches!(scope, DoctorScope::Registries | DoctorScope::All)
        || (scope == DoctorScope::Auto && project_has_registries(workspace))
    {
        match crate::registry::registry_doctor_report(workspace, None) {
            Ok(report) => checks.extend(report.checks.into_iter().map(|check| DoctorCheckResult {
                id: check.id,
                status: normalize_check_status(&check.status).to_string(),
                message: check.message,
            })),
            Err(error) => checks.push(failed_check("registries.config", error.body().message)),
        }
    }
    if network {
        checks.push(network_check());
    }

    let status = aggregate_status(&checks);
    Ok(CommandOutput::Doctor {
        report: DoctorReport {
            protocol_version: "rainy.doctor.v1".to_string(),
            status,
            workspace: workspace.to_string_lossy().to_string(),
            checks,
        },
    })
}

fn doctor_scope_name(scope: DoctorScope) -> &'static str {
    match scope {
        DoctorScope::Auto => "discovered workspace and runtime",
        DoctorScope::Project => "project",
        DoctorScope::Skills => "Skill profile",
        DoctorScope::Runtime => "runtime",
        DoctorScope::Defaults => "default package",
        DoctorScope::Registries => "registries",
        DoctorScope::All => "all configured components",
    }
}

fn runtime_checks(workspace: &Path) -> Vec<DoctorCheckResult> {
    let mut checks = Vec::new();
    let target = current_target();
    checks.push(DoctorCheckResult {
        id: "runtime.target".to_string(),
        status: if target == "unsupported" {
            "failed"
        } else {
            "passed"
        }
        .to_string(),
        message: if target == "unsupported" {
            format!(
                "unsupported platform {}-{}; Alpine/musl is not published",
                std::env::consts::ARCH,
                std::env::consts::OS
            )
        } else {
            format!("runtime target is supported: {target}")
        },
    });
    checks.push(match crate::paths::rainy_home() {
        Ok(home) => DoctorCheckResult {
            id: "runtime.rainy-home".to_string(),
            status: "passed".to_string(),
            message: format!("Rainy home resolves to {}", home.display()),
        },
        Err(error) => failed_check("runtime.rainy-home", error.body().message),
    });
    checks.push(match crate::audit::preflight(workspace) {
        Ok(()) => DoctorCheckResult {
            id: "runtime.audit".to_string(),
            status: "passed".to_string(),
            message: "audit storage is writable when required".to_string(),
        },
        Err(error) => failed_check("runtime.audit", error.body().message),
    });
    checks.push(runtime_path_check());
    checks
}

fn runtime_path_check() -> DoctorCheckResult {
    let executable = match std::env::current_exe() {
        Ok(path) => path,
        Err(error) => {
            return failed_check(
                "runtime.path",
                format!("cannot resolve the Rainy executable: {error}"),
            );
        }
    };
    let executable_name = executable.file_name().unwrap_or_default();
    let discoverable = std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|directory| directory.join(executable_name).is_file())
    });
    DoctorCheckResult {
        id: "runtime.path".to_string(),
        status: if discoverable { "passed" } else { "warning" }.to_string(),
        message: if discoverable {
            format!(
                "Rainy is discoverable through PATH ({})",
                executable.display()
            )
        } else {
            format!(
                "Rainy is running from {} but its directory is not in PATH",
                executable.display()
            )
        },
    }
}

fn current_target() -> &'static str {
    if cfg!(all(target_os = "linux", target_env = "musl")) {
        return "unsupported";
    }
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        _ => "unsupported",
    }
}

fn project_has_registries(workspace: &Path) -> bool {
    config::load_config(workspace)
        .is_ok_and(|config| !config.capability_registry.sources.is_empty())
}

fn prefix_checks(prefix: &str, checks: Vec<DoctorCheckResult>) -> Vec<DoctorCheckResult> {
    checks
        .into_iter()
        .map(|mut check| {
            check.id = format!("{prefix}.{}", check.id);
            check
        })
        .collect()
}

fn normalize_check_status(status: &str) -> &'static str {
    match status {
        "fail" | "failed" | "missing" => "failed",
        "warn" | "warning" | "degraded" => "warning",
        _ => "passed",
    }
}

fn aggregate_status(checks: &[DoctorCheckResult]) -> String {
    if checks.iter().any(|check| check.status == "failed") {
        "failed"
    } else if checks.iter().any(|check| check.status == "warning") {
        "warning"
    } else {
        "passed"
    }
    .to_string()
}

fn network_check() -> DoctorCheckResult {
    let url = std::env::var("RAINY_LATEST_VERSION_URL").unwrap_or_else(|_| {
        "https://api.github.com/repos/RainLib/rainy-cli/releases/latest".to_string()
    });
    if let Err(reason) = crate::security::validate_http(&url, true) {
        return failed_check(
            "network.update-source",
            format!("update URL is invalid: {reason}"),
        );
    }
    let result = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(3))
        .timeout_read(std::time::Duration::from_secs(5))
        .redirects(0)
        .build()
        .get(&url)
        .set("User-Agent", "rainy-cli")
        .call();
    match result {
        Ok(_) => DoctorCheckResult {
            id: "network.update-source".to_string(),
            status: "passed".to_string(),
            message: "update source is reachable".to_string(),
        },
        Err(error) => failed_check(
            "network.update-source",
            format!("update source is unreachable: {error}"),
        ),
    }
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
        for document in serde_yaml::Deserializer::from_str(&content) {
            let yaml = serde_yaml::Value::deserialize(document)?;
            collect_default_secret_checks(&rel_path, &yaml, &mut Vec::new(), &mut checks);
        }
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
