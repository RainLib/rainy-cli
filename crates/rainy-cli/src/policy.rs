use crate::config;
use crate::error::{RainyError, RainyResult};
use crate::patch::ChangeSet;
use crate::registry::RegistryClient;
use globset::{Glob, GlobSetBuilder};
use std::path::Path;

const BUILTIN_DENY: &[&str] = &[
    "**/application-prod.yml",
    "**/.env.production",
    "**/secrets/**",
    "**/*.pem",
    "**/*.key",
    "**/*.p12",
];

const BUILTIN_REQUIRE_APPROVAL: &[&str] = &[
    "gateway.publish",
    "k8s.apply",
    "db.migrate",
    "secret.write",
    "production.config.change",
];

pub fn check_plan(workspace: &Path, plan: &crate::actions::ExecutionPlan) -> RainyResult<()> {
    let config = config::load_config(workspace)?;
    let mut policy = load_layered_policy(workspace, config.policy)?;
    merge_policy(&mut policy, capability_policy(workspace, &plan.capability)?);
    policy.require_approval.extend(
        BUILTIN_REQUIRE_APPROVAL
            .iter()
            .map(|operation| operation.to_string()),
    );
    for action in &plan.actions {
        if policy
            .require_approval
            .iter()
            .any(|operation| operation == &action.id || operation == &action.uses)
        {
            return Err(RainyError::policy(
                "POLICY_APPROVAL_REQUIRED",
                format!(
                    "action {} ({}) requires approval before apply",
                    action.id, action.uses
                ),
            ));
        }
    }
    Ok(())
}

pub fn check_plan_changes(
    workspace: &Path,
    plan: &crate::actions::ExecutionPlan,
    changes: &ChangeSet,
) -> RainyResult<()> {
    let config = config::load_config(workspace)?;
    let mut policy = load_layered_policy(workspace, config.policy)?;
    merge_policy(&mut policy, capability_policy(workspace, &plan.capability)?);
    check_changes_with_policy(changes, policy)
}

pub fn check_changes(workspace: &Path, changes: &ChangeSet) -> RainyResult<()> {
    let config = config::load_config(workspace)?;
    let layered_policy = load_layered_policy(workspace, config.policy)?;
    check_changes_with_policy(changes, layered_policy)
}

fn check_changes_with_policy(
    changes: &ChangeSet,
    policy: config::PolicySection,
) -> RainyResult<()> {
    let mut deny_patterns = policy.deny_edit;
    deny_patterns.extend(BUILTIN_DENY.iter().map(|pattern| pattern.to_string()));
    let deny = compile(&deny_patterns)?;
    let allow = compile(&policy.allow_edit)?;
    let has_allow = !policy.allow_edit.is_empty();

    for change in &changes.changes {
        if change.noop {
            continue;
        }
        if deny.is_match(&change.path) {
            return Err(RainyError::policy(
                "POLICY_DENY_EDIT",
                format!("editing {} is denied by policy", change.path),
            ));
        }
        if has_allow && !allow.is_match(&change.path) {
            return Err(RainyError::policy(
                "POLICY_DENY_EDIT",
                format!("editing {} is not allowed by policy", change.path),
            ));
        }
    }
    Ok(())
}

fn capability_policy(workspace: &Path, capability_id: &str) -> RainyResult<config::PolicySection> {
    let Ok(registry) = RegistryClient::load(workspace) else {
        return Ok(config::PolicySection::default());
    };
    match registry.get_capability(capability_id) {
        Ok(capability) => Ok(capability.policy),
        Err(_) => Ok(config::PolicySection::default()),
    }
}

pub fn command_is_dangerous(command: &str) -> bool {
    let lowered = command.to_ascii_lowercase();
    lowered.contains("rm -rf")
        || lowered.contains("kubectl delete")
        || lowered.contains("drop database")
        || lowered.contains("chmod -r 777 /")
        || lowered.contains("cat .env.production")
}

pub fn check_command(command: &str) -> RainyResult<()> {
    if command_is_dangerous(command) {
        return Err(RainyError::policy(
            "POLICY_DENY_COMMAND",
            format!("command is denied by policy: {command}"),
        ));
    }
    Ok(())
}

fn load_layered_policy(
    workspace: &Path,
    project_policy: config::PolicySection,
) -> RainyResult<config::PolicySection> {
    let mut policy = config::PolicySection::default();
    for path in policy_files(workspace) {
        if !path.exists() {
            continue;
        }
        let content = std::fs::read_to_string(&path)?;
        let layer: config::PolicySection = serde_yaml::from_str(&content)?;
        merge_policy(&mut policy, layer);
    }
    merge_policy(&mut policy, project_policy);
    Ok(policy)
}

fn merge_policy(target: &mut config::PolicySection, layer: config::PolicySection) {
    extend_unique(&mut target.allow_edit, layer.allow_edit);
    extend_unique(&mut target.deny_edit, layer.deny_edit);
    extend_unique(&mut target.require_approval, layer.require_approval);
}

fn extend_unique(target: &mut Vec<String>, values: Vec<String>) {
    for value in values {
        if !target.contains(&value) {
            target.push(value);
        }
    }
}

fn policy_files(workspace: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    files.push(std::path::PathBuf::from("/etc/rainy/policy.yaml"));
    if let Some(home) = std::env::var_os("HOME") {
        files.push(std::path::PathBuf::from(home).join(".rainy/policy.yaml"));
    }
    files.push(workspace.join(".rainy/org-policy.yaml"));
    files
}

fn compile(patterns: &[String]) -> RainyResult<globset::GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(Glob::new(pattern).map_err(|err| {
            RainyError::policy(
                "POLICY_PATTERN_INVALID",
                format!("invalid policy pattern {pattern}: {err}"),
            )
        })?);
    }
    builder.build().map_err(|err| {
        RainyError::policy(
            "POLICY_PATTERN_INVALID",
            format!("invalid policy patterns: {err}"),
        )
    })
}
