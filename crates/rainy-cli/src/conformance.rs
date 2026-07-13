use crate::cli::{ConformanceCommand, ConformanceSubcommand};
use crate::config;
use crate::error::{RainyError, RainyResult};
use crate::output::CommandOutput;
use crate::plugin::{self, PluginManifest};
use crate::registry::{CapabilityDefinition, CapabilityPack};
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct ConformanceReport {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    pub status: String,
    pub checks: Vec<ConformanceCheck>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConformanceCheck {
    pub id: String,
    pub status: String,
    pub message: String,
}

pub fn handle_conformance_command(command: ConformanceCommand) -> RainyResult<CommandOutput> {
    match command.command {
        ConformanceSubcommand::Check(args) => {
            let path = match args.path {
                Some(path) => path,
                None => config::default_registry_path()?,
            };
            let report = check_path(&path)?;
            if report.status == "failed" {
                return Err(RainyError::registry(
                    "CONFORMANCE_FAILED",
                    serde_json::to_string(&report)?,
                ));
            }
            Ok(CommandOutput::Conformance { report })
        }
    }
}

fn check_path(path: &Path) -> RainyResult<ConformanceReport> {
    let mut checks = Vec::new();
    let mut capability_ids = BTreeSet::new();
    let pack_roots = pack_roots(path)?;
    let plugin_roots = plugin_roots(path)?;
    if pack_roots.is_empty() && plugin_roots.is_empty() {
        checks.push(failed(
            format!("source:{}:discover", path.display()),
            "no pack.yaml or plugin.json files found",
        ));
    }
    for pack_root in pack_roots {
        check_pack(&pack_root, &mut capability_ids, &mut checks)?;
    }
    for plugin_root in plugin_roots {
        check_plugin(&plugin_root, &mut checks)?;
    }
    let status = if checks.iter().any(|check| check.status == "failed") {
        "failed"
    } else if checks.iter().any(|check| check.status == "warning") {
        "warning"
    } else {
        "passed"
    };
    Ok(ConformanceReport {
        protocol_version: "rainy.conformance.v1".to_string(),
        status: status.to_string(),
        checks,
    })
}

fn pack_roots(path: &Path) -> RainyResult<Vec<PathBuf>> {
    if path.join("pack.yaml").exists() {
        return Ok(vec![path.to_path_buf()]);
    }
    let mut roots = Vec::new();
    for entry in std::fs::read_dir(path).map_err(|err| {
        RainyError::registry(
            "CONFORMANCE_SOURCE_INVALID",
            format!("cannot read {}: {err}", path.display()),
        )
    })? {
        let entry = entry?;
        if entry.file_type()?.is_dir() && entry.path().join("pack.yaml").exists() {
            roots.push(entry.path());
        }
    }
    roots.sort();
    Ok(roots)
}

fn plugin_roots(path: &Path) -> RainyResult<Vec<PathBuf>> {
    if path.join("plugin.json").exists() {
        return Ok(vec![path.to_path_buf()]);
    }
    let mut roots = Vec::new();
    for entry in std::fs::read_dir(path).map_err(|err| {
        RainyError::registry(
            "CONFORMANCE_SOURCE_INVALID",
            format!("cannot read {}: {err}", path.display()),
        )
    })? {
        let entry = entry?;
        if entry.file_type()?.is_dir() && entry.path().join("plugin.json").exists() {
            roots.push(entry.path());
        }
    }
    roots.sort();
    Ok(roots)
}

fn check_pack(
    root: &Path,
    capability_ids: &mut BTreeSet<String>,
    checks: &mut Vec<ConformanceCheck>,
) -> RainyResult<()> {
    let pack_path = root.join("pack.yaml");
    let content = std::fs::read_to_string(&pack_path)?;
    let pack = match serde_yaml::from_str::<CapabilityPack>(&content) {
        Ok(pack) => {
            checks.push(passed(
                format!("pack:{}:parse", root.display()),
                "pack.yaml parses",
            ));
            pack
        }
        Err(err) => {
            checks.push(failed(
                format!("pack:{}:parse", root.display()),
                format!("pack.yaml parse failed: {err}"),
            ));
            return Ok(());
        }
    };
    if pack.api_version != "rainy.dev/v1" || pack.kind != "CapabilityPack" {
        checks.push(failed(
            format!("pack:{}:identity", pack.metadata.name),
            "pack apiVersion/kind is invalid",
        ));
    } else {
        checks.push(passed(
            format!("pack:{}:identity", pack.metadata.name),
            "pack apiVersion/kind is valid",
        ));
    }
    for capability_path in &pack.exports.capabilities {
        check_capability(
            root,
            &pack.metadata.name,
            capability_path,
            capability_ids,
            checks,
        )?;
    }
    Ok(())
}

fn check_capability(
    root: &Path,
    pack_name: &str,
    capability_path: &str,
    capability_ids: &mut BTreeSet<String>,
    checks: &mut Vec<ConformanceCheck>,
) -> RainyResult<()> {
    let path = root.join(capability_path);
    if !path.exists() {
        checks.push(failed(
            format!("capability:{pack_name}:{capability_path}:exists"),
            "capability file is missing",
        ));
        return Ok(());
    }
    let content = std::fs::read_to_string(&path)?;
    let capability = match serde_yaml::from_str::<CapabilityDefinition>(&content) {
        Ok(capability) => {
            checks.push(passed(
                format!("capability:{pack_name}:{capability_path}:parse"),
                "capability YAML parses",
            ));
            capability
        }
        Err(err) => {
            checks.push(failed(
                format!("capability:{pack_name}:{capability_path}:parse"),
                format!("capability parse failed: {err}"),
            ));
            return Ok(());
        }
    };
    if !capability_ids.insert(capability.id.clone()) {
        checks.push(failed(
            format!("capability:{}:duplicate", capability.id),
            "duplicate capability id",
        ));
    }
    for action in &capability.actions.install {
        if known_action(&action.uses) {
            checks.push(passed(
                format!("capability:{}:action:{}", capability.id, action.id),
                format!("known action {}", action.uses),
            ));
        } else {
            checks.push(failed(
                format!("capability:{}:action:{}", capability.id, action.id),
                format!("unknown action {}", action.uses),
            ));
        }
    }
    Ok(())
}

fn check_plugin(root: &Path, checks: &mut Vec<ConformanceCheck>) -> RainyResult<()> {
    let manifest_path = root.join("plugin.json");
    let content = std::fs::read_to_string(&manifest_path)?;
    let manifest = match serde_json::from_str::<PluginManifest>(&content) {
        Ok(manifest) => {
            checks.push(passed(
                format!("plugin:{}:parse", root.display()),
                "plugin.json parses",
            ));
            manifest
        }
        Err(err) => {
            checks.push(failed(
                format!("plugin:{}:parse", root.display()),
                format!("plugin.json parse failed: {err}"),
            ));
            return Ok(());
        }
    };
    if manifest.protocol_version == "rainy.plugin.v1" {
        checks.push(passed(
            format!("plugin:{}:protocol", manifest.name),
            "plugin protocol is valid",
        ));
    } else {
        checks.push(failed(
            format!("plugin:{}:protocol", manifest.name),
            format!("unsupported plugin protocol: {}", manifest.protocol_version),
        ));
    }
    push_plugin_validation(
        checks,
        format!("plugin:{}:permissions", manifest.name),
        plugin::validate_plugin_permissions(&manifest),
        "plugin permissions are valid",
    );
    push_plugin_validation(
        checks,
        format!("plugin:{}:actions", manifest.name),
        plugin::validate_plugin_actions(&manifest),
        "plugin actions are valid",
    );
    check_plugin_executables(root, &manifest, checks)?;
    Ok(())
}

fn push_plugin_validation(
    checks: &mut Vec<ConformanceCheck>,
    id: String,
    result: RainyResult<()>,
    passed_message: &str,
) {
    match result {
        Ok(()) => checks.push(passed(id, passed_message)),
        Err(err) => checks.push(failed(id, err.to_string())),
    }
}

fn check_plugin_executables(
    root: &Path,
    manifest: &PluginManifest,
    checks: &mut Vec<ConformanceCheck>,
) -> RainyResult<()> {
    let mut found = false;
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("rainy-") {
            continue;
        }
        found = true;
        if plugin::shadows_builtin_command(&name) {
            checks.push(failed(
                format!("plugin:{}:command:{name}", manifest.name),
                "plugin command shadows a built-in Rainy command",
            ));
        } else {
            checks.push(passed(
                format!("plugin:{}:command:{name}", manifest.name),
                "plugin command name is valid",
            ));
        }
    }
    if !found {
        checks.push(failed(
            format!("plugin:{}:command", manifest.name),
            "no rainy-* plugin executable found",
        ));
    }
    Ok(())
}

fn known_action(action: &str) -> bool {
    matches!(
        action,
        "maven.addDependency"
            | "maven.addBom"
            | "yaml.merge"
            | "json.merge"
            | "jsonc.merge"
            | "toml.merge"
            | "template.render"
            | "file.create"
            | "file.append"
            | "dockerCompose.addService"
            | "packageJson.addDependency"
            | "packageJson.addScript"
            | "githubActions.addWorkflow"
            | "devcontainer.merge"
            | "helm.renderChart"
            | "capabilityLock.update"
            | "agentsMd.generate"
            | "command.runValidation"
    )
}

fn passed(id: impl Into<String>, message: impl Into<String>) -> ConformanceCheck {
    ConformanceCheck {
        id: id.into(),
        status: "passed".to_string(),
        message: message.into(),
    }
}

fn failed(id: impl Into<String>, message: impl Into<String>) -> ConformanceCheck {
    ConformanceCheck {
        id: id.into(),
        status: "failed".to_string(),
        message: message.into(),
    }
}
