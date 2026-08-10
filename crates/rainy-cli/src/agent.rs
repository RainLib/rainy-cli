use crate::cli::{AgentCommand, AgentSubcommand};
use crate::config;
use crate::error::RainyResult;
use crate::output::CommandOutput;
use std::path::Path;

const RAINY_CONTEXT_START: &str = "<!-- rainy:context:start -->";
const RAINY_CONTEXT_END: &str = "<!-- rainy:context:end -->";

pub fn handle_agent_command(workspace: &Path, command: AgentCommand) -> RainyResult<CommandOutput> {
    match command.command {
        AgentSubcommand::Init(args) => {
            if args.dry_run && args.apply {
                return Err(crate::error::RainyError::config(
                    "APPLY_MODE_CONFLICT",
                    "--dry-run and --apply cannot be used together",
                ));
            }
            let context = build_context(workspace)?;
            if !args.apply {
                return Ok(CommandOutput::Message {
                    status: "dry-run",
                    message: format!("Would refresh {}", skill_sync_paths(workspace).join(", ")),
                });
            }
            write_agent_context(workspace, &context)?;
            write_enterprise_agent_files(workspace, &context)?;
            Ok(CommandOutput::Message {
                status: "applied",
                message: "Generated AGENTS.md and .enterprise-agent context".to_string(),
            })
        }
        AgentSubcommand::Context => Ok(CommandOutput::AgentContext {
            context: build_context(workspace)?,
        }),
    }
}

pub fn sync_skills_command(workspace: &Path) -> RainyResult<CommandOutput> {
    let complete_rainy_project =
        workspace.join("rainy.yaml").is_file() && workspace.join("capability.lock").is_file();
    if !complete_rainy_project && workspace.join("rainy-skills.yaml").is_file() {
        let context = build_skill_context(workspace)?;
        write_agent_context(workspace, &context)?;
        return Ok(CommandOutput::message(
            "Synced Rainy agent skills and standalone project context",
        ));
    }

    let context = build_context(workspace)?;
    write_agent_context(workspace, &context)?;
    write_enterprise_agent_files(workspace, &context)?;
    Ok(CommandOutput::message(
        "Synced Rainy agent skills and context",
    ))
}

pub fn skill_sync_paths(workspace: &Path) -> Vec<String> {
    let mut paths = vec!["AGENTS.md".to_string()];
    if workspace.join("rainy.yaml").is_file() && workspace.join("capability.lock").is_file() {
        paths.extend(
            [
                ".enterprise-agent/context.md",
                ".enterprise-agent/capabilities.md",
                ".enterprise-agent/commands.md",
            ]
            .into_iter()
            .map(str::to_string),
        );
    }
    paths
}

fn build_skill_context(workspace: &Path) -> RainyResult<String> {
    let mut out = String::from(
        "# AGENTS.md\n\n## Project Rules\n- Use installed project Skills for repository-specific guidance.\n- Review Skill scripts before running them.\n- Require explicit approval before mutating protected resources.\n\n",
    );
    if let Some(summary) = crate::skills::context_summary(workspace)? {
        out.push_str("## Skill Workflow\n");
        out.push_str(&summary);
        out.push('\n');
    }
    Ok(out)
}

fn build_context(workspace: &Path) -> RainyResult<String> {
    let config = config::load_config(workspace)?;
    let lock = config::load_lock(workspace)?;
    let mut out = String::new();
    out.push_str("# AGENTS.md\n\n");
    out.push_str("## Project Rules\n");
    out.push_str("- Use Rainy CLI for capability changes.\n");
    out.push_str("- Prefer `--dry-run` before `--apply`.\n");
    out.push_str("- Keep `capability.lock` in sync with generated artifacts.\n\n");
    out.push_str("## Installed Capabilities\n");
    for id in lock.capabilities.keys() {
        out.push_str(&format!("- {id}\n"));
    }
    out.push_str("\n## Commands\n");
    out.push_str("- `rainy capability list`\n");
    out.push_str("- `rainy doctor`\n");
    out.push_str("- `rainy verify --profile local`\n");
    out.push_str("- `rainy verify --profile ci`\n");
    out.push_str("- `rainy evidence generate --apply`\n\n");
    if let Some(summary) = crate::skills::context_summary(workspace)? {
        out.push_str("## Skill Workflow\n");
        out.push_str(&summary);
        out.push('\n');
    }
    out.push_str("## Capability Usage\n");
    out.push_str(&format!(
        "Use Rainy packs before manually wiring common infrastructure in {}.\n",
        config.project.name
    ));
    Ok(out)
}

fn write_agent_context(workspace: &Path, context: &str) -> RainyResult<()> {
    let path = workspace.join("AGENTS.md");
    let managed = format!(
        "{RAINY_CONTEXT_START}\n{}\n{RAINY_CONTEXT_END}",
        context.trim()
    );
    let existing = if path.is_file() {
        std::fs::read_to_string(&path)?
    } else {
        String::new()
    };
    let merged = merge_managed_context(&existing, &managed);
    std::fs::write(path, merged)?;
    Ok(())
}

fn merge_managed_context(existing: &str, managed: &str) -> String {
    if let Some(start) = existing.find(RAINY_CONTEXT_START)
        && let Some(relative_end) = existing[start..].find(RAINY_CONTEXT_END)
    {
        let end = start + relative_end + RAINY_CONTEXT_END.len();
        return format!("{}{}{}", &existing[..start], managed, &existing[end..]);
    }
    if existing.trim().is_empty() {
        return format!("{managed}\n");
    }
    format!("{}\n\n{managed}\n", existing.trim_end())
}

fn write_enterprise_agent_files(workspace: &Path, context: &str) -> RainyResult<()> {
    let dir = workspace.join(".enterprise-agent");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("context.md"), context)?;

    let lock = config::load_lock(workspace)?;
    let mut capabilities = String::from("# Capabilities\n\n");
    for (id, capability) in &lock.capabilities {
        capabilities.push_str(&format!(
            "- `{}` {} from `{}`\n",
            id, capability.version, capability.pack
        ));
    }
    std::fs::write(dir.join("capabilities.md"), capabilities)?;

    let commands = r#"# Commands

- Backend test: `cd apps/backend && ./mvnw test`
- Frontend build: `cd apps/frontend && pnpm build`
- Project health: `rainy doctor`
- Local verification: `rainy verify --profile local`
- CI verification: `rainy verify --profile ci`
- Evidence: `rainy evidence generate --apply`
"#;
    std::fs::write(dir.join("commands.md"), commands)?;
    Ok(())
}
