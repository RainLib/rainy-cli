use crate::cli::{PluginCommand, PluginSubcommand};
use crate::error::{RainyError, RainyResult};
use crate::output::CommandOutput;
use crate::patch::{self, ChangeSet};
use crate::policy;
use globset::{Glob, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

const MAX_PLUGIN_RESPONSE_BYTES: u64 = 5 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct PluginInfo {
    pub name: String,
    pub path: String,
    pub version: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "shadowedPaths")]
    pub shadowed_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    pub protocol_version: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub commands: Vec<PluginManifestCommand>,
    #[serde(default)]
    pub actions: Vec<PluginManifestAction>,
    #[serde(default)]
    pub permissions: serde_json::Value,
    #[serde(default)]
    pub adapter: Option<PluginAdapter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum PluginAdapter {
    Http { url: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifestCommand {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifestAction {
    pub id: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "inputSchema", default)]
    pub input_schema: Option<String>,
    #[serde(default)]
    pub runtime: Option<String>,
    #[serde(default)]
    pub wasm: Option<String>,
}

pub fn handle_plugin_command(
    workspace: &Path,
    command: PluginCommand,
    allow_native_plugin: bool,
) -> RainyResult<CommandOutput> {
    match command.command {
        PluginSubcommand::List => Ok(CommandOutput::Plugins {
            plugins: discover_plugins(workspace)?,
        }),
        PluginSubcommand::Inspect { id } => {
            let plugins = discover_plugins(workspace)?
                .into_iter()
                .filter(|plugin| plugin.name == id || plugin.name == format!("rainy-{id}"))
                .collect::<Vec<_>>();
            if plugins.is_empty() {
                return Err(RainyError::plugin(
                    "PLUGIN_NOT_FOUND",
                    format!("plugin not found: {id}"),
                ));
            }
            Ok(CommandOutput::Plugins { plugins })
        }
        PluginSubcommand::Install(args) => install_plugin(workspace, args, allow_native_plugin),
        PluginSubcommand::Call(args) => call_plugin(workspace, args),
    }
}

pub fn run_external(
    workspace: &Path,
    args: Vec<OsString>,
    allow_native_plugin: bool,
) -> RainyResult<CommandOutput> {
    ensure_native_plugin_allowed(workspace, allow_native_plugin)?;
    let Some(command) = args.first() else {
        return Err(RainyError::plugin(
            "PLUGIN_COMMAND_INVALID",
            "missing external plugin command",
        ));
    };
    let command = command.to_string_lossy();
    let plugin_name = format!("rainy-{command}");
    let plugin = discover_plugins(workspace)?
        .into_iter()
        .find(|plugin| plugin.name == plugin_name)
        .ok_or_else(|| {
            RainyError::plugin(
                "PLUGIN_NOT_FOUND",
                format!("plugin not found for external command: {command}"),
            )
        })?;
    let manifest = load_manifest(workspace, plugin.name.trim_start_matches("rainy-")).ok();
    if let Some(manifest) = &manifest {
        ensure_plugin_command_allowed(manifest, &args)?;
    }

    let output = std::process::Command::new(&plugin.path)
        .args(args.into_iter().skip(1))
        .env(
            "RAINY_PLUGIN_NETWORK",
            plugin_network_permission(manifest.as_ref()),
        )
        .output()?;
    if !output.status.success() {
        return Err(RainyError::plugin(
            "PLUGIN_EXECUTION_FAILED",
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }

    Ok(CommandOutput::PluginRun {
        plugin: plugin.name,
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

fn discover_plugins(workspace: &Path) -> RainyResult<Vec<PluginInfo>> {
    let mut found = BTreeMap::<String, DiscoveredPlugin>::new();
    let mut dirs = Vec::new();
    dirs.push(workspace.join(".rainy/plugins/bin"));
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".rainy/plugins/bin"));
    }
    dirs.push(PathBuf::from("/opt/rainy/plugins/bin"));
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }

    for dir in dirs {
        collect_from_dir(&dir, &mut found)?;
    }

    Ok(found
        .into_iter()
        .map(|(name, discovered)| {
            let manifest = load_manifest(workspace, name.trim_start_matches("rainy-")).ok();
            PluginInfo {
                name,
                path: discovered.path.to_string_lossy().to_string(),
                version: manifest.as_ref().map(|manifest| manifest.version.clone()),
                description: manifest.and_then(|manifest| manifest.description),
                shadowed_paths: discovered
                    .shadowed_paths
                    .into_iter()
                    .map(|path| path.to_string_lossy().to_string())
                    .collect(),
            }
        })
        .collect())
}

#[derive(Debug)]
struct DiscoveredPlugin {
    path: PathBuf,
    shadowed_paths: Vec<PathBuf>,
}

fn collect_from_dir(dir: &Path, found: &mut BTreeMap<String, DiscoveredPlugin>) -> RainyResult<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("rainy-") || name == "rainy-cli" || shadows_builtin_command(&name) {
            continue;
        }
        let path = entry.path();
        if let Some(existing) = found.get_mut(&name) {
            if existing.path != path && !existing.shadowed_paths.contains(&path) {
                existing.shadowed_paths.push(path);
            }
        } else {
            found.insert(
                name,
                DiscoveredPlugin {
                    path,
                    shadowed_paths: Vec::new(),
                },
            );
        }
    }
    Ok(())
}

fn install_plugin(
    workspace: &Path,
    args: crate::cli::PluginInstallArgs,
    allow_native_plugin: bool,
) -> RainyResult<CommandOutput> {
    let apply = resolve_apply_flags(args.dry_run, args.apply)?;
    let source = prepare_plugin_source(workspace, &args.source, apply)?;
    let plugin_files = plugin_files(&source)?;
    if plugin_files.is_empty() {
        return Err(RainyError::plugin(
            "PLUGIN_INVALID",
            format!("no rainy-* executable found in {}", source.display()),
        ));
    }
    ensure_native_plugin_allowed(workspace, allow_native_plugin)?;

    let target_dir = workspace.join(".rainy/plugins/bin");
    let mut changes = crate::patch::ChangeSet::new();
    for plugin_file in &plugin_files {
        let name = plugin_file
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| RainyError::plugin("PLUGIN_INVALID", "invalid plugin filename"))?;
        if shadows_builtin_command(name) {
            return Err(RainyError::plugin(
                "PLUGIN_COMMAND_SHADOWS_BUILTIN",
                format!("plugin executable {name} would shadow a built-in command"),
            ));
        }
        let target = target_dir.join(name);
        let rel = target
            .strip_prefix(workspace)
            .unwrap_or(&target)
            .to_string_lossy()
            .replace('\\', "/");
        let content = std::fs::read_to_string(plugin_file)?;
        changes.push(crate::patch::change_for_file(
            workspace,
            rel,
            content,
            format!("install plugin {name}"),
        )?);
    }

    let mut wasm_assets = Vec::new();
    if let Some(manifest_path) = manifest_file(&source) {
        let manifest_content = std::fs::read_to_string(&manifest_path)?;
        let manifest: PluginManifest = serde_json::from_str(&manifest_content).map_err(|err| {
            RainyError::plugin(
                "PLUGIN_MANIFEST_INVALID",
                format!("invalid plugin manifest {}: {err}", manifest_path.display()),
            )
        })?;
        if manifest.protocol_version != "rainy.plugin.v1" {
            return Err(RainyError::plugin(
                "PLUGIN_MANIFEST_INVALID",
                format!("unsupported plugin protocol: {}", manifest.protocol_version),
            ));
        }
        validate_plugin_permissions(&manifest)?;
        validate_plugin_actions(&manifest)?;
        wasm_assets = collect_wasm_assets(&manifest, &manifest_path)?;
        changes.push(crate::patch::change_for_file(
            workspace,
            format!(".rainy/plugins/manifests/{}.json", manifest.name),
            format!("{}\n", serde_json::to_string_pretty(&manifest)?),
            format!("install plugin manifest {}", manifest.name),
        )?);
    }

    if apply {
        let mut policy_changes = changes.clone();
        for asset in &wasm_assets {
            policy_changes.push(patch::Change {
                kind: patch::ChangeKind::Create,
                path: asset.target_rel.to_string_lossy().replace('\\', "/"),
                before: None,
                after: None,
                summary: "install plugin wasm asset".to_string(),
                noop: false,
            });
        }
        policy::check_changes(workspace, &policy_changes)?;
        crate::patch::apply_changes(workspace, &changes)?;
        for asset in wasm_assets {
            let target = workspace.join(asset.target_rel);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&asset.source, target)?;
        }
        #[cfg(unix)]
        for plugin_file in plugin_files {
            use std::os::unix::fs::PermissionsExt;
            let target = target_dir.join(plugin_file.file_name().expect("plugin filename"));
            let mut permissions = std::fs::metadata(&target)?.permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(target, permissions)?;
        }
        Ok(CommandOutput::change_applied("plugin install", changes))
    } else {
        Ok(CommandOutput::change_dry_run("plugin install", changes))
    }
}

fn call_plugin(workspace: &Path, args: crate::cli::PluginCallArgs) -> RainyResult<CommandOutput> {
    let apply = resolve_apply_flags(args.dry_run, args.apply)?;
    let manifest = load_manifest(workspace, &args.id)?;
    let action = manifest
        .actions
        .iter()
        .find(|action| action.id == args.action)
        .cloned()
        .ok_or_else(|| {
            RainyError::plugin(
                "PLUGIN_PERMISSION_DENIED",
                format!("plugin action is not declared in manifest: {}", args.action),
            )
        })?;
    let input = if let Some(path) = &args.input {
        let content = std::fs::read_to_string(path)?;
        if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext == "json")
        {
            serde_json::from_str(&content)?
        } else {
            serde_json::to_value(serde_yaml::from_str::<serde_yaml::Value>(&content)?)?
        }
    } else {
        serde_json::json!({})
    };
    let changes = if is_wasm_action(&action) {
        call_wasm_action(workspace, &manifest, &action, input)?
    } else {
        match manifest.adapter.as_ref() {
            Some(PluginAdapter::Http { url }) => {
                ensure_network_allowed(&manifest)?;
                call_http_adapter(url, &args.action, input)?
            }
            None => {
                return Err(RainyError::plugin(
                    "PLUGIN_ADAPTER_NOT_FOUND",
                    format!("plugin {} does not declare an adapter", manifest.name),
                ));
            }
        }
    };
    check_plugin_change_permissions(&manifest, &changes)?;
    if apply {
        policy::check_changes(workspace, &changes)?;
        patch::apply_changes(workspace, &changes)?;
        Ok(CommandOutput::change_applied(
            format!("plugin call {} {}", args.id, args.action),
            changes,
        ))
    } else {
        Ok(CommandOutput::change_dry_run(
            format!("plugin call {} {}", args.id, args.action),
            changes,
        ))
    }
}

fn resolve_apply_flags(dry_run: bool, apply: bool) -> RainyResult<bool> {
    if dry_run && apply {
        return Err(RainyError::plugin(
            "APPLY_MODE_CONFLICT",
            "--dry-run and --apply cannot be used together",
        ));
    }
    Ok(apply)
}

fn ensure_native_plugin_allowed(workspace: &Path, allowed: bool) -> RainyResult<()> {
    if !allowed {
        return Err(RainyError::plugin(
            "PLUGIN_NATIVE_NOT_TRUSTED",
            "native plugins execute with host process permissions; rerun with --allow-native-plugin only after reviewing and trusting the plugin",
        ));
    }
    if !workspace.join("rainy.yaml").exists() {
        return Err(RainyError::plugin(
            "PLUGIN_NATIVE_AUDIT_REQUIRED",
            "native plugins must run inside a Rainy project so execution can be audited",
        ));
    }
    Ok(())
}

fn is_wasm_action(action: &PluginManifestAction) -> bool {
    action.runtime.as_deref() == Some("wasm") || action.wasm.is_some()
}

fn call_wasm_action(
    workspace: &Path,
    manifest: &PluginManifest,
    action: &PluginManifestAction,
    input: serde_json::Value,
) -> RainyResult<ChangeSet> {
    let wasm = action.wasm.as_deref().ok_or_else(|| {
        RainyError::plugin(
            "PLUGIN_WASM_INVALID",
            format!("wasm action {} must declare a wasm module", action.id),
        )
    })?;
    let wasm_rel = validate_relative_asset_path(wasm)?;
    let wasm_path = workspace
        .join(".rainy/plugins/wasm")
        .join(&manifest.name)
        .join(&wasm_rel);
    if !wasm_path.exists() {
        return Err(RainyError::plugin(
            "PLUGIN_WASM_NOT_FOUND",
            format!("wasm module not installed: {}", wasm_path.display()),
        ));
    }

    let engine = wasmtime::Engine::default();
    let module_bytes = std::fs::read(&wasm_path)?;
    let module = wasmtime::Module::new(&engine, module_bytes).map_err(|err| {
        RainyError::plugin(
            "PLUGIN_WASM_INVALID",
            format!("invalid wasm module {}: {err}", wasm_path.display()),
        )
    })?;
    let mut store = wasmtime::Store::new(&engine, ());
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).map_err(|err| {
        RainyError::plugin(
            "PLUGIN_WASM_FAILED",
            format!("instantiate wasm failed: {err}"),
        )
    })?;
    let memory = instance
        .get_memory(&mut store, "memory")
        .ok_or_else(|| RainyError::plugin("PLUGIN_WASM_INVALID", "wasm must export memory"))?;

    let input_bytes = serde_json::to_vec(&input)?;
    let packed = match (
        instance.get_typed_func::<i32, i32>(&mut store, "rainy_alloc"),
        instance.get_typed_func::<(i32, i32), i64>(&mut store, "rainy_action"),
    ) {
        (Ok(alloc), Ok(action_func)) => {
            let input_len = i32::try_from(input_bytes.len()).map_err(|_| {
                RainyError::plugin(
                    "PLUGIN_WASM_INPUT_TOO_LARGE",
                    "wasm action input is too large",
                )
            })?;
            let ptr = alloc.call(&mut store, input_len).map_err(|err| {
                RainyError::plugin("PLUGIN_WASM_FAILED", format!("wasm alloc failed: {err}"))
            })?;
            if ptr < 0 {
                return Err(RainyError::plugin(
                    "PLUGIN_WASM_FAILED",
                    "wasm allocator returned a negative pointer",
                ));
            }
            let ptr = ptr as usize;
            let end = ptr.checked_add(input_bytes.len()).ok_or_else(|| {
                RainyError::plugin("PLUGIN_WASM_FAILED", "wasm input pointer overflow")
            })?;
            {
                let data = memory.data_mut(&mut store);
                if end > data.len() {
                    return Err(RainyError::plugin(
                        "PLUGIN_WASM_FAILED",
                        "wasm allocator returned a region outside memory",
                    ));
                }
                data[ptr..end].copy_from_slice(&input_bytes);
            }
            action_func
                .call(&mut store, (ptr as i32, input_len))
                .map_err(|err| {
                    RainyError::plugin("PLUGIN_WASM_FAILED", format!("wasm action failed: {err}"))
                })?
        }
        _ => {
            let action_func = instance
                .get_typed_func::<(), i64>(&mut store, "rainy_action")
                .map_err(|err| {
                    RainyError::plugin(
                        "PLUGIN_WASM_INVALID",
                        format!(
                            "wasm must export rainy_action() -> i64 or rainy_action(i32, i32) -> i64: {err}"
                        ),
                    )
                })?;
            action_func.call(&mut store, ()).map_err(|err| {
                RainyError::plugin("PLUGIN_WASM_FAILED", format!("wasm action failed: {err}"))
            })?
        }
    };

    let response = read_wasm_response(&store, &memory, packed)?;
    parse_plugin_response(&response)
}

fn read_wasm_response(
    store: &wasmtime::Store<()>,
    memory: &wasmtime::Memory,
    packed: i64,
) -> RainyResult<String> {
    let packed = packed as u64;
    let ptr = (packed >> 32) as usize;
    let len = (packed & 0xffff_ffff) as usize;
    let end = ptr.checked_add(len).ok_or_else(|| {
        RainyError::plugin("PLUGIN_WASM_FAILED", "wasm response pointer overflow")
    })?;
    let data = memory.data(store);
    if end > data.len() {
        return Err(RainyError::plugin(
            "PLUGIN_WASM_FAILED",
            "wasm response points outside memory",
        ));
    }
    std::str::from_utf8(&data[ptr..end])
        .map(|value| value.to_string())
        .map_err(|err| {
            RainyError::plugin(
                "PLUGIN_RESPONSE_INVALID",
                format!("wasm response is not valid UTF-8: {err}"),
            )
        })
}

fn ensure_network_allowed(manifest: &PluginManifest) -> RainyResult<()> {
    let network = manifest
        .permissions
        .get("network")
        .and_then(|value| value.as_str())
        .unwrap_or("none");
    if network == "none" {
        return Err(RainyError::plugin(
            "PLUGIN_PERMISSION_DENIED",
            format!("plugin {} does not allow network access", manifest.name),
        ));
    }
    Ok(())
}

fn call_http_adapter(url: &str, action: &str, input: serde_json::Value) -> RainyResult<ChangeSet> {
    let body = serde_json::json!({
        "protocolVersion": "rainy.plugin-rpc.v1",
        "action": action,
        "input": input
    });
    let response = http_post_json(url, &body)?;
    parse_plugin_response(&response)
}

fn parse_plugin_response(response: &str) -> RainyResult<ChangeSet> {
    let envelope: PluginRpcResponse = serde_json::from_str(response).map_err(|err| {
        RainyError::plugin(
            "PLUGIN_RESPONSE_INVALID",
            format!("invalid plugin response: {err}"),
        )
    })?;
    if envelope.protocol_version != "rainy.plugin-rpc.v1" {
        return Err(RainyError::plugin(
            "PLUGIN_RESPONSE_INVALID",
            format!(
                "unsupported plugin response protocol: {}",
                envelope.protocol_version
            ),
        ));
    }
    Ok(envelope.change_set)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginRpcResponse {
    protocol_version: String,
    change_set: ChangeSet,
}

fn http_post_json(url: &str, body: &serde_json::Value) -> RainyResult<String> {
    if !url.starts_with("https://") && !is_loopback_http(url) {
        return Err(RainyError::plugin(
            "PLUGIN_ADAPTER_UNSUPPORTED_URL",
            format!("only HTTPS or loopback HTTP adapters are allowed: {url}"),
        ));
    }
    let body = serde_json::to_string(body)?;
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(3))
        .timeout_read(Duration::from_secs(10))
        .timeout_write(Duration::from_secs(10))
        .redirects(0)
        .build();
    let response = agent
        .post(url)
        .set("User-Agent", "rainy-cli")
        .set("Content-Type", "application/json")
        .send_string(&body)
        .map_err(|err| {
            RainyError::plugin(
                "PLUGIN_ADAPTER_FAILED",
                format!("adapter request failed: {err}"),
            )
        })?;
    let mut response_body = String::new();
    response
        .into_reader()
        .take(MAX_PLUGIN_RESPONSE_BYTES + 1)
        .read_to_string(&mut response_body)
        .map_err(|err| {
            RainyError::plugin(
                "PLUGIN_ADAPTER_FAILED",
                format!("adapter response read failed: {err}"),
            )
        })?;
    if response_body.len() as u64 > MAX_PLUGIN_RESPONSE_BYTES {
        return Err(RainyError::plugin(
            "PLUGIN_ADAPTER_RESPONSE_TOO_LARGE",
            "plugin adapter response exceeds 5 MiB limit",
        ));
    }
    Ok(response_body)
}

fn is_loopback_http(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("http://") else {
        return false;
    };
    let host = rest
        .split_once('/')
        .map(|(host, _)| host)
        .unwrap_or(rest)
        .split(':')
        .next()
        .unwrap_or_default();
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}

pub(crate) fn validate_plugin_permissions(manifest: &PluginManifest) -> RainyResult<()> {
    let permissions = manifest.permissions.as_object().ok_or_else(|| {
        RainyError::plugin(
            "PLUGIN_MANIFEST_INVALID",
            "plugin permissions must be an object",
        )
    })?;
    if !permissions.contains_key("fs")
        || !permissions.contains_key("network")
        || !permissions.contains_key("secrets")
    {
        return Err(RainyError::plugin(
            "PLUGIN_MANIFEST_INVALID",
            "plugin permissions must declare fs, network, and secrets",
        ));
    }
    let fs = permissions
        .get("fs")
        .and_then(|value| value.as_object())
        .ok_or_else(|| {
            RainyError::plugin(
                "PLUGIN_MANIFEST_INVALID",
                "plugin fs permissions must be an object",
            )
        })?;
    require_string_array(fs.get("read"), "permissions.fs.read")?;
    require_string_array(fs.get("write"), "permissions.fs.write")?;
    Ok(())
}

fn require_string_array(value: Option<&serde_json::Value>, path: &str) -> RainyResult<()> {
    let Some(value) = value else {
        return Err(RainyError::plugin(
            "PLUGIN_MANIFEST_INVALID",
            format!("{path} must be declared"),
        ));
    };
    let Some(items) = value.as_array() else {
        return Err(RainyError::plugin(
            "PLUGIN_MANIFEST_INVALID",
            format!("{path} must be an array"),
        ));
    };
    if items.iter().any(|item| !item.is_string()) {
        return Err(RainyError::plugin(
            "PLUGIN_MANIFEST_INVALID",
            format!("{path} must contain only strings"),
        ));
    }
    Ok(())
}

fn check_plugin_change_permissions(
    manifest: &PluginManifest,
    changes: &ChangeSet,
) -> RainyResult<()> {
    let patterns = plugin_write_patterns(manifest)?;
    let allow = compile_plugin_globs(&patterns)?;
    for change in &changes.changes {
        if change.noop {
            continue;
        }
        validate_relative_change_path(&change.path)?;
        if patterns.is_empty() || !allow.is_match(&change.path) {
            return Err(RainyError::plugin(
                "PLUGIN_FS_WRITE_DENIED",
                format!(
                    "plugin {} is not allowed to write {}",
                    manifest.name, change.path
                ),
            ));
        }
    }
    Ok(())
}

fn plugin_write_patterns(manifest: &PluginManifest) -> RainyResult<Vec<String>> {
    let permissions = manifest.permissions.as_object().ok_or_else(|| {
        RainyError::plugin(
            "PLUGIN_MANIFEST_INVALID",
            "plugin permissions must be an object",
        )
    })?;
    let write = permissions
        .get("fs")
        .and_then(|value| value.get("write"))
        .and_then(|value| value.as_array())
        .ok_or_else(|| {
            RainyError::plugin(
                "PLUGIN_MANIFEST_INVALID",
                "permissions.fs.write must be an array",
            )
        })?;
    Ok(write
        .iter()
        .filter_map(|value| value.as_str().map(str::to_string))
        .collect())
}

fn compile_plugin_globs(patterns: &[String]) -> RainyResult<globset::GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(Glob::new(pattern).map_err(|err| {
            RainyError::plugin(
                "PLUGIN_MANIFEST_INVALID",
                format!("invalid permissions.fs.write glob {pattern}: {err}"),
            )
        })?);
    }
    builder.build().map_err(|err| {
        RainyError::plugin(
            "PLUGIN_MANIFEST_INVALID",
            format!("invalid permissions.fs.write globs: {err}"),
        )
    })
}

fn validate_relative_change_path(raw: &str) -> RainyResult<()> {
    let path = Path::new(raw);
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err(RainyError::plugin(
            "PLUGIN_RESPONSE_INVALID",
            format!("plugin change path must be relative: {raw}"),
        ));
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            _ => {
                return Err(RainyError::plugin(
                    "PLUGIN_RESPONSE_INVALID",
                    format!("plugin change path cannot escape workspace: {raw}"),
                ));
            }
        }
    }
    Ok(())
}

pub(crate) fn validate_plugin_actions(manifest: &PluginManifest) -> RainyResult<()> {
    for action in &manifest.actions {
        if let Some(runtime) = action.runtime.as_deref()
            && runtime != "wasm"
        {
            return Err(RainyError::plugin(
                "PLUGIN_MANIFEST_INVALID",
                format!("unsupported action runtime for {}: {runtime}", action.id),
            ));
        }
        if action.runtime.as_deref() == Some("wasm") && action.wasm.is_none() {
            return Err(RainyError::plugin(
                "PLUGIN_MANIFEST_INVALID",
                format!("wasm action {} must declare a wasm module", action.id),
            ));
        }
        if let Some(wasm) = action.wasm.as_deref() {
            validate_relative_asset_path(wasm)?;
        }
    }
    Ok(())
}

#[derive(Debug)]
struct WasmAsset {
    source: PathBuf,
    target_rel: PathBuf,
}

fn collect_wasm_assets(
    manifest: &PluginManifest,
    manifest_path: &Path,
) -> RainyResult<Vec<WasmAsset>> {
    let source_dir = manifest_path.parent().ok_or_else(|| {
        RainyError::plugin(
            "PLUGIN_MANIFEST_INVALID",
            "plugin manifest has no parent directory",
        )
    })?;
    let mut assets = BTreeMap::new();
    for action in &manifest.actions {
        let Some(wasm) = action.wasm.as_deref() else {
            continue;
        };
        let rel = validate_relative_asset_path(wasm)?;
        let source = source_dir.join(&rel);
        if !source.is_file() {
            return Err(RainyError::plugin(
                "PLUGIN_WASM_NOT_FOUND",
                format!(
                    "wasm module not found for action {}: {}",
                    action.id,
                    source.display()
                ),
            ));
        }
        let target_rel = PathBuf::from(".rainy/plugins/wasm")
            .join(&manifest.name)
            .join(&rel);
        assets.insert(target_rel.clone(), WasmAsset { source, target_rel });
    }
    Ok(assets.into_values().collect())
}

fn validate_relative_asset_path(raw: &str) -> RainyResult<PathBuf> {
    let path = Path::new(raw);
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err(RainyError::plugin(
            "PLUGIN_MANIFEST_INVALID",
            format!("plugin asset path must be relative: {raw}"),
        ));
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            _ => {
                return Err(RainyError::plugin(
                    "PLUGIN_MANIFEST_INVALID",
                    format!("plugin asset path cannot escape plugin directory: {raw}"),
                ));
            }
        }
    }
    Ok(path.to_path_buf())
}

fn ensure_plugin_command_allowed(manifest: &PluginManifest, args: &[OsString]) -> RainyResult<()> {
    let Some(command) = args.first().and_then(|arg| arg.to_str()) else {
        return Ok(());
    };
    let command_line = std::iter::once(command)
        .chain(args.iter().skip(1).filter_map(|arg| arg.to_str()))
        .collect::<Vec<_>>()
        .join(" ");
    let allowed = manifest.commands.iter().any(|item| {
        command_line == item.name
            || command_line.starts_with(&format!("{} ", item.name))
            || item.name == command
    });
    if !allowed && !manifest.commands.is_empty() {
        return Err(RainyError::plugin(
            "PLUGIN_PERMISSION_DENIED",
            format!("plugin command is not declared in manifest: {command_line}"),
        ));
    }
    Ok(())
}

fn plugin_network_permission(manifest: Option<&PluginManifest>) -> String {
    let Some(manifest) = manifest else {
        return "none".to_string();
    };
    manifest
        .permissions
        .get("network")
        .and_then(|value| value.as_str())
        .unwrap_or("none")
        .to_string()
}

pub(crate) fn shadows_builtin_command(plugin_name: &str) -> bool {
    let Some(command) = plugin_name.strip_prefix("rainy-") else {
        return false;
    };
    matches!(
        command,
        "init"
            | "new"
            | "add"
            | "apply"
            | "capability"
            | "pack"
            | "doctor"
            | "verify"
            | "evidence"
            | "plugin"
            | "agent"
            | "skill"
            | "conformance"
            | "schema"
    )
}

fn prepare_plugin_source(workspace: &Path, source: &str, apply: bool) -> RainyResult<PathBuf> {
    if let Some(git_url) = source.strip_prefix("git+") {
        if !git_url.starts_with("https://")
            && !git_url.starts_with("ssh://")
            && !git_url.starts_with("git@")
        {
            return Err(RainyError::plugin(
                "PLUGIN_SOURCE_UNSUPPORTED_URL",
                format!("git plugin source must use HTTPS or SSH: {git_url}"),
            ));
        }
        let target = workspace.join(".rainy/plugins/src").join(slugify(git_url));
        if apply && !target.exists() {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let output = std::process::Command::new("git")
                .arg("clone")
                .arg(git_url)
                .arg(&target)
                .output()?;
            if !output.status.success() {
                return Err(RainyError::plugin(
                    "PLUGIN_INSTALL_FAILED",
                    String::from_utf8_lossy(&output.stderr).to_string(),
                ));
            }
        }
        return Ok(target);
    }

    let path = PathBuf::from(source);
    let path = if path.is_absolute() {
        path
    } else {
        workspace.join(path)
    };
    path.canonicalize().map_err(|err| {
        RainyError::plugin(
            "PLUGIN_SOURCE_NOT_FOUND",
            format!("plugin source not found: {} ({err})", path.display()),
        )
    })
}

fn plugin_files(source: &Path) -> RainyResult<Vec<PathBuf>> {
    if source.is_file() {
        let name = source
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        return Ok(if name.starts_with("rainy-") {
            vec![source.to_path_buf()]
        } else {
            Vec::new()
        });
    }

    let mut files = Vec::new();
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("rainy-") {
            files.push(entry.path());
        }
    }
    files.sort();
    Ok(files)
}

fn manifest_file(source: &Path) -> Option<PathBuf> {
    if source.is_file() {
        return None;
    }
    let direct = source.join("plugin.json");
    if direct.exists() {
        return Some(direct);
    }
    let nested = source.join(".rainy-plugin/plugin.json");
    if nested.exists() {
        return Some(nested);
    }
    None
}

fn load_manifest(workspace: &Path, id: &str) -> RainyResult<PluginManifest> {
    let path = workspace
        .join(".rainy/plugins/manifests")
        .join(format!("{id}.json"));
    let content = std::fs::read_to_string(path)?;
    serde_json::from_str(&content).map_err(|err| {
        RainyError::plugin(
            "PLUGIN_MANIFEST_INVALID",
            format!("invalid plugin manifest: {err}"),
        )
    })
}

fn slugify(input: &str) -> String {
    input
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}
