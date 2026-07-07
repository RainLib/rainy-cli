use crate::cli::{AgentCommand, AgentSubcommand};
use crate::config;
use crate::error::RainyResult;
use crate::output::CommandOutput;
use std::path::Path;

pub fn handle_agent_command(workspace: &Path, command: AgentCommand) -> RainyResult<CommandOutput> {
    match command.command {
        AgentSubcommand::Init => {
            let context = build_context(workspace)?;
            std::fs::write(workspace.join("AGENTS.md"), &context)?;
            write_enterprise_agent_files(workspace, &context)?;
            Ok(CommandOutput::message(
                "Generated AGENTS.md and .enterprise-agent context",
            ))
        }
        AgentSubcommand::Context => Ok(CommandOutput::AgentContext {
            context: build_context(workspace)?,
        }),
    }
}

pub fn sync_skills_command(workspace: &Path) -> RainyResult<CommandOutput> {
    let context = build_context(workspace)?;
    std::fs::write(workspace.join("AGENTS.md"), &context)?;
    write_enterprise_agent_files(workspace, &context)?;
    Ok(CommandOutput::message(
        "Synced Rainy agent skills and context",
    ))
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
    out.push_str("- `rainy evidence generate`\n\n");
    out.push_str("## Capability Usage\n");
    out.push_str(&format!(
        "Use Rainy packs before manually wiring common infrastructure in {}.\n",
        config.project.name
    ));
    Ok(out)
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
- Evidence: `rainy evidence generate`
"#;
    std::fs::write(dir.join("commands.md"), commands)?;
    Ok(())
}
