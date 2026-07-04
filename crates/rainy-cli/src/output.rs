use crate::actions::CapabilityOutcome;
use crate::doctor::DoctorReport;
use crate::error::RainyError;
use crate::registry::{CapabilityGraph, CapabilityInfo, CapabilitySummary, PackInfo};
use crate::verify::VerifyReport;
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum CommandOutput {
    Message {
        status: &'static str,
        message: String,
    },
    Init {
        status: &'static str,
        project: String,
        path: String,
        files: Vec<String>,
    },
    DryRun {
        status: &'static str,
        capability: String,
        plan: crate::actions::ExecutionPlan,
        diff: String,
    },
    Applied {
        status: &'static str,
        capability: String,
        changed_files: Vec<String>,
    },
    ChangeDryRun {
        status: &'static str,
        operation: String,
        diff: String,
    },
    ChangeApplied {
        status: &'static str,
        operation: String,
        changed_files: Vec<String>,
    },
    Capabilities {
        capabilities: Vec<CapabilitySummary>,
    },
    Capability {
        capability: CapabilityInfo,
    },
    CapabilityGraph {
        graph: CapabilityGraph,
    },
    Installed {
        capabilities: Vec<crate::config::InstalledCapability>,
    },
    Packs {
        packs: Vec<PackInfo>,
    },
    Doctor {
        report: DoctorReport,
    },
    Verify {
        report: VerifyReport,
    },
    Evidence {
        status: &'static str,
        files: Vec<String>,
    },
    Plugins {
        plugins: Vec<crate::plugin::PluginInfo>,
    },
    PluginRun {
        plugin: String,
        stdout: String,
        stderr: String,
    },
    Conformance {
        report: crate::conformance::ConformanceReport,
    },
    Schemas {
        schemas: Vec<crate::schema::SchemaInfo>,
    },
    SchemaValidation {
        report: crate::schema::SchemaValidationReport,
    },
    AgentContext {
        context: String,
    },
    Update {
        report: crate::update::UpdateReport,
    },
}

impl CommandOutput {
    pub fn message(message: impl Into<String>) -> Self {
        Self::Message {
            status: "ok",
            message: message.into(),
        }
    }

    pub fn dry_run(outcome: CapabilityOutcome) -> Self {
        let diff = crate::patch::render_diff(&outcome.changes);
        Self::DryRun {
            status: "dry-run",
            capability: outcome.plan.capability.clone(),
            plan: outcome.plan,
            diff,
        }
    }

    pub fn applied(outcome: CapabilityOutcome) -> Self {
        let changed_files = outcome
            .changes
            .changes
            .into_iter()
            .filter(|change| !change.noop)
            .map(|change| change.path)
            .collect();
        Self::Applied {
            status: "applied",
            capability: outcome.plan.capability,
            changed_files,
        }
    }

    pub fn change_dry_run(operation: impl Into<String>, changes: crate::patch::ChangeSet) -> Self {
        Self::ChangeDryRun {
            status: "dry-run",
            operation: operation.into(),
            diff: crate::patch::render_diff(&changes),
        }
    }

    pub fn change_applied(operation: impl Into<String>, changes: crate::patch::ChangeSet) -> Self {
        Self::ChangeApplied {
            status: "applied",
            operation: operation.into(),
            changed_files: changes.changed_files(),
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::Message { .. } => "message",
            Self::Init { .. } => "init",
            Self::DryRun { .. } => "dry-run",
            Self::Applied { .. } => "applied",
            Self::ChangeDryRun { .. } => "change-dry-run",
            Self::ChangeApplied { .. } => "change-applied",
            Self::Capabilities { .. } => "capabilities",
            Self::Capability { .. } => "capability",
            Self::CapabilityGraph { .. } => "capability-graph",
            Self::Installed { .. } => "installed",
            Self::Packs { .. } => "packs",
            Self::Doctor { .. } => "doctor",
            Self::Verify { .. } => "verify",
            Self::Evidence { .. } => "evidence",
            Self::Plugins { .. } => "plugins",
            Self::PluginRun { .. } => "plugin-run",
            Self::Conformance { .. } => "conformance",
            Self::Schemas { .. } => "schemas",
            Self::SchemaValidation { .. } => "schema-validation",
            Self::AgentContext { .. } => "agent-context",
            Self::Update { .. } => "update",
        }
    }

    pub fn status(&self) -> &str {
        match self {
            Self::Message { status, .. }
            | Self::Init { status, .. }
            | Self::DryRun { status, .. }
            | Self::Applied { status, .. }
            | Self::ChangeDryRun { status, .. }
            | Self::ChangeApplied { status, .. }
            | Self::Evidence { status, .. } => status,
            Self::Doctor { report } => &report.status,
            Self::Verify { report } => &report.status,
            Self::Conformance { report } => &report.status,
            Self::SchemaValidation { report } => &report.status,
            Self::Update { .. } => "ok",
            _ => "ok",
        }
    }

    pub fn is_dry_run(&self) -> bool {
        matches!(
            self,
            Self::DryRun { .. }
                | Self::ChangeDryRun { .. }
                | Self::Init {
                    status: "dry-run",
                    ..
                }
        )
    }

    pub fn audit_summary(&self) -> String {
        match self {
            Self::Message { message, .. } => message.clone(),
            Self::Init { project, files, .. } => {
                format!("initialized {project} with {} files", files.len())
            }
            Self::DryRun { capability, .. } => format!("planned capability {capability}"),
            Self::Applied {
                capability,
                changed_files,
                ..
            } => format!(
                "applied capability {capability}; changed {} files",
                changed_files.len()
            ),
            Self::ChangeDryRun { operation, .. } => format!("planned {operation}"),
            Self::ChangeApplied {
                operation,
                changed_files,
                ..
            } => format!("applied {operation}; changed {} files", changed_files.len()),
            Self::Capabilities { capabilities } => {
                format!("listed {} capabilities", capabilities.len())
            }
            Self::Capability { capability } => format!("explained capability {}", capability.id),
            Self::CapabilityGraph { graph } => format!("graph has {} nodes", graph.nodes.len()),
            Self::Installed { capabilities } => {
                format!("listed {} installed capabilities", capabilities.len())
            }
            Self::Packs { packs } => format!("listed {} packs", packs.len()),
            Self::Doctor { report } => format!("doctor {}", report.status),
            Self::Verify { report } => format!("verify {} {}", report.profile, report.status),
            Self::Evidence { files, .. } => format!("generated {} evidence files", files.len()),
            Self::Plugins { plugins } => format!("listed {} plugins", plugins.len()),
            Self::PluginRun { plugin, .. } => format!("ran plugin {plugin}"),
            Self::Conformance { report } => format!("conformance {}", report.status),
            Self::Schemas { schemas } => format!("listed {} schemas", schemas.len()),
            Self::SchemaValidation { report } => format!("schema validation {}", report.status),
            Self::AgentContext { .. } => "rendered agent context".to_string(),
            Self::Update { report } => {
                if report.update_available {
                    format!(
                        "update available {} -> {}",
                        report.current_version,
                        report.latest_version.as_deref().unwrap_or("unknown")
                    )
                } else {
                    format!("rainy is up to date at {}", report.current_version)
                }
            }
        }
    }

    pub fn print(&self, json: bool) {
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(self).expect("serialize command output")
            );
            return;
        }

        match self {
            Self::Message { message, .. } => println!("{message}"),
            Self::Init {
                status,
                project,
                path,
                files,
                ..
            } => {
                if *status == "dry-run" {
                    println!("Dry-run init for {project} at {path}");
                } else {
                    println!("Created {project} at {path}");
                }
                for file in files {
                    println!("  {file}");
                }
            }
            Self::DryRun {
                capability,
                plan,
                diff,
                ..
            } => {
                println!("Dry-run plan for capability {capability}");
                println!("Actions:");
                for action in &plan.actions {
                    println!("  - {} ({})", action.id, action.uses);
                }
                if diff.trim().is_empty() {
                    println!("No file changes.");
                } else {
                    println!("{diff}");
                }
            }
            Self::Applied {
                capability,
                changed_files,
                ..
            } => {
                if changed_files.is_empty() {
                    println!("Capability {capability} is already installed; no changes.");
                } else {
                    println!("Applied capability {capability}");
                    for file in changed_files {
                        println!("  {file}");
                    }
                }
            }
            Self::ChangeDryRun {
                operation, diff, ..
            } => {
                println!("Dry-run {operation}");
                if diff.trim().is_empty() {
                    println!("No file changes.");
                } else {
                    println!("{diff}");
                }
            }
            Self::ChangeApplied {
                operation,
                changed_files,
                ..
            } => {
                if changed_files.is_empty() {
                    println!("{operation}: no changes.");
                } else {
                    println!("Applied {operation}");
                    for file in changed_files {
                        println!("  {file}");
                    }
                }
            }
            Self::Capabilities { capabilities } => {
                for capability in capabilities {
                    println!(
                        "{}\t{}\t{}",
                        capability.id, capability.version, capability.description
                    );
                }
            }
            Self::Capability { capability } => {
                println!("{} {}", capability.id, capability.version);
                println!("{}", capability.description);
                if !capability.depends_on.is_empty() {
                    println!("Depends on: {}", capability.depends_on.join(", "));
                }
                if !capability.providers.is_empty() {
                    println!("Providers: {}", capability.providers.join(", "));
                }
            }
            Self::CapabilityGraph { graph } => {
                for node in &graph.nodes {
                    let deps = graph
                        .edges
                        .iter()
                        .filter(|edge| edge.from == *node)
                        .map(|edge| edge.to.as_str())
                        .collect::<Vec<_>>();
                    println!("{node}: {}", deps.join(", "));
                }
            }
            Self::Installed { capabilities } => {
                for capability in capabilities {
                    println!(
                        "{}\t{}\t{}",
                        capability.id,
                        capability.version,
                        capability.provider.as_deref().unwrap_or("-")
                    );
                }
            }
            Self::Packs { packs } => {
                for pack in packs {
                    println!("{}\t{}\t{}", pack.name, pack.version, pack.path);
                }
            }
            Self::Doctor { report } => {
                println!("Doctor: {}", report.status);
                for check in &report.checks {
                    println!("  {} {} - {}", check.status, check.id, check.message);
                }
            }
            Self::Verify { report } => {
                println!("Verify {}: {}", report.profile, report.status);
                for check in &report.checks {
                    println!("  {} {} - {}", check.status, check.id, check.message);
                }
            }
            Self::Evidence { files, .. } => {
                println!("Generated evidence:");
                for file in files {
                    println!("  {file}");
                }
            }
            Self::Plugins { plugins } => {
                if plugins.is_empty() {
                    println!("No rainy-* plugins found.");
                } else {
                    for plugin in plugins {
                        println!("{}\t{}", plugin.name, plugin.path);
                        if !plugin.shadowed_paths.is_empty() {
                            println!(
                                "  warning: shadowed duplicate plugin(s): {}",
                                plugin.shadowed_paths.join(", ")
                            );
                        }
                    }
                }
            }
            Self::PluginRun { stdout, stderr, .. } => {
                if !stdout.is_empty() {
                    print!("{stdout}");
                }
                if !stderr.is_empty() {
                    eprint!("{stderr}");
                }
            }
            Self::Conformance { report } => {
                println!("Conformance: {}", report.status);
                for check in &report.checks {
                    println!("  {} {} - {}", check.status, check.id, check.message);
                }
            }
            Self::Schemas { schemas } => {
                for schema in schemas {
                    println!("{}\t{}", schema.name, schema.path);
                }
            }
            Self::SchemaValidation { report } => {
                println!("Schema validation: {}", report.status);
                for issue in &report.issues {
                    println!("  {} - {}", issue.path, issue.message);
                }
            }
            Self::AgentContext { context } => println!("{context}"),
            Self::Update { report } => {
                if report.update_available {
                    println!(
                        "Rainy update available: {} -> {}",
                        report.current_version,
                        report.latest_version.as_deref().unwrap_or("unknown")
                    );
                    if report.skipped {
                        println!("This version is currently skipped.");
                    } else {
                        println!("Update with: rainy self update");
                    }
                } else {
                    println!("Rainy is up to date: {}", report.current_version);
                }
                println!("Install command: {}", report.install_command);
            }
        }
    }
}

#[derive(Debug, Serialize)]
struct ErrorEnvelope {
    status: &'static str,
    error: crate::error::ErrorBody,
}

pub fn print_error(err: &RainyError, json: bool) {
    let body = err.body();
    if json {
        let envelope = ErrorEnvelope {
            status: "error",
            error: body,
        };
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&envelope).expect("serialize error")
        );
    } else {
        eprintln!("{}: {}", body.code, body.message);
    }
}
