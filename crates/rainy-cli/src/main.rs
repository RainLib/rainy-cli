mod actions;
mod agent;
mod audit;
mod cli;
mod config;
mod conformance;
mod doctor;
mod error;
mod evidence;
mod init;
mod output;
mod patch;
mod plugin;
mod policy;
mod registry;
mod schema;
mod verify;

use clap::Parser;
use cli::{
    AddSubcommand, CapabilitySubcommand, Cli, Commands, EvidenceFormat, EvidenceSubcommand,
    InitSubcommand, SkillSubcommand,
};
use error::{RainyError, RainyResult};
use output::CommandOutput;
use std::path::{Path, PathBuf};

fn main() {
    let cli = Cli::parse();
    let json = cli.json;
    let trace_id = cli.trace_id.clone();
    let audit_workspace = cli
        .workspace
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let audit_command = command_label(&cli.command).to_string();

    match run(cli) {
        Ok(output) => {
            let _ = audit::record_success(
                &audit_workspace,
                &audit_command,
                trace_id.as_deref(),
                &output,
            );
            output.print(json);
        }
        Err(err) => {
            let _ =
                audit::record_error(&audit_workspace, &audit_command, trace_id.as_deref(), &err);
            output::print_error(&err, json);
            std::process::exit(err.exit_code());
        }
    }
}

fn command_label(command: &Commands) -> &'static str {
    match command {
        Commands::Init(_) => "init",
        Commands::New(_) => "new",
        Commands::Add(_) => "add capability",
        Commands::Apply(_) => "apply",
        Commands::Capability(_) => "capability",
        Commands::Pack(_) => "pack",
        Commands::Doctor(_) => "doctor",
        Commands::Verify(_) => "verify",
        Commands::Evidence(_) => "evidence",
        Commands::Plugin(_) => "plugin",
        Commands::Agent(_) => "agent",
        Commands::Skill(_) => "skill",
        Commands::Conformance(_) => "conformance",
        Commands::Schema(_) => "schema",
        Commands::External(_) => "external",
    }
}

fn run(cli: Cli) -> RainyResult<CommandOutput> {
    let workspace = cli.workspace.unwrap_or(std::env::current_dir()?);

    match cli.command {
        Commands::Init(command) => match command.command {
            InitSubcommand::App(args) => init::init_app(init::InitOptions {
                base_dir: workspace,
                name: args.name,
                package: args.package,
                preset: args.preset,
                golden_path: Some("spring-nextjs-saas".to_string()),
                dry_run: resolve_init_mode(args.dry_run, args.apply)?,
            }),
        },
        Commands::New(args) => init::init_app(init::InitOptions {
            base_dir: workspace,
            name: args.name,
            package: args
                .package
                .unwrap_or_else(|| "com.example.demo".to_string()),
            preset: Some("spring-nextjs".to_string()),
            golden_path: Some(args.golden_path),
            dry_run: resolve_init_mode(args.dry_run, args.apply)?,
        }),
        Commands::Add(command) => match command.command {
            AddSubcommand::Capability(args) => add_capability(&workspace, args),
        },
        Commands::Apply(args) => apply_plan_command(&workspace, args),
        Commands::Capability(command) => match command.command {
            CapabilitySubcommand::List => registry::capability_list(&workspace),
            CapabilitySubcommand::Explain { id } => registry::capability_explain(&workspace, &id),
            CapabilitySubcommand::Installed => config::capability_installed(&workspace),
            CapabilitySubcommand::Graph => registry::capability_graph(&workspace),
            CapabilitySubcommand::Upgrade(args) => upgrade_capability(&workspace, args),
            CapabilitySubcommand::Remove(args) => remove_capability(&workspace, args),
        },
        Commands::Pack(command) => registry::handle_pack_command(&workspace, command),
        Commands::Doctor(args) => doctor::doctor_command(&workspace, args.capability.as_deref()),
        Commands::Verify(args) => {
            verify::verify_command(&workspace, &args.profile, args.capability.as_deref())
        }
        Commands::Evidence(args) => {
            let format = match args.command {
                Some(EvidenceSubcommand::Generate(generate)) => generate.format.or(args.format),
                None => args.format,
            }
            .unwrap_or(EvidenceFormat::All);
            evidence::generate_command(&workspace, format)
        }
        Commands::Plugin(command) => plugin::handle_plugin_command(&workspace, command),
        Commands::Agent(command) => agent::handle_agent_command(&workspace, command),
        Commands::Skill(command) => match command.command {
            SkillSubcommand::Sync => agent::sync_skills_command(&workspace),
        },
        Commands::Conformance(command) => conformance::handle_conformance_command(command),
        Commands::Schema(command) => schema::handle_schema_command(command),
        Commands::External(args) => plugin::run_external(&workspace, args),
    }
}

fn resolve_init_mode(dry_run: bool, apply: bool) -> RainyResult<bool> {
    if dry_run && apply {
        return Err(RainyError::config(
            "APPLY_MODE_CONFLICT",
            "--dry-run and --apply cannot be used together",
        ));
    }
    Ok(dry_run)
}

fn add_capability(workspace: &Path, args: cli::AddCapabilityArgs) -> RainyResult<CommandOutput> {
    let apply = resolve_apply_flags(args.dry_run, args.apply)?;
    let result = if let Some(plan_path) = args.plan {
        let plan = read_plan(&plan_path)?;
        if plan.capability != args.id {
            return Err(RainyError::plan(
                "PLAN_CAPABILITY_MISMATCH",
                format!(
                    "plan capability {} does not match requested capability {}",
                    plan.capability, args.id
                ),
            ));
        }
        actions::plan_from_execution_plan(workspace, plan, args.force)?
    } else {
        let request = actions::AddCapabilityRequest {
            capability_id: args.id,
            provider: args.provider,
            force: args.force,
        };
        actions::plan_add_capability(workspace, request)?
    };

    if let Some(path) = args.output_plan {
        write_json(&path, &result.plan)?;
    }

    finish_capability_changes(workspace, result, apply)
}

fn apply_plan_command(workspace: &Path, args: cli::ApplyCommand) -> RainyResult<CommandOutput> {
    let apply = resolve_apply_flags(args.dry_run, args.apply)?;
    let plan = read_plan(&args.plan)?;
    let result = actions::plan_from_execution_plan(workspace, plan, args.force)?;
    finish_capability_changes(workspace, result, apply)
}

fn upgrade_capability(
    workspace: &Path,
    args: cli::CapabilityChangeArgs,
) -> RainyResult<CommandOutput> {
    let apply = resolve_apply_flags(args.dry_run, args.apply)?;
    let result = actions::plan_upgrade_capability(workspace, &args.id, args.force)?;
    if let Some(path) = args.output_plan {
        write_json(&path, &result.plan)?;
    }
    finish_capability_changes(workspace, result, apply)
}

fn remove_capability(
    workspace: &Path,
    args: cli::CapabilityChangeArgs,
) -> RainyResult<CommandOutput> {
    let apply = resolve_apply_flags(args.dry_run, args.apply)?;
    let result = actions::plan_remove_capability(workspace, &args.id)?;
    if let Some(path) = args.output_plan {
        write_json(&path, &result.plan)?;
    }
    finish_capability_changes(workspace, result, apply)
}

fn resolve_apply_flags(dry_run: bool, apply: bool) -> RainyResult<bool> {
    if dry_run && apply {
        return Err(RainyError::plan(
            "APPLY_MODE_CONFLICT",
            "--dry-run and --apply cannot be used together",
        ));
    }
    Ok(apply)
}

fn finish_capability_changes(
    workspace: &Path,
    result: actions::CapabilityOutcome,
    apply: bool,
) -> RainyResult<CommandOutput> {
    if apply {
        policy::check_plan(workspace, &result.plan)?;
        policy::check_plan_changes(workspace, &result.plan, &result.changes)?;
        patch::apply_changes(workspace, &result.changes)?;
        Ok(CommandOutput::applied(result))
    } else {
        Ok(CommandOutput::dry_run(result))
    }
}

fn read_plan(path: &Path) -> RainyResult<actions::ExecutionPlan> {
    let content = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

fn write_json(path: &PathBuf, value: &impl serde::Serialize) -> RainyResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(value)?;
    std::fs::write(path, format!("{content}\n"))?;
    Ok(())
}
