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
    Registry {
        report: crate::registry::RegistryReport,
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
    Skill {
        report: crate::skills::SkillReport,
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
            Self::Registry { .. } => "registry",
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
            Self::Skill { report } => &report.status,
            Self::Registry { report } => &report.status,
            Self::Defaults { report } => &report.status,
            Self::Update { .. } => "ok",
            _ => "ok",
        }
    }

    pub fn is_dry_run(&self) -> bool {
        match self {
            Self::DryRun { .. }
            | Self::ChangeDryRun { .. }
            | Self::Init {
                status: "dry-run", ..
            } => true,
            Self::Skill { report } => report.status == "dry-run",
            _ => false,
        }
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
            Self::Registry { report } => {
                format!("registry {} {}", report.operation, report.status)
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
        }
    }

    pub fn print(&self, json: bool, verbose: bool) {
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(self).expect("serialize command output")
            );
            return;
        }

        match self {
            Self::Message { message, .. } => {
                print_title("Rainy");
                print_summary(&[
                    ("Status", "Completed".to_string()),
                    ("Result", message.clone()),
                ]);
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
                    "Review the plan and diff, then apply the saved plan with rainy apply --plan <PLAN_FILE> --apply.",
                );
                println!();
                println!("Actions");
                for action in &plan.actions {
                    println!("  {:<24} {}", action.id, action.uses);
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
                    println!(
                        "  {:<28} {:<12} {}",
                        capability.id, capability.version, capability.description
                    );
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
                    println!(
                        "  {:<28} {:<12} {}",
                        capability.id,
                        capability.version,
                        capability.provider.as_deref().unwrap_or("-")
                    );
                }
            }
            Self::Packs { packs } => {
                print_title("Capability packs");
                print_summary(&[("Available", packs.len().to_string())]);
                println!();
                println!("Items");
                for pack in packs {
                    println!("  {:<28} {:<12} {}", pack.name, pack.version, pack.path);
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
                        println!(
                            "  {:<18} {:<8} priority {:<4} {}",
                            registry.name, registry.source_type, registry.priority, registry.source
                        );
                        if let Some(resolved) = &registry.resolved_ref {
                            println!("    resolved {resolved}");
                        }
                        if !registry.modules.is_empty() {
                            println!("    modules  {}", registry.modules.join(", "));
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
                        println!("  {:<5} {:<28} {}", check_status_label(status), id, message);
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
            Self::Evidence { files, .. } => {
                print_title("Evidence generation");
                print_summary(&[
                    ("Status", "Completed".to_string()),
                    ("Files", files.len().to_string()),
                ]);
                print_paths("Affected locations", files);
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
                        println!("  {:<28} {}", plugin.name, plugin.path);
                        if !plugin.shadowed_paths.is_empty() {
                            println!(
                                "    WARN shadowed duplicate plugin(s): {}",
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
                    println!("  {:<28} {}", schema.name, schema.path);
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
                        println!("  {:<24} {}", issue.path, issue.message);
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
                        println!("  {:<5} {:<28} {}", check.status, check.id, check.message);
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
                if report.update_available {
                    print_summary(&[
                        (
                            "Status",
                            if report.skipped {
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
                    if report.skipped {
                        print_details("The latest version is currently skipped.");
                    } else {
                        print_next_step(
                            "Run rainy self update to install and verify the latest release.",
                        );
                    }
                } else {
                    print_summary(&[
                        ("Status", "Up to date".to_string()),
                        ("Current", report.current_version.clone()),
                        ("Release", report.release_type.clone()),
                    ]);
                }
                if verbose {
                    print_details(&format!("Install command: {}", report.install_command));
                }
            }
        }
    }
}

fn print_title(title: &str) {
    println!("{title}");
    println!();
}

fn print_summary(rows: &[(&str, String)]) {
    println!("Summary");
    for (label, value) in rows {
        println!("  {label:<14}{value}");
    }
}

fn print_next_step(message: &str) {
    println!();
    println!("Next step");
    println!("  {message}");
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
    println!("  {message}");
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
        println!("  {:<5} {:<28} {}", check_status_label(status), id, message);
    }
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
        eprintln!("Error");
        eprintln!("  Code    {}", body.code);
        eprintln!(
            "  Reason  {}",
            human_error_reason(&body.code, &body.message)
        );
        print_error_report(&body.message);
        if let Some(commands) = error_next_steps(&body.code) {
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
        eprintln!("  {:<5} {:<28} {}", check_status_label(status), id, message);
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
        "CONFIG_NOT_FOUND" => Some(&["rainy new --help", "rainy --workspace <PROJECT_DIR> doctor"]),
        "LOCK_NOT_FOUND" => Some(&["rainy doctor --verbose", "rainy new --help"]),
        "CAPABILITY_NOT_FOUND" | "CAPABILITY_PROVIDER_INVALID" => Some(&[
            "rainy capability list",
            "rainy capability explain <CAPABILITY_ID>",
        ]),
        "REGISTRY_EMPTY" | "PACK_NOT_FOUND" => Some(&[
            "rainy pack list",
            "rainy pack install <PACK_SOURCE> --dry-run",
        ]),
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
            Some(&["rainy conformance check --path <PATH> --verbose"])
        }
        "PLUGIN_NOT_FOUND" | "PLUGIN_MANIFEST_INVALID" => {
            Some(&["rainy plugin list", "rainy plugin inspect <PLUGIN_ID>"])
        }
        "UPDATE_CHECK_FAILED" | "UPDATE_VERIFY_FAILED" => {
            Some(&["rainy self check --verbose", "rainy self update --help"])
        }
        "SKILL_PROFILE_EXISTS" => Some(&[
            "rainy skill status",
            "rainy skill install",
            "rainy skill install --apply",
        ]),
        "SKILL_PROFILE_NOT_FOUND" => Some(&["rainy skill init", "rainy skill init --help"]),
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
            "rainy skill init",
        ]),
        _ => None,
    }
}
