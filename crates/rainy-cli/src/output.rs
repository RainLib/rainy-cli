use crate::actions::CapabilityOutcome;
use crate::doctor::DoctorReport;
use crate::error::RainyError;
use crate::registry::{CapabilityGraph, CapabilityInfo, CapabilitySummary, PackInfo};
use crate::verify::VerifyReport;
use serde::Serialize;

const COMMAND_PROTOCOL_VERSION: &str = "rainy.command.v1";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CommandEnvelope {
    protocol_version: &'static str,
    #[serde(rename = "type")]
    output_type: &'static str,
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    trace_id: Option<String>,
    data: serde_json::Value,
}

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
    ProjectTemplate {
        status: &'static str,
        project: String,
        path: String,
        template: String,
        source: String,
        requested_ref: String,
        resolved_ref: Option<String>,
        source_git_removed: bool,
        files: Vec<String>,
        default_branch: String,
        remote_url: Option<String>,
        next_commands: Vec<String>,
    },
    SourceProject {
        status: &'static str,
        project: String,
        path: String,
        source: String,
        source_version: String,
        resolved_ref: String,
        template: String,
        modules: Vec<String>,
        files: Vec<String>,
        remote_url: Option<String>,
        next_commands: Vec<String>,
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
    Registry {
        report: crate::registry::RegistryReport,
    },
    Source {
        report: crate::source::SourceReport,
    },
    Defaults {
        report: crate::defaults::DefaultsReport,
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
        #[serde(skip_serializing_if = "Option::is_none")]
        apply_command: Option<String>,
    },
    Plugins {
        plugins: Vec<crate::plugin::PluginInfo>,
    },
    PluginRun {
        plugin: String,
        stdout: String,
        stderr: String,
        stdout_truncated: bool,
        stderr_truncated: bool,
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
    Skill {
        report: crate::skills::SkillReport,
    },
    Update {
        report: crate::update::UpdateReport,
    },
    Completion {
        shell: String,
        script: String,
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
            Self::ProjectTemplate { .. } => "project-template",
            Self::SourceProject { .. } => "source-project",
            Self::DryRun { .. } => "dry-run",
            Self::Applied { .. } => "applied",
            Self::ChangeDryRun { .. } => "change-dry-run",
            Self::ChangeApplied { .. } => "change-applied",
            Self::Capabilities { .. } => "capabilities",
            Self::Capability { .. } => "capability",
            Self::CapabilityGraph { .. } => "capability-graph",
            Self::Installed { .. } => "installed",
            Self::Packs { .. } => "packs",
            Self::Registry { .. } => "registry",
            Self::Source { .. } => "source",
            Self::Defaults { .. } => "defaults",
            Self::Doctor { .. } => "doctor",
            Self::Verify { .. } => "verify",
            Self::Evidence { .. } => "evidence",
            Self::Plugins { .. } => "plugins",
            Self::PluginRun { .. } => "plugin-run",
            Self::Conformance { .. } => "conformance",
            Self::Schemas { .. } => "schemas",
            Self::SchemaValidation { .. } => "schema-validation",
            Self::AgentContext { .. } => "agent-context",
            Self::Skill { .. } => "skill",
            Self::Update { .. } => "update",
            Self::Completion { .. } => "completion",
        }
    }

    pub fn status(&self) -> &str {
        match self {
            Self::Message { status, .. }
            | Self::Init { status, .. }
            | Self::ProjectTemplate { status, .. }
            | Self::SourceProject { status, .. }
            | Self::DryRun { status, .. }
            | Self::Applied { status, .. }
            | Self::ChangeDryRun { status, .. }
            | Self::ChangeApplied { status, .. }
            | Self::Evidence { status, .. } => status,
            Self::Doctor { report } => &report.status,
            Self::Verify { report } => &report.status,
            Self::Conformance { report } => &report.status,
            Self::SchemaValidation { report } => &report.status,
            Self::Skill { report } => &report.status,
            Self::Registry { report } => &report.status,
            Self::Source { report } => &report.status,
            Self::Defaults { report } => &report.status,
            Self::Update { report } => &report.status,
            Self::Completion { .. } => "ok",
            _ => "ok",
        }
    }

    pub fn protocol_status(&self) -> &'static str {
        match self.status() {
            "dry-run" | "preview" => "preview",
            "applied" => "applied",
            "failed" | "fail" => "failed",
            "warning" | "warn" | "degraded" => "warning",
            _ => "ok",
        }
    }

    pub fn exit_code(&self) -> i32 {
        if self.protocol_status() == "failed" {
            4
        } else {
            0
        }
    }

    pub fn is_dry_run(&self) -> bool {
        match self {
            Self::DryRun { .. }
            | Self::ChangeDryRun { .. }
            | Self::Init {
                status: "dry-run", ..
            }
            | Self::ProjectTemplate {
                status: "dry-run", ..
            }
            | Self::SourceProject {
                status: "dry-run", ..
            } => true,
            Self::Skill { report } => report.status == "dry-run",
            Self::Source { report } => report.status == "dry-run",
            _ => false,
        }
    }

    pub fn audit_summary(&self) -> String {
        match self {
            Self::Message { message, .. } => message.clone(),
            Self::Init { project, files, .. } => {
                format!("initialized {project} with {} files", files.len())
            }
            Self::ProjectTemplate {
                project,
                template,
                files,
                ..
            } => format!(
                "created {project} from template {template} with {} files",
                files.len()
            ),
            Self::SourceProject {
                project,
                source,
                template,
                modules,
                files,
                ..
            } => format!(
                "created {project} from Source {source} template {template} with {} modules and {} files",
                modules.len(),
                files.len()
            ),
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
            Self::Registry { report } => {
                format!("registry {} {}", report.operation, report.status)
            }
            Self::Source { report } => {
                format!("source {} {}", report.operation, report.status)
            }
            Self::Defaults { report } => {
                format!("defaults {} {}", report.operation, report.status)
            }
            Self::Doctor { report } => format!("doctor {}", report.status),
            Self::Verify { report } => format!("verify {} {}", report.profile, report.status),
            Self::Evidence { files, .. } => format!("generated {} evidence files", files.len()),
            Self::Plugins { plugins } => format!("listed {} plugins", plugins.len()),
            Self::PluginRun { plugin, .. } => format!("ran plugin {plugin}"),
            Self::Conformance { report } => format!("conformance {}", report.status),
            Self::Schemas { schemas } => format!("listed {} schemas", schemas.len()),
            Self::SchemaValidation { report } => format!("schema validation {}", report.status),
            Self::AgentContext { .. } => "rendered agent context".to_string(),
            Self::Skill { report } => {
                format!("skill {} {}", report.operation, report.status)
            }
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
            Self::Completion { shell, .. } => format!("generated {shell} completion"),
        }
    }

    pub fn print(&self, json: bool, verbose: bool, trace_id: Option<&str>) {
        if json {
            let mut data = serde_json::to_value(self).expect("serialize command output");
            if let Some(object) = data.as_object_mut() {
                object.remove("type");
                object.remove("status");
            }
            crate::redaction::json(&mut data);
            let envelope = CommandEnvelope {
                protocol_version: COMMAND_PROTOCOL_VERSION,
                output_type: self.kind(),
                status: self.protocol_status(),
                trace_id: trace_id.map(str::to_string),
                data,
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&envelope).expect("serialize command envelope")
            );
            return;
        }

        match self {
            Self::Message {
                status, message, ..
            } => {
                print_title("Rainy");
                print_summary(&[
                    ("Status", result_status_label(status).to_string()),
                    ("Result", message.clone()),
                ]);
                if *status == "dry-run" {
                    print_next_step("Rerun the same command with --apply.");
                }
            }
            Self::Init {
                status,
                project,
                path,
                files,
                ..
            } => {
                print_title("Project initialization");
                print_summary(&[
                    ("Status", result_status_label(status).to_string()),
                    ("Project", project.clone()),
                    ("Location", path.clone()),
                    ("Files", files.len().to_string()),
                ]);
                if *status == "dry-run" {
                    print_next_step(
                        "Review the preview, then rerun the same command with --apply.",
                    );
                    print_paths("Planned locations", files);
                } else {
                    print_paths("Affected locations", files);
                }
            }
            Self::ProjectTemplate {
                status,
                project,
                path,
                template,
                source,
                requested_ref,
                resolved_ref,
                source_git_removed,
                files,
                default_branch,
                remote_url,
                next_commands,
            } => {
                print_title("Enterprise project template");
                print_summary(&[
                    ("Status", result_status_label(status).to_string()),
                    ("Project", project.clone()),
                    ("Template", template.clone()),
                    ("Location", path.clone()),
                    ("Source ref", requested_ref.clone()),
                    (
                        "Resolved commit",
                        resolved_ref
                            .clone()
                            .unwrap_or_else(|| "Not fetched".to_string()),
                    ),
                    (
                        "Source Git metadata",
                        if *source_git_removed {
                            "Removed".to_string()
                        } else {
                            "Will be excluded".to_string()
                        },
                    ),
                    ("Target branch", default_branch.clone()),
                    (
                        "Target remote",
                        remote_url
                            .clone()
                            .unwrap_or_else(|| "Not configured".to_string()),
                    ),
                    (
                        "Files",
                        if *status == "dry-run" {
                            "Not fetched".to_string()
                        } else {
                            files.len().to_string()
                        },
                    ),
                ]);
                print_details(&format!("Template source: {source}"));
                if *status == "dry-run" {
                    print_next_step(
                        "Review the source, ref, destination, and repository settings, then rerun with --apply.",
                    );
                } else {
                    println!();
                    println!("Git repository setup");
                    println!("  Default branch  {default_branch}");
                    println!(
                        "  Remote URL      {}",
                        remote_url.as_deref().unwrap_or("Not configured")
                    );
                    print_next_step("Create the destination Git repository, then run:");
                    for command in next_commands {
                        println!("  $ {command}");
                    }
                    print_paths("Affected locations", files);
                }
            }
            Self::SourceProject {
                status,
                project,
                path,
                source,
                source_version,
                resolved_ref,
                template,
                modules,
                files,
                remote_url,
                next_commands,
            } => {
                print_title("Rainy Source project");
                print_summary(&[
                    ("Status", result_status_label(status).to_string()),
                    ("Project", project.clone()),
                    ("Location", path.clone()),
                    ("Source", source.clone()),
                    ("Source version", source_version.clone()),
                    ("Resolved revision", resolved_ref.clone()),
                    ("Template", template.clone()),
                    ("Modules", list_or_none(modules)),
                    (
                        "Target remote",
                        remote_url
                            .clone()
                            .unwrap_or_else(|| "Not configured".to_string()),
                    ),
                    (
                        "Files",
                        if *status == "dry-run" {
                            "Not written".to_string()
                        } else {
                            files.len().to_string()
                        },
                    ),
                ]);
                if *status == "dry-run" {
                    print_next_step(
                        "Review the selected Source template and modules, then rerun with --apply.",
                    );
                } else {
                    print_details(
                        "Origin metadata was recorded in .rainy/project-source.lock for later checks.",
                    );
                    print_next_step("Create the destination Git repository, then run:");
                    for command in next_commands {
                        println!("  $ {command}");
                    }
                    print_paths("Affected locations", files);
                }
            }
            Self::DryRun {
                capability,
                plan,
                diff,
                ..
            } => {
                print_title("Capability plan");
                print_summary(&[
                    ("Status", "Preview only; no files changed".to_string()),
                    ("Capability", capability.clone()),
                    ("Plan", plan.id.clone()),
                    ("Actions", plan.actions.len().to_string()),
                ]);
                print_next_step(
                    "Review the plan and diff, then rerun the same capability command with --apply.",
                );
                println!();
                println!("Actions");
                for action in &plan.actions {
                    print_columns(&[&action.id, &action.uses]);
                }
                println!();
                println!("Changes");
                if diff.trim().is_empty() {
                    println!("  No file changes.");
                } else {
                    println!("{diff}");
                }
            }
            Self::Applied {
                capability,
                changed_files,
                ..
            } => {
                print_title("Capability apply");
                print_summary(&[
                    ("Status", "Applied".to_string()),
                    ("Capability", capability.clone()),
                    ("Changed files", changed_files.len().to_string()),
                ]);
                if changed_files.is_empty() {
                    print_details("No changes were required; the capability is already installed.");
                } else {
                    print_paths("Affected locations", changed_files);
                    print_next_step("Run rainy doctor, then rainy verify --profile local.");
                }
            }
            Self::ChangeDryRun {
                operation, diff, ..
            } => {
                print_title("Change plan");
                print_summary(&[
                    ("Status", "Preview only; no files changed".to_string()),
                    ("Operation", operation.clone()),
                ]);
                print_next_step("Review the diff, then rerun the same command with --apply.");
                println!();
                println!("Changes");
                if diff.trim().is_empty() {
                    println!("  No file changes.");
                } else {
                    println!("{diff}");
                }
            }
            Self::ChangeApplied {
                operation,
                changed_files,
                ..
            } => {
                print_title("Change apply");
                print_summary(&[
                    ("Status", "Applied".to_string()),
                    ("Operation", operation.clone()),
                    ("Changed files", changed_files.len().to_string()),
                ]);
                if changed_files.is_empty() {
                    print_details("No changes were required.");
                } else {
                    print_paths("Affected locations", changed_files);
                }
            }
            Self::Capabilities { capabilities } => {
                print_title("Capabilities");
                print_summary(&[("Available", capabilities.len().to_string())]);
                println!();
                println!("Items");
                for capability in capabilities {
                    print_columns(&[&capability.id, &capability.version, &capability.description]);
                }
            }
            Self::Capability { capability } => {
                print_title("Capability details");
                print_summary(&[
                    ("ID", capability.id.clone()),
                    ("Name", capability.name.clone()),
                    ("Version", capability.version.clone()),
                    ("Pack", capability.pack.clone()),
                    ("Description", capability.description.clone()),
                    ("Dependencies", list_or_none(&capability.depends_on)),
                    ("Providers", list_or_none(&capability.providers)),
                    ("Actions", list_or_none(&capability.actions)),
                ]);
            }
            Self::CapabilityGraph { graph } => {
                print_title("Capability graph");
                print_summary(&[
                    ("Nodes", graph.nodes.len().to_string()),
                    ("Edges", graph.edges.len().to_string()),
                ]);
                println!();
                println!("Dependencies");
                for node in &graph.nodes {
                    let deps = graph
                        .edges
                        .iter()
                        .filter(|edge| edge.from == *node)
                        .map(|edge| edge.to.as_str())
                        .collect::<Vec<_>>();
                    println!(
                        "  {node}: {}",
                        if deps.is_empty() {
                            "none".to_string()
                        } else {
                            deps.join(", ")
                        }
                    );
                }
            }
            Self::Installed { capabilities } => {
                print_title("Installed capabilities");
                print_summary(&[("Installed", capabilities.len().to_string())]);
                println!();
                println!("Items");
                for capability in capabilities {
                    print_columns(&[
                        &capability.id,
                        &capability.version,
                        capability.provider.as_deref().unwrap_or("-"),
                    ]);
                }
            }
            Self::Packs { packs } => {
                print_title("Capability packs");
                print_summary(&[("Available", packs.len().to_string())]);
                println!();
                println!("Items");
                for pack in packs {
                    print_columns(&[&pack.name, &pack.version, &pack.path]);
                }
            }
            Self::Registry { report } => {
                print_title(&format!("Registry {}", report.operation));
                print_summary(&[
                    ("Status", result_status_label(&report.status).to_string()),
                    ("Registries", report.registries.len().to_string()),
                ]);
                if !report.registries.is_empty() {
                    println!();
                    println!("Registries");
                    for registry in &report.registries {
                        print_columns(&[
                            &registry.name,
                            &registry.source_type,
                            &format!("priority {}", registry.priority),
                            &registry.source,
                        ]);
                        if let Some(resolved) = &registry.resolved_ref {
                            println!("    resolved {resolved}");
                        }
                        if !registry.modules.is_empty() {
                            println!("    modules  {}", registry.modules.join(", "));
                        }
                        if !registry.installed_skills.is_empty() {
                            println!("    skills   {}", registry.installed_skills.join(", "));
                        }
                        if verbose {
                            if let Some(cache_path) = &registry.cache_path {
                                println!("    cache    {cache_path}");
                            }
                        }
                    }
                }
                if !report.checks.is_empty() {
                    let checks = report
                        .checks
                        .iter()
                        .map(|check| {
                            (
                                check.status.as_str(),
                                check.id.as_str(),
                                check.message.as_str(),
                            )
                        })
                        .collect::<Vec<_>>();
                    println!();
                    println!("Validation");
                    for (status, id, message) in checks
                        .iter()
                        .filter(|(status, _, _)| verbose || !matches!(*status, "pass" | "passed"))
                    {
                        print_check_line(status, id, message);
                    }
                    if !verbose
                        && checks
                            .iter()
                            .all(|(status, _, _)| matches!(*status, "pass" | "passed"))
                    {
                        println!("  All registry checks passed. Use --verbose for details.");
                    }
                }
                if report.status == "dry-run" {
                    print_next_step(
                        "Review the registry plan, then rerun the same command with --apply.",
                    );
                }
            }
            Self::Source { report } => {
                print_title(&format!("Source {}", report.operation));
                print_summary(&[
                    ("Status", result_status_label(&report.status).to_string()),
                    ("Sources", report.sources.len().to_string()),
                ]);
                if !report.sources.is_empty() {
                    println!();
                    println!("Sources");
                }
                for source in &report.sources {
                    print_columns(&[
                        &source.name,
                        &source.state,
                        source.current_version.as_deref().unwrap_or("-"),
                        &source.source,
                    ]);
                    if let Some(latest) = &source.latest_version
                        && source.current_version.as_ref() != Some(latest)
                    {
                        println!("    latest    {latest}");
                    }
                    if let Some(resolved) = &source.resolved_ref {
                        println!("    revision  {resolved}");
                    }
                    if let Some(message) = &source.message {
                        println!("    result    {message}");
                    }
                    if !source.contents.is_empty() {
                        println!("    contents");
                        for content in &source.contents {
                            println!(
                                "      {}  {}  {}",
                                content.id, content.content_type, content.path
                            );
                            if report.operation == "resolve"
                                && let Some(path) = &content.resolved_path
                            {
                                println!("        resolved  {path}");
                            }
                        }
                    }
                    if verbose && let Some(cache) = &source.cache_path {
                        println!("    cache     {cache}");
                    }
                }
                match report.status.as_str() {
                    "dry-run" => print_next_step(
                        "Review the Source and rerun the same command with --apply.",
                    ),
                    "warning" => print_next_step(
                        "Cached Sources remain usable. Restore connectivity, then rerun source check or update.",
                    ),
                    _ => {}
                }
            }
            Self::Defaults { report } => {
                print_title(&format!("Defaults {}", report.operation));
                print_summary(&[
                    ("Status", result_status_label(&report.status).to_string()),
                    (
                        "Package",
                        report.package_version.as_deref().unwrap_or("-").to_string(),
                    ),
                    ("Source", report.source.clone()),
                    ("Requested ref", report.requested_ref.clone()),
                ]);
                if let Some(resolved) = &report.resolved_ref {
                    print_details(&format!("Resolved commit: {resolved}"));
                }
                if let Some(cache) = &report.cache_path {
                    print_details(&format!("Content root: {cache}"));
                }
                if report.status == "dry-run" {
                    print_next_step(
                        "Review the source and ref, then rerun the same command with --apply.",
                    );
                } else if report.status == "missing" {
                    print_next_step("Run rainy defaults install --apply while online.");
                }
            }
            Self::Doctor { report } => {
                print_title("Project doctor");
                let checks = report
                    .checks
                    .iter()
                    .map(|check| {
                        (
                            check.status.as_str(),
                            check.id.as_str(),
                            check.message.as_str(),
                        )
                    })
                    .collect::<Vec<_>>();
                print_check_report(&report.status, &checks, verbose, Vec::new());
            }
            Self::Verify { report } => {
                print_title("Project verification");
                let checks = report
                    .checks
                    .iter()
                    .map(|check| {
                        (
                            check.status.as_str(),
                            check.id.as_str(),
                            check.message.as_str(),
                        )
                    })
                    .collect::<Vec<_>>();
                print_check_report(
                    &report.status,
                    &checks,
                    verbose,
                    vec![("Profile", report.profile.clone())],
                );
            }
            Self::Evidence {
                status,
                files,
                apply_command,
            } => {
                print_title("Evidence generation");
                print_summary(&[
                    ("Status", result_status_label(status).to_string()),
                    ("Files", files.len().to_string()),
                ]);
                if let Some(command) = apply_command {
                    print_next_step(&format!("$ {command}"));
                }
                print_paths(
                    if *status == "dry-run" {
                        "Planned locations"
                    } else {
                        "Affected locations"
                    },
                    files,
                );
            }
            Self::Plugins { plugins } => {
                print_title("Plugins");
                print_summary(&[("Discovered", plugins.len().to_string())]);
                if plugins.is_empty() {
                    print_details("No rainy-* plugins were found.");
                } else {
                    println!();
                    println!("Items");
                    for plugin in plugins {
                        print_columns(&[&plugin.name, &plugin.path]);
                        if !plugin.shadowed_paths.is_empty() {
                            println!(
                                "    WARN shadowed duplicate plugin(s): {}",
                                plugin.shadowed_paths.join(", ")
                            );
                        }
                    }
                }
            }
            Self::PluginRun {
                stdout,
                stderr,
                stdout_truncated,
                stderr_truncated,
                ..
            } => {
                if !stdout.is_empty() {
                    print!("{stdout}");
                }
                if !stderr.is_empty() {
                    eprint!("{stderr}");
                }
                if *stdout_truncated || *stderr_truncated {
                    eprintln!();
                    eprintln!(
                        "Plugin output was truncated (stdout: {stdout_truncated}, stderr: {stderr_truncated})."
                    );
                }
            }
            Self::Conformance { report } => {
                print_title("Protocol conformance");
                let checks = report
                    .checks
                    .iter()
                    .map(|check| {
                        (
                            check.status.as_str(),
                            check.id.as_str(),
                            check.message.as_str(),
                        )
                    })
                    .collect::<Vec<_>>();
                print_check_report(&report.status, &checks, verbose, Vec::new());
            }
            Self::Schemas { schemas } => {
                print_title("Schemas");
                print_summary(&[("Available", schemas.len().to_string())]);
                println!();
                println!("Items");
                for schema in schemas {
                    print_columns(&[&schema.name, &schema.path]);
                }
            }
            Self::SchemaValidation { report } => {
                print_title("Schema validation");
                print_summary(&[
                    ("Status", result_status_label(&report.status).to_string()),
                    ("Schema", report.schema.clone()),
                    ("File", report.file.clone()),
                    ("Issues", report.issues.len().to_string()),
                ]);
                if !report.issues.is_empty() {
                    println!();
                    println!("Issues");
                    for issue in &report.issues {
                        print_columns(&[&issue.path, &issue.message]);
                    }
                }
            }
            Self::AgentContext { context } => println!("{context}"),
            Self::Skill { report } => {
                println!("Skill {}", report.operation);
                println!();
                println!("Summary");
                println!("  Status    {}", skill_status_label(&report.status));
                println!("  Bundle    {}", skill_profile_label(&report.profile));
                println!("  Targets   {}", report.targets.join(", "));
                println!("  Language  {}", report.language);

                println!();
                println!("Enabled Skills");
                println!("  Rainy CLI        execution, approval, verify, and evidence");
                if report.profile == "comet" {
                    println!("  Rainy Comet      workflow handoff and safety boundaries");
                    println!("  OpenSpec         requirements and acceptance criteria");
                    println!("  Superpowers      engineering methods and delivery workflow");
                    println!("  Comet            phase orchestration and recovery state");
                }
                for skill in &report.custom_skills {
                    print_columns(&[skill, "project-owned Skill"]);
                }

                if !report.apply_command.is_empty() {
                    println!();
                    println!("Next step");
                    println!("  $ {}", report.apply_command.join(" "));
                    if report.status == "configured" {
                        println!("  Then run: rainy skill doctor");
                    }
                }

                if !report.changed_files.is_empty() {
                    println!();
                    println!(
                        "{}",
                        if report.status == "dry-run" {
                            "Planned locations"
                        } else {
                            "Affected locations"
                        }
                    );
                    for (root, count) in summarize_skill_paths(&report.changed_files) {
                        if report.status == "dry-run" || count == 1 {
                            println!("  {root}");
                        } else {
                            println!("  {root}  ({count} managed entries)");
                        }
                    }
                }

                if !report.checks.is_empty() {
                    let passed = report
                        .checks
                        .iter()
                        .filter(|check| check.status == "pass")
                        .count();
                    let failed = report
                        .checks
                        .iter()
                        .filter(|check| check.status == "fail")
                        .count();
                    println!();
                    println!("Checks");
                    println!("  {passed} passed, {failed} failed");
                    for check in report
                        .checks
                        .iter()
                        .filter(|check| verbose || check.status != "pass")
                    {
                        print_check_line(&check.status, &check.id, &check.message);
                    }
                }

                if verbose {
                    if !report.changed_files.is_empty() {
                        println!();
                        println!("Path details");
                        for file in &report.changed_files {
                            println!("  {file}");
                        }
                    }
                    if !report.command.is_empty() {
                        println!();
                        println!("Upstream command");
                        println!("  {}", report.command.join(" "));
                    }
                } else if !report.command.is_empty() {
                    println!();
                    println!("Details");
                    println!("  Run with --verbose to show upstream commands and every path.");
                }
            }
            Self::Update { report } => {
                print_title("Rainy update");
                if report.operation == "skip" {
                    print_summary(&[
                        (
                            "Status",
                            if report.status == "dry-run" {
                                "Preview only; no state changed"
                            } else {
                                "Update skipped"
                            }
                            .to_string(),
                        ),
                        (
                            "Version",
                            report
                                .latest_version
                                .as_deref()
                                .unwrap_or("unknown")
                                .to_string(),
                        ),
                        ("Repository", report.repository.clone()),
                    ]);
                } else if report.operation == "update" && report.status == "applied" {
                    print_summary(&[
                        ("Status", "Installed and verified".to_string()),
                        (
                            "Version",
                            report
                                .latest_version
                                .as_deref()
                                .unwrap_or("unknown")
                                .to_string(),
                        ),
                        ("Repository", report.repository.clone()),
                    ]);
                } else if report.update_available {
                    print_summary(&[
                        (
                            "Status",
                            if report.status == "dry-run" {
                                "Preview only; no files changed"
                            } else if report.skipped {
                                "Update skipped"
                            } else {
                                "Update available"
                            }
                            .to_string(),
                        ),
                        ("Current", report.current_version.clone()),
                        (
                            "Latest",
                            report
                                .latest_version
                                .as_deref()
                                .unwrap_or("unknown")
                                .to_string(),
                        ),
                        ("Release", report.release_type.clone()),
                    ]);
                    if report.status == "dry-run" {
                        print_details("The selected release has not been installed.");
                    } else if report.skipped {
                        print_details("The latest version is currently skipped.");
                    }
                } else {
                    print_summary(&[
                        ("Status", "Up to date".to_string()),
                        ("Current", report.current_version.clone()),
                        ("Release", report.release_type.clone()),
                    ]);
                }
                if let Some(command) = &report.apply_command {
                    print_next_step(&format!("$ {command}"));
                }
                if verbose {
                    print_details(&format!("Install command: {}", report.install_command));
                }
            }
            Self::Completion { script, .. } => print!("{script}"),
        }
    }
}

fn print_title(title: &str) {
    println!("{title}");
    println!();
}

fn print_summary(rows: &[(&str, String)]) {
    println!("Summary");
    let terminal_width = output_width();
    if terminal_width < 64 {
        for (label, value) in rows {
            println!("  {label}");
            for line in wrap_text(value, terminal_width.saturating_sub(4).max(16)) {
                println!("    {line}");
            }
        }
        return;
    }
    let width = rows
        .iter()
        .map(|(label, _)| label.chars().count())
        .max()
        .unwrap_or(12)
        + 2;
    for (label, value) in rows {
        let prefix = format!("  {label:<width$}");
        let lines = wrap_text(
            value,
            terminal_width
                .saturating_sub(prefix.chars().count())
                .max(16),
        );
        if let Some((first, rest)) = lines.split_first() {
            println!("{prefix}{first}");
            let continuation = " ".repeat(prefix.chars().count());
            for line in rest {
                println!("{continuation}{line}");
            }
        }
    }
}

fn print_columns(values: &[&str]) {
    if values.is_empty() {
        return;
    }
    let width = output_width();
    if width < 80 || values.len() == 1 {
        println!("  {}", values[0]);
        for value in &values[1..] {
            for line in wrap_text(value, width.saturating_sub(6).max(16)) {
                println!("      {line}");
            }
        }
        return;
    }

    let column_widths: &[usize] = match values.len() {
        2 => &[28],
        3 => &[28, 12],
        _ => &[18, 12, 16],
    };
    let mut prefix = String::from("  ");
    for (index, value) in values[..values.len() - 1].iter().enumerate() {
        let column_width = column_widths
            .get(index)
            .copied()
            .unwrap_or(16)
            .max(value.chars().count() + 2);
        prefix.push_str(&format!("{value:<column_width$}"));
    }
    let lines = wrap_text(
        values[values.len() - 1],
        width.saturating_sub(prefix.chars().count()).max(16),
    );
    if let Some((first, rest)) = lines.split_first() {
        println!("{prefix}{first}");
        let continuation = " ".repeat(prefix.chars().count());
        for line in rest {
            println!("{continuation}{line}");
        }
    }
}

fn print_check_line(status: &str, id: &str, message: &str) {
    let terminal_width = output_width();
    if terminal_width < 80 || id.chars().count() > 28 {
        println!("  {} {id}", check_status_label(status));
        for line in wrap_text(message, terminal_width.saturating_sub(6).max(16)) {
            println!("      {line}");
        }
    } else {
        let prefix = format!("  {:<5} {id:<28} ", check_status_label(status));
        let lines = wrap_text(
            message,
            terminal_width
                .saturating_sub(prefix.chars().count())
                .max(16),
        );
        if let Some((first, rest)) = lines.split_first() {
            println!("{prefix}{first}");
            let continuation = " ".repeat(prefix.chars().count());
            for line in rest {
                println!("{continuation}{line}");
            }
        }
    }
}

fn print_next_step(message: &str) {
    println!();
    println!("Next step");
    print_wrapped(message, 2);
}

fn print_paths(title: &str, paths: &[String]) {
    if paths.is_empty() {
        return;
    }
    println!();
    println!("{title}");
    for path in paths {
        println!("  {path}");
    }
}

fn print_details(message: &str) {
    println!();
    println!("Details");
    print_wrapped(message, 2);
}

fn print_check_report(
    status: &str,
    checks: &[(&str, &str, &str)],
    verbose: bool,
    mut context: Vec<(&str, String)>,
) {
    let passed = checks
        .iter()
        .filter(|(status, _, _)| matches!(*status, "pass" | "passed"))
        .count();
    let warnings = checks
        .iter()
        .filter(|(status, _, _)| matches!(*status, "warn" | "warning"))
        .count();
    let failed = checks
        .iter()
        .filter(|(status, _, _)| matches!(*status, "fail" | "failed"))
        .count();
    context.push(("Status", result_status_label(status).to_string()));
    context.push((
        "Checks",
        format!("{passed} passed, {warnings} warnings, {failed} failed"),
    ));
    print_summary(&context);

    println!();
    println!("Checks");
    let visible = checks
        .iter()
        .filter(|(status, _, _)| verbose || !matches!(*status, "pass" | "passed"))
        .collect::<Vec<_>>();
    if visible.is_empty() {
        println!("  All checks passed. Run with --verbose to show each check.");
        return;
    }
    for (status, id, message) in visible {
        if output_width() < 80 {
            println!("  {} {id}", check_status_label(status));
            for line in wrap_text(message, output_width().saturating_sub(4).max(16)) {
                println!("    {line}");
            }
        } else {
            print_check_line(status, id, message);
        }
    }
}

fn output_width() -> usize {
    terminal_size::terminal_size()
        .map(|(terminal_size::Width(width), _)| usize::from(width))
        .unwrap_or(100)
        .clamp(40, 160)
}

fn print_wrapped(message: &str, indent: usize) {
    let prefix = " ".repeat(indent);
    for line in wrap_text(message, output_width().saturating_sub(indent).max(16)) {
        println!("{prefix}{line}");
    }
}

fn wrap_text(message: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in message.lines() {
        let mut current = String::new();
        for word in paragraph.split_whitespace() {
            if !current.is_empty() && current.chars().count() + 1 + word.chars().count() > width {
                lines.push(std::mem::take(&mut current));
            }
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        }
        if !current.is_empty() {
            lines.push(current);
        } else if paragraph.is_empty() {
            lines.push(String::new());
        }
    }
    lines
}

fn check_status_label(status: &str) -> &'static str {
    match status {
        "pass" | "passed" => "PASS",
        "warn" | "warning" => "WARN",
        "fail" | "failed" => "FAIL",
        _ => "INFO",
    }
}

fn result_status_label(status: &str) -> &'static str {
    match status {
        "dry-run" => "Preview only; no files changed",
        "applied" => "Applied",
        "passed" | "pass" | "ok" => "Passed",
        "warning" | "warn" | "degraded" => "Needs attention",
        "failed" | "fail" => "Failed",
        _ => "Completed",
    }
}

fn list_or_none(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(", ")
    }
}

#[derive(Debug, Serialize)]
struct ErrorEnvelope {
    #[serde(rename = "protocolVersion")]
    protocol_version: &'static str,
    status: &'static str,
    #[serde(rename = "traceId", skip_serializing_if = "Option::is_none")]
    trace_id: Option<String>,
    error: crate::error::ErrorBody,
}

pub fn print_error(err: &RainyError, json: bool, trace_id: Option<&str>) {
    let body = err.body();
    if json {
        let envelope = ErrorEnvelope {
            protocol_version: COMMAND_PROTOCOL_VERSION,
            status: "error",
            trace_id: trace_id.map(str::to_string),
            error: body,
        };
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&envelope).expect("serialize error")
        );
    } else {
        eprintln!("Error");
        eprintln!("  Code    {}", body.code);
        let reason = human_error_reason(&body.code, &body.message);
        let mut lines = reason.lines();
        eprintln!("  Reason  {}", lines.next().unwrap_or("unknown error"));
        for line in lines {
            eprintln!("          {}", line.trim_start());
        }
        print_error_report(&body.message);
        if !body.next_steps.is_empty() {
            eprintln!();
            eprintln!("Next steps");
            for command in &body.next_steps {
                eprintln!("  $ {command}");
            }
        } else if let Some(commands) = error_next_steps(&body.code) {
            eprintln!();
            eprintln!("Next steps");
            for command in commands {
                eprintln!("  $ {command}");
            }
        }
    }
}

fn human_error_reason<'a>(code: &str, fallback: &'a str) -> &'a str {
    match code {
        "DOCTOR_FAILED" => "one or more project health checks failed",
        "VERIFY_FAILED" => "one or more verification steps failed",
        "CONFORMANCE_FAILED" => "the pack or plugin does not conform to the Rainy protocol",
        "SCHEMA_VALIDATION_FAILED" => "the document does not match the selected schema",
        "SKILL_DOCTOR_FAILED" => "one or more Skill health checks failed",
        _ => fallback,
    }
}

fn print_error_report(message: &str) {
    let Ok(report) = serde_json::from_str::<serde_json::Value>(message) else {
        return;
    };
    let entries = report
        .get("checks")
        .or_else(|| report.get("steps"))
        .or_else(|| report.get("issues"))
        .and_then(serde_json::Value::as_array);
    let Some(entries) = entries else {
        return;
    };
    let failed = entries
        .iter()
        .filter_map(|entry| {
            let status = entry
                .get("status")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("fail");
            if matches!(status, "pass" | "passed") {
                return None;
            }
            let id = entry
                .get("id")
                .or_else(|| entry.get("path"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("report");
            let message = entry
                .get("message")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("check failed");
            Some((status, id, message))
        })
        .collect::<Vec<_>>();
    if failed.is_empty() {
        return;
    }
    eprintln!();
    eprintln!("Checks");
    for (status, id, message) in failed {
        let terminal_width = output_width();
        if terminal_width < 80 || id.chars().count() > 28 {
            eprintln!("  {} {id}", check_status_label(status));
            for line in wrap_text(message, terminal_width.saturating_sub(6).max(16)) {
                eprintln!("      {line}");
            }
        } else {
            let prefix = format!("  {:<5} {id:<28} ", check_status_label(status));
            let lines = wrap_text(
                message,
                terminal_width
                    .saturating_sub(prefix.chars().count())
                    .max(16),
            );
            if let Some((first, rest)) = lines.split_first() {
                eprintln!("{prefix}{first}");
                let continuation = " ".repeat(prefix.chars().count());
                for line in rest {
                    eprintln!("{continuation}{line}");
                }
            }
        }
    }
}

fn skill_status_label(status: &str) -> &'static str {
    match status {
        "dry-run" => "Preview only; no files changed",
        "applied" => "Applied",
        "configured" => "Already configured",
        "passed" | "ok" => "Healthy",
        "failed" | "degraded" => "Needs attention",
        _ => "Completed",
    }
}

fn skill_profile_label(profile: &str) -> &'static str {
    if profile == "comet" {
        "Complete workflow"
    } else {
        "Rainy only"
    }
}

fn summarize_skill_paths(paths: &[String]) -> Vec<(String, usize)> {
    let mut groups = std::collections::BTreeMap::<String, usize>::new();
    for path in paths {
        let root = if path.starts_with(".agents/skills/") {
            ".agents/skills"
        } else if path.starts_with(".claude/skills/") {
            ".claude/skills"
        } else if path.starts_with(".cursor/skills/") {
            ".cursor/skills"
        } else if path.starts_with(".github/skills/") {
            ".github/skills"
        } else if path.starts_with(".gemini/skills/") {
            ".gemini/skills"
        } else if path.starts_with(".opencode/skills/") {
            ".opencode/skills"
        } else if path.starts_with(".comet/") {
            ".comet"
        } else {
            path
        };
        *groups.entry(root.to_string()).or_default() += 1;
    }
    groups.into_iter().collect()
}

fn error_next_steps(code: &str) -> Option<&'static [&'static str]> {
    match code {
        "CONFIG_NOT_FOUND" => Some(&[
            "rainy skill doctor",
            "rainy new --help",
            "rainy doctor --scope auto",
        ]),
        "PROJECT_TEMPLATE_CONFIG_NOT_FOUND"
        | "PROJECT_TEMPLATE_CONFIG_INVALID"
        | "PROJECT_TEMPLATE_NOT_FOUND" => {
            Some(&["rainy schema validate --help", "rainy new --help"])
        }
        "PROJECT_TEMPLATE_GIT_FAILED"
        | "PROJECT_TEMPLATE_GIT_NOT_AVAILABLE"
        | "PROJECT_TEMPLATE_SOURCE_INVALID" => Some(&["rainy new --help", "rainy new --help"]),
        "LOCK_NOT_FOUND" => Some(&["rainy doctor --verbose", "rainy new --help"]),
        "CAPABILITY_NOT_FOUND" | "CAPABILITY_PROVIDER_INVALID" => {
            Some(&["rainy capability list", "rainy capability explain --help"])
        }
        "REGISTRY_EMPTY" | "PACK_NOT_FOUND" => {
            Some(&["rainy pack list", "rainy pack install --help"])
        }
        "POLICY_APPROVAL_REQUIRED" | "POLICY_DENY_EDIT" | "POLICY_DENY_COMMAND" => {
            Some(&["rainy doctor --verbose"])
        }
        "DOCTOR_FAILED" => Some(&["rainy doctor --verbose"]),
        "VERIFY_FAILED" | "VERIFY_PROFILE_NOT_FOUND" => Some(&[
            "rainy verify --profile local --verbose",
            "rainy verify --profile ci --verbose",
        ]),
        "SCHEMA_VALIDATION_FAILED" | "SCHEMA_NOT_FOUND" => {
            Some(&["rainy schema list", "rainy schema validate --help"])
        }
        "CONFORMANCE_FAILED" | "CONFORMANCE_SOURCE_INVALID" => {
            Some(&["rainy conformance check --help"])
        }
        "PLUGIN_NOT_FOUND" | "PLUGIN_MANIFEST_INVALID" => {
            Some(&["rainy plugin list", "rainy plugin inspect --help"])
        }
        "EXTERNAL_COMMAND_NOT_FOUND" => {
            Some(&["rainy --help", "rainy plugin list", "rainy plugin --help"])
        }
        "CLI_ARGUMENT_INVALID" => Some(&["rainy --help"]),
        "UPDATE_CHECK_FAILED" | "UPDATE_VERIFY_FAILED" => {
            Some(&["rainy self check --verbose", "rainy self update --help"])
        }
        "SKILL_PROFILE_EXISTS" => Some(&[
            "rainy skill status",
            "rainy skill install",
            "rainy skill install --apply",
        ]),
        "SKILL_PROFILE_NOT_FOUND" => Some(&["rainy skill install", "rainy skill install --help"]),
        "SKILL_CUSTOM_NOT_FOUND"
        | "SKILL_CUSTOM_INVALID"
        | "SKILL_CUSTOM_FRONTMATTER_REQUIRED"
        | "SKILL_CUSTOM_FRONTMATTER_INVALID" => {
            Some(&["rainy skill create --help", "rainy skill install --help"])
        }
        "SKILL_INSTALL_SETUP_ALREADY_CONFIGURED" => Some(&[
            "rainy skill status",
            "rainy skill install --help",
            "rainy skill uninstall",
        ]),
        "SKILL_DOCTOR_FAILED" | "SKILL_UPSTREAM_INCOMPLETE" => Some(&[
            "rainy skill status",
            "rainy skill install --apply",
            "rainy skill doctor --verbose",
        ]),
        "SKILL_LAYOUT_CONFLICT"
        | "SKILL_MANAGED_FILES_MODIFIED"
        | "SKILL_UPSTREAM_FILES_MODIFIED" => Some(&[
            "rainy skill status --verbose",
            "rainy skill install --force --apply",
        ]),
        "SKILL_PROFILE_CHANGE_REQUIRES_UNINSTALL" => Some(&[
            "rainy skill uninstall",
            "rainy skill uninstall --apply",
            "rainy skill install",
        ]),
        _ => None,
    }
}
