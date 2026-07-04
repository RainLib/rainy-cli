use crate::cli::{PackCommand, PackSubcommand};
use crate::config::{self, ProjectConfig, RegistrySourceConfig};
use crate::error::{RainyError, RainyResult};
use crate::output::CommandOutput;
use crate::patch::{self, ChangeSet};
use crate::policy;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityPack {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    pub metadata: PackMetadata,
    #[serde(default)]
    pub requires: BTreeMap<String, String>,
    #[serde(default)]
    pub exports: PackExports,
    #[serde(skip)]
    pub root: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackMetadata {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PackExports {
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub validators: Vec<String>,
    #[serde(default)]
    pub skills: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityDefinition {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(rename = "dependsOn", default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub providers: Vec<CapabilityProvider>,
    #[serde(default)]
    pub inputs: BTreeMap<String, CapabilityInput>,
    pub actions: CapabilityActions,
    #[serde(default)]
    pub validations: Vec<ValidationCommand>,
    #[serde(default)]
    pub doctor: CapabilityDoctor,
    #[serde(rename = "agentRules", default)]
    pub agent_rules: Vec<String>,
    #[serde(default)]
    pub policy: config::PolicySection,
    #[serde(skip)]
    pub pack_root: PathBuf,
    #[serde(skip)]
    pub pack_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityProvider {
    pub id: String,
    #[serde(default)]
    pub default: bool,
    #[serde(rename = "requiredConfig", default)]
    pub required_config: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityInput {
    #[serde(rename = "type")]
    pub input_type: String,
    #[serde(default)]
    pub default: Option<serde_yaml::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityActions {
    #[serde(default)]
    pub install: Vec<ActionSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionSpec {
    pub id: String,
    pub uses: String,
    #[serde(rename = "with", default)]
    pub with_value: serde_yaml::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationCommand {
    pub id: String,
    pub command: String,
    #[serde(rename = "workingDirectory", default)]
    pub working_directory: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapabilityDoctor {
    #[serde(default)]
    pub checks: Vec<DoctorCheckSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorCheckSpec {
    pub id: String,
    pub uses: String,
    #[serde(rename = "with", default)]
    pub with_value: serde_yaml::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct CapabilitySummary {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub pack: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CapabilityInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub depends_on: Vec<String>,
    pub providers: Vec<String>,
    pub actions: Vec<String>,
    pub pack: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PackInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpRegistryIndex {
    pub protocol_version: String,
    #[serde(default)]
    pub packs: Vec<HttpRegistryPack>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpRegistryPack {
    pub name: String,
    pub version: String,
    pub base_url: String,
    #[serde(default)]
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackSignature {
    pub protocol_version: String,
    pub algorithm: String,
    pub digest: String,
    pub signed_at: String,
    pub files: Vec<PackFileDigest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackFileDigest {
    pub path: String,
    pub digest: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CapabilityGraph {
    pub nodes: Vec<String>,
    pub edges: Vec<CapabilityEdge>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CapabilityEdge {
    pub from: String,
    pub to: String,
}

pub fn capability_list(workspace: &Path) -> RainyResult<CommandOutput> {
    let registry = RegistryClient::load(workspace)?;
    let capabilities = registry
        .capabilities()
        .into_iter()
        .map(|capability| CapabilitySummary {
            id: capability.id,
            name: capability.name,
            version: capability.version,
            description: capability.description,
            pack: capability.pack_name,
        })
        .collect();
    Ok(CommandOutput::Capabilities { capabilities })
}

pub fn capability_explain(workspace: &Path, id: &str) -> RainyResult<CommandOutput> {
    let registry = RegistryClient::load(workspace)?;
    let capability = registry.get_capability(id)?;
    let info = CapabilityInfo {
        id: capability.id,
        name: capability.name,
        version: capability.version,
        description: capability.description,
        depends_on: capability.depends_on,
        providers: capability
            .providers
            .into_iter()
            .map(|provider| provider.id)
            .collect(),
        actions: capability
            .actions
            .install
            .into_iter()
            .map(|action| format!("{} ({})", action.id, action.uses))
            .collect(),
        pack: capability.pack_name,
    };
    Ok(CommandOutput::Capability { capability: info })
}

pub fn capability_graph(workspace: &Path) -> RainyResult<CommandOutput> {
    let registry = RegistryClient::load(workspace)?;
    let capabilities = registry.capabilities();
    let mut nodes = BTreeSet::new();
    let mut edges = Vec::new();
    for capability in capabilities {
        nodes.insert(capability.id.clone());
        for dep in capability.depends_on {
            nodes.insert(dep.clone());
            edges.push(CapabilityEdge {
                from: capability.id.clone(),
                to: dep,
            });
        }
    }
    Ok(CommandOutput::CapabilityGraph {
        graph: CapabilityGraph {
            nodes: nodes.into_iter().collect(),
            edges,
        },
    })
}

pub fn handle_pack_command(workspace: &Path, command: PackCommand) -> RainyResult<CommandOutput> {
    match command.command {
        PackSubcommand::List => {
            let registry = RegistryClient::load(workspace)?;
            let packs = registry
                .packs
                .into_iter()
                .map(|pack| PackInfo {
                    name: pack.metadata.name,
                    version: pack.metadata.version,
                    description: pack.metadata.description.unwrap_or_default(),
                    path: pack.root.to_string_lossy().to_string(),
                })
                .collect();
            Ok(CommandOutput::Packs { packs })
        }
        PackSubcommand::Inspect { id } => {
            let registry = RegistryClient::load(workspace)?;
            let packs = registry
                .packs
                .into_iter()
                .filter(|pack| pack.metadata.name == id)
                .map(|pack| PackInfo {
                    name: pack.metadata.name,
                    version: pack.metadata.version,
                    description: pack.metadata.description.unwrap_or_default(),
                    path: pack.root.to_string_lossy().to_string(),
                })
                .collect::<Vec<_>>();
            if packs.is_empty() {
                return Err(RainyError::registry(
                    "PACK_NOT_FOUND",
                    format!("pack not found: {id}"),
                ));
            }
            Ok(CommandOutput::Packs { packs })
        }
        PackSubcommand::Install(args) => install_pack(workspace, args),
        PackSubcommand::Update(args) => update_packs(workspace, args),
        PackSubcommand::Sign(args) => sign_pack(&args.path),
        PackSubcommand::Verify(args) => verify_pack_signature(&args.path),
    }
}

#[derive(Debug, Clone)]
pub struct RegistryClient {
    packs: Vec<CapabilityPack>,
    capabilities: BTreeMap<String, CapabilityDefinition>,
}

impl RegistryClient {
    pub fn load(workspace: &Path) -> RainyResult<Self> {
        let config = config::load_config(workspace).ok();
        let sources = registry_sources(workspace, config.as_ref());
        let mut packs = Vec::new();
        let mut capabilities = BTreeMap::new();

        for source in sources {
            if source.exists() {
                load_packs_from_dir(&source, &mut packs, &mut capabilities)?;
            }
        }

        if capabilities.is_empty() {
            return Err(RainyError::registry(
                "REGISTRY_EMPTY",
                "no local capability packs found",
            ));
        }

        Ok(Self {
            packs,
            capabilities,
        })
    }

    pub fn get_capability(&self, id: &str) -> RainyResult<CapabilityDefinition> {
        self.capabilities.get(id).cloned().ok_or_else(|| {
            RainyError::registry(
                "CAPABILITY_NOT_FOUND",
                format!("capability not found: {id}"),
            )
        })
    }

    pub fn capabilities(&self) -> Vec<CapabilityDefinition> {
        self.capabilities.values().cloned().collect()
    }
}

fn registry_sources(workspace: &Path, config: Option<&ProjectConfig>) -> Vec<PathBuf> {
    let mut sources = Vec::new();
    if let Some(config) = config {
        for source in &config.capability_registry.sources {
            if let RegistrySourceConfig::Local { path } = source {
                let path = PathBuf::from(path);
                if path.is_absolute() {
                    sources.push(path);
                } else {
                    sources.push(workspace.join(path));
                }
            } else if let RegistrySourceConfig::Git { url, .. } = source {
                sources.push(workspace.join(".rainy/packs").join(slugify(url)));
            } else if let RegistrySourceConfig::Http { url } = source {
                sources.push(http_cache_path(workspace, url));
            }
        }
    }
    sources.push(config::default_registry_path());
    sources.sort();
    sources.dedup();
    sources
}

fn load_packs_from_dir(
    source: &Path,
    packs: &mut Vec<CapabilityPack>,
    capabilities: &mut BTreeMap<String, CapabilityDefinition>,
) -> RainyResult<()> {
    if source.join("pack.yaml").exists() {
        load_pack_at(source, packs, capabilities)?;
        return Ok(());
    }

    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let root = entry.path();
        if !root.join("pack.yaml").exists() {
            continue;
        }
        load_pack_at(&root, packs, capabilities)?;
    }
    Ok(())
}

fn load_pack_at(
    root: &Path,
    packs: &mut Vec<CapabilityPack>,
    capabilities: &mut BTreeMap<String, CapabilityDefinition>,
) -> RainyResult<()> {
    let pack_path = root.join("pack.yaml");
    let content = std::fs::read_to_string(&pack_path)?;
    let mut pack: CapabilityPack = serde_yaml::from_str(&content)?;
    pack.root = root.to_path_buf();
    for capability_path in &pack.exports.capabilities {
        let path = root.join(capability_path);
        let content = std::fs::read_to_string(&path)?;
        let mut capability: CapabilityDefinition = serde_yaml::from_str(&content)?;
        capability.pack_root = root.to_path_buf();
        capability.pack_name = pack.metadata.name.clone();
        if capabilities.contains_key(&capability.id) {
            return Err(RainyError::registry(
                "CAPABILITY_DUPLICATE",
                format!("duplicate capability id: {}", capability.id),
            ));
        }
        capabilities.insert(capability.id.clone(), capability);
    }
    packs.push(pack);
    Ok(())
}

fn install_pack(workspace: &Path, args: crate::cli::PackInstallArgs) -> RainyResult<CommandOutput> {
    let apply = resolve_apply_flags(args.dry_run, args.apply)?;
    let mut config = config::load_config(workspace)?;
    if let Some(url) = args.source.strip_prefix("http+") {
        if apply {
            sync_http_registry(workspace, url)?;
        }
        let already_present = config
            .capability_registry
            .sources
            .iter()
            .any(|source| matches!(source, RegistrySourceConfig::Http { url: existing } if existing == url));
        if !already_present {
            config
                .capability_registry
                .sources
                .push(RegistrySourceConfig::Http {
                    url: url.to_string(),
                });
        }
        let mut changes = ChangeSet::new();
        changes.push(patch::change_for_file(
            workspace,
            "rainy.yaml",
            config::serialize_config(&config)?,
            "install HTTP capability registry source",
        )?);
        if apply {
            policy::check_changes(workspace, &changes)?;
            patch::apply_changes(workspace, &changes)?;
            Ok(CommandOutput::change_applied("pack install", changes))
        } else {
            Ok(CommandOutput::change_dry_run("pack install", changes))
        }
    } else {
        let source_path = prepare_pack_source(workspace, &args.source, apply)?;
        validate_pack_source(&source_path)?;
        let source_text = source_path.to_string_lossy().to_string();

        let already_present = config
        .capability_registry
        .sources
        .iter()
        .any(|source| matches!(source, RegistrySourceConfig::Local { path } if normalize_path(workspace, path) == source_path));
        if !already_present {
            config
                .capability_registry
                .sources
                .push(RegistrySourceConfig::Local { path: source_text });
        }

        let mut changes = ChangeSet::new();
        changes.push(patch::change_for_file(
            workspace,
            "rainy.yaml",
            config::serialize_config(&config)?,
            "install capability pack source",
        )?);

        if apply {
            policy::check_changes(workspace, &changes)?;
            patch::apply_changes(workspace, &changes)?;
            Ok(CommandOutput::change_applied("pack install", changes))
        } else {
            Ok(CommandOutput::change_dry_run("pack install", changes))
        }
    }
}

fn resolve_apply_flags(dry_run: bool, apply: bool) -> RainyResult<bool> {
    if dry_run && apply {
        return Err(RainyError::registry(
            "APPLY_MODE_CONFLICT",
            "--dry-run and --apply cannot be used together",
        ));
    }
    Ok(apply)
}

fn update_packs(workspace: &Path, args: crate::cli::PackUpdateArgs) -> RainyResult<CommandOutput> {
    let apply = resolve_apply_flags(args.dry_run, args.apply)?;
    if !apply {
        return Ok(CommandOutput::change_dry_run(
            "pack update",
            ChangeSet::new(),
        ));
    }
    let config = config::load_config(workspace)?;
    let mut updated = Vec::new();
    for source in config.capability_registry.sources {
        let RegistrySourceConfig::Local { path } = source else {
            if let RegistrySourceConfig::Http { url } = source {
                sync_http_registry(workspace, &url)?;
                updated.push(url);
            }
            continue;
        };
        let path = normalize_path(workspace, &path);
        if path.join(".git").exists() {
            let output = std::process::Command::new("git")
                .arg("-C")
                .arg(&path)
                .arg("pull")
                .arg("--ff-only")
                .output()?;
            if !output.status.success() {
                return Err(RainyError::registry(
                    "PACK_UPDATE_FAILED",
                    String::from_utf8_lossy(&output.stderr).to_string(),
                ));
            }
            updated.push(path.to_string_lossy().to_string());
        }
    }
    if updated.is_empty() {
        Ok(CommandOutput::message("Pack registry is up to date."))
    } else {
        Ok(CommandOutput::message(format!(
            "Updated pack sources:\n{}",
            updated.join("\n")
        )))
    }
}

fn sign_pack(path: &Path) -> RainyResult<CommandOutput> {
    validate_pack_source(path)?;
    let signature = calculate_pack_signature(path)?;
    let signature_path = path.join(".rainy-pack-signature.json");
    std::fs::write(
        &signature_path,
        format!("{}\n", serde_json::to_string_pretty(&signature)?),
    )?;
    Ok(CommandOutput::message(format!(
        "Signed pack {} with sha256 {}",
        path.display(),
        signature.digest
    )))
}

fn verify_pack_signature(path: &Path) -> RainyResult<CommandOutput> {
    let signature_path = path.join(".rainy-pack-signature.json");
    if !signature_path.exists() {
        return Err(RainyError::registry(
            "PACK_SIGNATURE_NOT_FOUND",
            format!("signature not found: {}", signature_path.display()),
        ));
    }
    let expected: PackSignature = serde_json::from_str(&std::fs::read_to_string(&signature_path)?)?;
    let actual = calculate_pack_signature(path)?;
    if expected.digest != actual.digest || expected.files.len() != actual.files.len() {
        return Err(RainyError::registry(
            "PACK_SIGNATURE_INVALID",
            format!(
                "pack signature mismatch: expected {}, actual {}",
                expected.digest, actual.digest
            ),
        ));
    }
    for (expected_file, actual_file) in expected.files.iter().zip(actual.files.iter()) {
        if expected_file.path != actual_file.path || expected_file.digest != actual_file.digest {
            return Err(RainyError::registry(
                "PACK_SIGNATURE_INVALID",
                format!("pack signature mismatch at {}", expected_file.path),
            ));
        }
    }
    Ok(CommandOutput::message(format!(
        "Pack signature verified: {}",
        expected.digest
    )))
}

fn calculate_pack_signature(path: &Path) -> RainyResult<PackSignature> {
    let mut files = Vec::new();
    collect_pack_files(path, path, &mut files)?;
    files.sort_by(|left, right| left.path.cmp(&right.path));
    let mut root = Sha256::new();
    for file in &files {
        root.update(file.path.as_bytes());
        root.update(b"\0");
        root.update(file.digest.as_bytes());
        root.update(b"\0");
    }
    Ok(PackSignature {
        protocol_version: "rainy.pack-signature.v1".to_string(),
        algorithm: "sha256".to_string(),
        digest: hex(&root.finalize()),
        signed_at: chrono::Utc::now().to_rfc3339(),
        files,
    })
}

fn collect_pack_files(
    root: &Path,
    current: &Path,
    files: &mut Vec<PackFileDigest>,
) -> RainyResult<()> {
    for entry in std::fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            collect_pack_files(root, &path, files)?;
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if rel == ".rainy-pack-signature.json" {
            continue;
        }
        let bytes = std::fs::read(&path)?;
        let mut digest = Sha256::new();
        digest.update(&bytes);
        files.push(PackFileDigest {
            path: rel,
            digest: hex(&digest.finalize()),
        });
    }
    Ok(())
}

fn sync_http_registry(workspace: &Path, url: &str) -> RainyResult<()> {
    let index_text = http_get(url)?;
    let index: HttpRegistryIndex = serde_yaml::from_str(&index_text)?;
    if index.protocol_version != "rainy.registry.v1" {
        return Err(RainyError::registry(
            "HTTP_REGISTRY_INVALID",
            format!("unsupported registry protocol: {}", index.protocol_version),
        ));
    }
    let cache_root = http_cache_path(workspace, url);
    if cache_root.exists() {
        std::fs::remove_dir_all(&cache_root)?;
    }
    std::fs::create_dir_all(&cache_root)?;
    for pack in index.packs {
        let pack_root = cache_root.join(&pack.name);
        std::fs::create_dir_all(&pack_root)?;
        for file in pack.files {
            validate_relative_registry_file(&file)?;
            let file_url = join_url(&pack.base_url, &file);
            let content = http_get(&file_url)?;
            let target = pack_root.join(&file);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(target, content)?;
        }
    }
    Ok(())
}

fn http_get(url: &str) -> RainyResult<String> {
    if let Some(rest) = url.strip_prefix("http://") {
        return http_get_plain(rest);
    }
    if url.starts_with("https://") {
        let output = std::process::Command::new("curl")
            .arg("-fsSL")
            .arg(url)
            .output()?;
        if !output.status.success() {
            return Err(RainyError::registry(
                "HTTP_REGISTRY_FETCH_FAILED",
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }
        return String::from_utf8(output.stdout)
            .map_err(|err| RainyError::registry("HTTP_REGISTRY_FETCH_FAILED", err.to_string()));
    }
    Err(RainyError::registry(
        "HTTP_REGISTRY_UNSUPPORTED_URL",
        format!("unsupported URL: {url}"),
    ))
}

fn http_get_plain(rest: &str) -> RainyResult<String> {
    let (host_port, path) = rest.split_once('/').unwrap_or((rest, ""));
    let path = format!("/{path}");
    let mut stream = TcpStream::connect(host_port).map_err(|err| {
        RainyError::registry(
            "HTTP_REGISTRY_FETCH_FAILED",
            format!("connect {host_port} failed: {err}"),
        )
    })?;
    let host = host_port.split(':').next().unwrap_or(host_port);
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nUser-Agent: rainy-cli\r\n\r\n"
    );
    stream.write_all(request.as_bytes())?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response)?;
    let response = String::from_utf8(response)
        .map_err(|err| RainyError::registry("HTTP_REGISTRY_FETCH_FAILED", err.to_string()))?;
    let (head, body) = response.split_once("\r\n\r\n").ok_or_else(|| {
        RainyError::registry("HTTP_REGISTRY_FETCH_FAILED", "invalid HTTP response")
    })?;
    if !head.starts_with("HTTP/1.1 200") && !head.starts_with("HTTP/1.0 200") {
        return Err(RainyError::registry(
            "HTTP_REGISTRY_FETCH_FAILED",
            head.lines().next().unwrap_or("HTTP error").to_string(),
        ));
    }
    Ok(body.to_string())
}

fn http_cache_path(workspace: &Path, url: &str) -> PathBuf {
    workspace
        .join(".rainy/packs/http")
        .join(slugify(url).trim_matches('-'))
}

fn validate_relative_registry_file(path: &str) -> RainyResult<()> {
    let path = Path::new(path);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(RainyError::registry(
            "HTTP_REGISTRY_INVALID",
            format!("registry file path is unsafe: {}", path.display()),
        ));
    }
    Ok(())
}

fn join_url(base: &str, file: &str) -> String {
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        file.trim_start_matches('/')
    )
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn prepare_pack_source(workspace: &Path, source: &str, apply: bool) -> RainyResult<PathBuf> {
    if let Some(git_url) = source.strip_prefix("git+") {
        let target = workspace.join(".rainy/packs").join(slugify(git_url));
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
                return Err(RainyError::registry(
                    "PACK_INSTALL_FAILED",
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
        RainyError::registry(
            "PACK_SOURCE_NOT_FOUND",
            format!("pack source not found: {} ({err})", path.display()),
        )
    })
}

fn validate_pack_source(source: &Path) -> RainyResult<()> {
    if source.join("pack.yaml").exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(source).map_err(|err| {
        RainyError::registry(
            "PACK_SOURCE_INVALID",
            format!("cannot read pack source {}: {err}", source.display()),
        )
    })? {
        let entry = entry?;
        if entry.path().join("pack.yaml").exists() {
            return Ok(());
        }
    }
    Err(RainyError::registry(
        "PACK_SOURCE_INVALID",
        format!("no pack.yaml found in {}", source.display()),
    ))
}

fn normalize_path(workspace: &Path, path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else {
        workspace.join(path)
    }
}

fn slugify(input: &str) -> String {
    input
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}
