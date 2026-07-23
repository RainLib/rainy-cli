use crate::error::{RainyError, RainyResult};
use crate::output::CommandOutput;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    pub project: ProjectSection,
    #[serde(default)]
    pub stack: BTreeMap<String, serde_yaml::Value>,
    pub paths: PathSection,
    pub package: PackageSection,
    #[serde(rename = "capabilityRegistry", default)]
    pub capability_registry: CapabilityRegistrySection,
    #[serde(default)]
    pub policy: PolicySection,
    #[serde(default)]
    pub verify: VerifySection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSection {
    pub name: String,
    #[serde(rename = "type", default)]
    pub project_type: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathSection {
    pub backend: String,
    pub frontend: String,
    #[serde(default = "default_generated")]
    pub generated: String,
    #[serde(default = "default_evidence")]
    pub evidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageSection {
    pub java: String,
    #[serde(rename = "npmScope", default)]
    pub npm_scope: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapabilityRegistrySection {
    #[serde(default)]
    pub sources: Vec<RegistrySourceConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum RegistrySourceConfig {
    Local {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "is_zero")]
        priority: i32,
        path: String,
    },
    Git {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "is_zero")]
        priority: i32,
        url: String,
        #[serde(rename = "ref", default)]
        reference: Option<String>,
    },
    Http {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "is_zero")]
        priority: i32,
        url: String,
    },
    Archive {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "is_zero")]
        priority: i32,
        url: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sha256: Option<String>,
    },
}

fn is_zero(value: &i32) -> bool {
    *value == 0
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PolicySection {
    #[serde(rename = "allowEdit", default)]
    pub allow_edit: Vec<String>,
    #[serde(rename = "denyEdit", default)]
    pub deny_edit: Vec<String>,
    #[serde(rename = "requireApproval", default)]
    pub require_approval: Vec<String>,
    #[serde(rename = "allowNativePlugins", default)]
    pub allow_native_plugins: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VerifySection {
    #[serde(default)]
    pub profiles: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityLock {
    #[serde(rename = "lockfileVersion")]
    pub lockfile_version: u32,
    pub project: LockProject,
    pub rainy: LockRainy,
    #[serde(default)]
    pub capabilities: BTreeMap<String, LockedCapability>,
    #[serde(default)]
    pub skills: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockProject {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockRainy {
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedCapability {
    pub version: String,
    #[serde(default)]
    pub provider: Option<String>,
    pub pack: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    #[serde(rename = "installedAt")]
    pub installed_at: DateTime<Utc>,
    #[serde(default)]
    pub artifacts: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstalledCapability {
    pub id: String,
    pub version: String,
    pub provider: Option<String>,
    pub pack: String,
    pub source: Option<String>,
    pub digest: Option<String>,
    pub artifacts: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryLock {
    #[serde(default = "registry_lock_version")]
    pub lockfile_version: u32,
    #[serde(default)]
    pub registries: BTreeMap<String, LockedRegistry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockedRegistry {
    #[serde(rename = "type")]
    pub source_type: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_ref: Option<String>,
    pub digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_path: Option<String>,
    #[serde(default)]
    pub all_modules: bool,
    #[serde(default)]
    pub modules: Vec<String>,
    #[serde(default)]
    pub installed_skills: Vec<InstalledRegistrySkill>,
    pub synced_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledRegistrySkill {
    pub id: String,
    pub target: String,
    pub path: String,
    pub digest: String,
}

fn registry_lock_version() -> u32 {
    1
}

impl RegistrySourceConfig {
    pub fn configured_name(&self) -> Option<&str> {
        match self {
            Self::Local { name, .. }
            | Self::Git { name, .. }
            | Self::Http { name, .. }
            | Self::Archive { name, .. } => name.as_deref(),
        }
    }

    pub fn priority(&self) -> i32 {
        match self {
            Self::Local { priority, .. }
            | Self::Git { priority, .. }
            | Self::Http { priority, .. }
            | Self::Archive { priority, .. } => *priority,
        }
    }
}

pub fn load_config(workspace: &Path) -> RainyResult<ProjectConfig> {
    let path = workspace.join("rainy.yaml");
    if !path.exists() {
        return Err(RainyError::config(
            "CONFIG_NOT_FOUND",
            format!("rainy.yaml not found in {}", workspace.display()),
        ));
    }
    let content = std::fs::read_to_string(&path)?;
    let config: ProjectConfig = serde_yaml::from_str(&content)?;
    if config.project.name.trim().is_empty() {
        return Err(RainyError::config(
            "CONFIG_INVALID",
            "project.name must not be empty",
        ));
    }
    Ok(config)
}

pub fn serialize_config(config: &ProjectConfig) -> RainyResult<String> {
    Ok(serde_yaml::to_string(config)?)
}

pub fn load_lock(workspace: &Path) -> RainyResult<CapabilityLock> {
    let path = workspace.join("capability.lock");
    if !path.exists() {
        return Err(RainyError::config(
            "LOCK_NOT_FOUND",
            format!("capability.lock not found in {}", workspace.display()),
        ));
    }
    let content = std::fs::read_to_string(&path)?;
    Ok(serde_yaml::from_str(&content)?)
}

pub fn load_registry_lock(workspace: &Path) -> RainyResult<RegistryLock> {
    let path = workspace.join(".rainy/registry.lock");
    if !path.exists() {
        return Ok(RegistryLock {
            lockfile_version: registry_lock_version(),
            registries: BTreeMap::new(),
        });
    }
    let content = std::fs::read_to_string(path)?;
    Ok(serde_yaml::from_str(&content)?)
}

pub fn save_registry_lock_content(lock: &RegistryLock) -> RainyResult<String> {
    Ok(serde_yaml::to_string(lock)?)
}

pub fn save_lock_content(lock: &CapabilityLock) -> RainyResult<String> {
    Ok(serde_yaml::to_string(lock)?)
}

pub fn empty_lock(project_name: &str) -> CapabilityLock {
    CapabilityLock {
        lockfile_version: 1,
        project: LockProject {
            name: project_name.to_string(),
        },
        rainy: LockRainy {
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        capabilities: BTreeMap::new(),
        skills: Vec::new(),
    }
}

pub fn capability_installed(workspace: &Path) -> RainyResult<CommandOutput> {
    let lock = load_lock(workspace)?;
    let capabilities = lock
        .capabilities
        .into_iter()
        .map(|(id, cap)| InstalledCapability {
            id,
            version: cap.version,
            provider: cap.provider,
            pack: cap.pack,
            source: cap.source,
            digest: cap.digest,
            artifacts: cap.artifacts,
        })
        .collect();
    Ok(CommandOutput::Installed { capabilities })
}

pub fn package_path(config: &ProjectConfig) -> String {
    config.package.java.replace('.', "/")
}

pub fn default_registry_path() -> RainyResult<PathBuf> {
    crate::bundled_assets::registry_path()
}

fn default_generated() -> String {
    "generated".to_string()
}

fn default_evidence() -> String {
    "evidence".to_string()
}
