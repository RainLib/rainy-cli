mod actions;
mod agent;
mod audit;
mod bundled_assets;
mod cli;
mod config;
mod conformance;
mod defaults;
mod doctor;
mod error;
mod evidence;
mod init;
mod output;
mod patch;
mod plugin;
mod policy;
mod progress;
mod registry;
mod schema;
mod skills;
mod update;
mod verify;

use clap::Parser;
use cli::{
    AddSubcommand, CapabilitySubcommand, Cli, Commands, EvidenceFormat, EvidenceSubcommand,
    InitSubcommand,
};
use error::{RainyError, RainyResult};
use output::CommandOutput;
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};

fn main() {
    let cli = Cli::parse();
    let json = cli.json;
    let verbose = cli.verbose;
    let no_color = cli.no_color;
    let interactive =
        !cli.json && !cli.quiet && io::stdin().is_terminal() && io::stderr().is_terminal();
    let prompt_before_progress = interactive && skill_command_needs_prompt(&cli.command);
    let trace_id = cli.trace_id.clone();
    let audit_workspace = cli
        .workspace
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let audit_command = command_label(&cli.command).to_string();
    let is_self_command = matches!(cli.command, Commands::SelfCommand(_));
    update::maybe_notify(json, cli.quiet, is_self_command);
    let progress_mode = if prompt_before_progress {
        progress::ProgressMode::Never
    } else {
        cli.progress
    };
    let mut progress =
        progress::ProgressReporter::new(progress_mode, cli.json, cli.quiet, cli.no_color);
    progress.stage(format!("Preparing {audit_command}"));

    if command_requires_audit(&cli.command)
        && let Err(err) = audit::preflight(&audit_workspace)
    {
        progress.finish_error();
        output::print_error(&err, json);
        std::process::exit(err.exit_code());
    }

    progress.stage(format!("Running {audit_command}"));

    match run(cli, &progress, interactive, no_color) {
        Ok(output) => {
            progress.stage("Recording command result");
            if let Err(err) = audit::record_success(
                &audit_workspace,
                &audit_command,
                trace_id.as_deref(),
                &output,
            ) {
                progress.finish_error();
                output::print_error(&err, json);
                std::process::exit(err.exit_code());
            }
            progress.stage("Rendering output");
            progress.finish_success();
            output.print(json, verbose);
        }
        Err(err) => {
            let _ =
                audit::record_error(&audit_workspace, &audit_command, trace_id.as_deref(), &err);
            progress.finish_error();
            output::print_error(&err, json);
            std::process::exit(err.exit_code());
        }
    }
}

fn skill_command_needs_prompt(command: &Commands) -> bool {
    let Commands::Skill(command) = command else {
        return false;
    };
    match &command.command {
        cli::SkillSubcommand::Init(args) => {
            args.profile.is_none() || args.target.is_empty() || (!args.apply && !args.dry_run)
        }
        cli::SkillSubcommand::Install(args) => !args.apply && !args.dry_run,
        _ => false,
    }
}

fn command_requires_audit(command: &Commands) -> bool {
    match command {
        Commands::Add(command) => match &command.command {
            AddSubcommand::Capability(args) => args.apply,
        },
        Commands::Apply(args) => args.apply,
        Commands::Capability(command) => match &command.command {
            CapabilitySubcommand::Upgrade(args) | CapabilitySubcommand::Remove(args) => args.apply,
            _ => false,
        },
        Commands::Pack(command) => match &command.command {
            cli::PackSubcommand::Install(args) => args.apply,
            cli::PackSubcommand::Update(args) => args.apply,
            cli::PackSubcommand::Sign(_) => true,
            _ => false,
        },
        Commands::Registry(command) => match &command.command {
            cli::RegistrySubcommand::Add(args) => args.apply,
            cli::RegistrySubcommand::Sync(args) => args.apply,
            cli::RegistrySubcommand::Remove(args) => args.apply,
            _ => false,
        },
        Commands::Defaults(_) => false,
        Commands::Plugin(command) => match &command.command {
            cli::PluginSubcommand::Install(args) => args.apply,
            cli::PluginSubcommand::Call(args) => args.apply,
            _ => false,
        },
        Commands::Evidence(_) | Commands::Agent(_) | Commands::Skill(_) | Commands::External(_) => {
            true
        }
        _ => false,
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
        Commands::Registry(_) => "registry",
        Commands::Defaults(_) => "defaults",
        Commands::Doctor(_) => "doctor",
        Commands::Verify(_) => "verify",
        Commands::Evidence(_) => "evidence",
        Commands::Plugin(_) => "plugin",
        Commands::Agent(_) => "agent",
        Commands::Skill(_) => "skill",
        Commands::Conformance(_) => "conformance",
        Commands::Schema(_) => "schema",
        Commands::SelfCommand(_) => "self",
        Commands::External(_) => "external",
    }
}

fn run(
    cli: Cli,
    progress: &progress::ProgressReporter,
    interactive: bool,
    no_color: bool,
) -> RainyResult<CommandOutput> {
    let workspace = cli.workspace.unwrap_or(std::env::current_dir()?);
    let allow_native_plugin = cli.allow_native_plugin
        || config::load_config(&workspace)
            .map(|config| config.policy.allow_native_plugins)
            .unwrap_or(false);

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
        Commands::Registry(command) => registry::handle_registry_command(&workspace, command),
        Commands::Defaults(command) => defaults::handle_defaults_command(command),
        Commands::Doctor(args) => {
            doctor::doctor_command(&workspace, args.capability.as_deref(), progress)
        }
        Commands::Verify(args) => verify::verify_command(
            &workspace,
            &args.profile,
            args.capability.as_deref(),
            progress,
        ),
        Commands::Evidence(args) => {
            let format = match args.command {
                Some(EvidenceSubcommand::Generate(generate)) => generate.format.or(args.format),
                None => args.format,
            }
            .unwrap_or(EvidenceFormat::All);
            evidence::generate_command(&workspace, format)
        }
        Commands::Plugin(command) => {
            plugin::handle_plugin_command(&workspace, command, allow_native_plugin)
        }
        Commands::Agent(command) => agent::handle_agent_command(&workspace, command),
        Commands::Skill(command) => {
            skills::handle_skill_command(&workspace, command, progress, interactive, no_color)
        }
        Commands::Conformance(command) => conformance::handle_conformance_command(command),
        Commands::Schema(command) => schema::handle_schema_command(command),
        Commands::SelfCommand(command) => update::handle_self_command(command),
        Commands::External(args) => plugin::run_external(&workspace, args, allow_native_plugin),
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
