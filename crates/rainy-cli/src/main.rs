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
mod paths;
mod plugin;
mod policy;
mod process;
mod progress;
mod project_template;
mod redaction;
mod registry;
mod runtime;
mod schema;
mod security;
mod skills;
mod source;
mod update;
mod verify;
mod workspace;

use clap::{CommandFactory, Parser, error::ErrorKind};
use cli::{
    AddSubcommand, CapabilitySubcommand, Cli, Commands, EvidenceFormat, EvidenceSubcommand,
    InitSubcommand,
};
use error::{RainyError, RainyResult};
use output::CommandOutput;
use std::path::{Path, PathBuf};

fn main() {
    let mut cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            try_parse_installed_plugin(&error).unwrap_or_else(|| handle_cli_parse_error(error))
        }
    };
    let json = cli.json;
    if let Err(error) = validate_trace_id(cli.trace_id.as_deref()) {
        output::print_error(&error, json, cli.trace_id.as_deref());
        std::process::exit(error.exit_code());
    }
    if let Err(error) = progress::install_interrupt_handler() {
        let error = RainyError::action(
            "INTERRUPT_HANDLER_UNAVAILABLE",
            format!("unable to install Ctrl+C handler: {error}"),
        );
        output::print_error(&error, json, cli.trace_id.as_deref());
        std::process::exit(1);
    }
    let verbose = cli.verbose;
    let no_color = color_disabled(cli.no_color);
    let output_mode = if cli.json {
        runtime::OutputMode::Json
    } else if cli.quiet {
        runtime::OutputMode::Quiet
    } else {
        runtime::OutputMode::Human
    };
    let terminal = runtime::TerminalCapabilities::detect(no_color, output_mode);
    let marker = workspace_marker(&cli.command);
    let resolved_workspace = match workspace::resolve(cli.workspace.take(), marker) {
        Ok(workspace) => workspace,
        Err(error) => {
            let error = RainyError::from(error);
            output::print_error(&error, json, cli.trace_id.as_deref());
            std::process::exit(error.exit_code());
        }
    };
    cli.workspace = Some(resolved_workspace.clone());
    let audit_workspace = resolved_workspace;
    let trace_id = cli.trace_id.clone();
    let audit_command = command_label(&cli.command).to_string();
    let audit_required = command_requires_audit(&cli.command, terminal.interactive);
    let is_self_command = matches!(cli.command, Commands::SelfCommand(_));
    let is_completion_command = matches!(cli.command, Commands::Completion(_));
    let implicit_defaults_fetch =
        command_uses_default_content(&cli.command) && defaults::implicit_fetch_required();
    update::maybe_notify(json, cli.quiet || is_completion_command, is_self_command);
    let progress_mode = if is_completion_command
        || (matches!(cli.progress, progress::ProgressMode::Auto)
            && !command_benefits_from_progress(&cli.command)
            && !implicit_defaults_fetch)
    {
        progress::ProgressMode::Never
    } else {
        cli.progress
    };
    let progress = progress::ProgressReporter::new(progress_mode, cli.json, cli.quiet, no_color);
    let context = runtime::RunContext::new(
        audit_workspace.clone(),
        output_mode,
        terminal,
        trace_id.clone(),
        &progress,
    );
    progress.stage(format!("Preparing {audit_command}"));

    if audit_required && let Err(err) = audit::preflight(&audit_workspace) {
        progress.finish_error();
        output::print_error(&err, json, trace_id.as_deref());
        std::process::exit(err.exit_code());
    }

    progress.stage(format!("Running {audit_command}"));

    let result = run(cli, &context);
    if progress::cancelled() {
        progress.finish_cancelled();
        let error = RainyError::action("CANCELLED", "command cancelled by user");
        output::print_error(&error, json, trace_id.as_deref());
        std::process::exit(130);
    }

    match result {
        Ok(output) => {
            progress.stage("Recording command result");
            if audit_required
                && let Err(err) = audit::record_success(
                    &audit_workspace,
                    &audit_command,
                    trace_id.as_deref(),
                    &output,
                )
            {
                progress.finish_error();
                output::print_error(&err, json, trace_id.as_deref());
                std::process::exit(err.exit_code());
            }
            progress.stage("Rendering output");
            progress.finish_success();
            let exit_code = output.exit_code();
            output.print(json, verbose, trace_id.as_deref());
            if exit_code != 0 {
                std::process::exit(exit_code);
            }
        }
        Err(err) => {
            if audit_required {
                let _ = audit::record_error(
                    &audit_workspace,
                    &audit_command,
                    trace_id.as_deref(),
                    &err,
                );
            }
            progress.finish_error();
            output::print_error(&err, json, trace_id.as_deref());
            std::process::exit(err.exit_code());
        }
    }
}

fn try_parse_installed_plugin(error: &clap::Error) -> Option<Cli> {
    if error.kind() != ErrorKind::InvalidSubcommand {
        return None;
    }
    let rendered = error.to_string();
    let command = rendered.lines().find_map(|line| {
        line.strip_prefix("error: unrecognized subcommand '")
            .and_then(|line| line.split_once('\'').map(|(command, _)| command))
    })?;
    let mut arguments = std::env::args_os().collect::<Vec<_>>();
    let position = arguments
        .iter()
        .position(|argument| argument.to_str() == Some(command))?;
    let explicit_workspace = arguments.iter().enumerate().find_map(|(index, argument)| {
        let value = argument.to_string_lossy();
        value
            .strip_prefix("--workspace=")
            .map(PathBuf::from)
            .or_else(|| {
                (value == "--workspace")
                    .then(|| arguments.get(index + 1).map(PathBuf::from))
                    .flatten()
            })
    });
    let workspace =
        workspace::resolve(explicit_workspace, workspace::WorkspaceMarker::Either).ok()?;
    if !plugin::external_command_exists(&workspace, command) {
        return None;
    }
    arguments.insert(position, "external".into());
    Cli::try_parse_from(arguments).ok()
}

fn handle_cli_parse_error(error: clap::Error) -> ! {
    if error.kind() == ErrorKind::MissingSubcommand {
        print_current_command_help();
        std::process::exit(0);
    }
    if matches!(
        error.kind(),
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
    ) {
        print!("{error}");
        std::process::exit(0);
    }

    let rendered = error.to_string();
    if !rendered.lines().any(|line| line.starts_with("error: ")) {
        print!("{rendered}");
        std::process::exit(0);
    }
    let reason = clap_reason(&rendered);
    let json = std::env::args_os().any(|argument| argument == "--json");
    let parse_error = RainyError::config("CLI_ARGUMENT_INVALID", reason);
    output::print_error(&parse_error, json, None);
    if !json {
        if let Some(usage) = clap_usage(&rendered) {
            eprintln!();
            eprintln!("Usage");
            eprintln!("  {usage}");
        }
        eprintln!("  Run 'rainy --help', or append '--help' to the current command path.");
    }
    std::process::exit(2);
}

fn print_current_command_help() {
    let mut command = Cli::command();
    for argument in std::env::args().skip(1) {
        if let Some(subcommand) = command.find_subcommand(&argument).cloned() {
            command = subcommand;
        }
    }
    let _ = command.print_long_help();
    println!();
}

fn clap_reason(rendered: &str) -> String {
    let mut collecting = false;
    let mut lines = Vec::new();
    for line in rendered.lines() {
        if let Some(first) = line.strip_prefix("error: ") {
            collecting = true;
            lines.push(first.to_string());
            continue;
        }
        if collecting {
            if line.trim_start().starts_with("Usage:") {
                break;
            }
            if !line.trim().is_empty() {
                lines.push(line.trim_end().to_string());
            }
        }
    }
    if lines.is_empty() {
        rendered
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("invalid command-line input")
            .to_string()
    } else {
        lines.join("\n")
    }
}

fn validate_trace_id(trace_id: Option<&str>) -> RainyResult<()> {
    let Some(trace_id) = trace_id else {
        return Ok(());
    };
    if trace_id.is_empty()
        || trace_id.len() > 64
        || !trace_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':'))
    {
        return Err(RainyError::config(
            "TRACE_ID_INVALID",
            "--trace-id must contain 1-64 ASCII letters, digits, '.', '-', '_', or ':'",
        ));
    }
    Ok(())
}

fn workspace_marker(command: &Commands) -> workspace::WorkspaceMarker {
    use workspace::WorkspaceMarker;
    match command {
        Commands::New(_)
        | Commands::Init(_)
        | Commands::Defaults(_)
        | Commands::Schema(_)
        | Commands::Conformance(_)
        | Commands::SelfCommand(_)
        | Commands::Completion(_) => WorkspaceMarker::None,
        Commands::Source(command) => match &command.command {
            cli::SourceSubcommand::Check(args) if args.project => WorkspaceMarker::Project,
            cli::SourceSubcommand::Sync(args) | cli::SourceSubcommand::Update(args)
                if args.selection.project =>
            {
                WorkspaceMarker::Project
            }
            _ => WorkspaceMarker::None,
        },
        Commands::Skill(_) => WorkspaceMarker::Skills,
        Commands::Doctor(_) | Commands::Plugin(_) | Commands::External(_) => {
            WorkspaceMarker::Either
        }
        _ => WorkspaceMarker::Project,
    }
}

fn clap_usage(rendered: &str) -> Option<String> {
    rendered
        .lines()
        .find_map(|line| line.trim_start().strip_prefix("Usage: "))
        // Environment-backed global flags are already in effect. Showing them
        // as positional-looking input makes a recovery command misleading.
        .map(|usage| usage.replace(" --allow-native-plugin", ""))
}

fn command_benefits_from_progress(command: &Commands) -> bool {
    match command {
        Commands::Init(_) | Commands::New(_) | Commands::Apply(_) => true,
        Commands::Add(_) => true,
        Commands::Capability(command) => matches!(
            command.command,
            CapabilitySubcommand::Add(_)
                | CapabilitySubcommand::Upgrade(_)
                | CapabilitySubcommand::Remove(_)
        ),
        Commands::Pack(command) => matches!(
            command.command,
            cli::PackSubcommand::Install(_)
                | cli::PackSubcommand::Update(_)
                | cli::PackSubcommand::Sign(_)
        ),
        Commands::Registry(command) => matches!(
            command.command,
            cli::RegistrySubcommand::Add(_)
                | cli::RegistrySubcommand::Sync(_)
                | cli::RegistrySubcommand::Remove(_)
        ),
        Commands::Source(command) => matches!(
            command.command,
            cli::SourceSubcommand::Inspect(_)
                | cli::SourceSubcommand::Add(_)
                | cli::SourceSubcommand::Check(_)
                | cli::SourceSubcommand::Sync(_)
                | cli::SourceSubcommand::Update(_)
        ),
        Commands::Defaults(command) => matches!(
            command.command,
            cli::DefaultsSubcommand::Install(_) | cli::DefaultsSubcommand::Update(_)
        ),
        Commands::Doctor(_) | Commands::Verify(_) | Commands::Evidence(_) => true,
        Commands::Plugin(command) => matches!(
            command.command,
            cli::PluginSubcommand::Install(_) | cli::PluginSubcommand::Call(_)
        ),
        Commands::Agent(command) => matches!(command.command, cli::AgentSubcommand::Init(_)),
        Commands::Skill(command) => !matches!(
            command.command,
            cli::SkillSubcommand::Status | cli::SkillSubcommand::Doctor
        ),
        Commands::Conformance(_) | Commands::SelfCommand(_) | Commands::External(_) => true,
        Commands::Schema(_) | Commands::Completion(_) => false,
    }
}

fn command_uses_default_content(command: &Commands) -> bool {
    match command {
        Commands::Init(_) | Commands::Add(_) | Commands::Apply(_) | Commands::Capability(_) => true,
        Commands::New(args) => args.template.is_none() && args.source.is_none(),
        Commands::Pack(command) => matches!(
            command.command,
            cli::PackSubcommand::List
                | cli::PackSubcommand::Inspect { .. }
                | cli::PackSubcommand::Update(_)
        ),
        Commands::Doctor(_) | Commands::Verify(_) | Commands::Evidence(_) => true,
        Commands::Skill(command) => matches!(
            command.command,
            cli::SkillSubcommand::Init(_)
                | cli::SkillSubcommand::Install(_)
                | cli::SkillSubcommand::Update(_)
        ),
        _ => false,
    }
}

fn command_requires_audit(command: &Commands, interactive: bool) -> bool {
    match command {
        Commands::Init(command) => match &command.command {
            InitSubcommand::App(args) => args.apply,
        },
        Commands::New(args) => args.apply,
        Commands::Add(command) => match &command.command {
            AddSubcommand::Capability(args) => args.apply,
        },
        Commands::Apply(args) => args.apply,
        Commands::Capability(command) => match &command.command {
            CapabilitySubcommand::Add(args) => args.apply,
            CapabilitySubcommand::Upgrade(args) | CapabilitySubcommand::Remove(args) => args.apply,
            _ => false,
        },
        Commands::Pack(command) => match &command.command {
            cli::PackSubcommand::Install(args) => args.apply,
            cli::PackSubcommand::Update(args) => args.apply,
            cli::PackSubcommand::Sign(args) => args.apply,
            _ => false,
        },
        Commands::Registry(command) => match &command.command {
            cli::RegistrySubcommand::Add(args) => args.apply,
            cli::RegistrySubcommand::Sync(args) => args.apply,
            cli::RegistrySubcommand::Remove(args) => args.apply,
            _ => false,
        },
        Commands::Source(command) => match &command.command {
            cli::SourceSubcommand::Add(args) => args.apply,
            cli::SourceSubcommand::Sync(args) | cli::SourceSubcommand::Update(args) => args.apply,
            cli::SourceSubcommand::Remove(args) => args.apply,
            cli::SourceSubcommand::Inspect(_)
            | cli::SourceSubcommand::List
            | cli::SourceSubcommand::Resolve(_)
            | cli::SourceSubcommand::Check(_) => false,
        },
        Commands::Defaults(command) => match &command.command {
            cli::DefaultsSubcommand::Install(args) | cli::DefaultsSubcommand::Update(args) => {
                args.apply
            }
            _ => false,
        },
        Commands::Plugin(command) => match &command.command {
            cli::PluginSubcommand::Install(args) => args.apply,
            cli::PluginSubcommand::Call(args) => args.apply,
            _ => false,
        },
        Commands::Evidence(args) => {
            args.apply
                || matches!(
                    &args.command,
                    Some(EvidenceSubcommand::Generate(generate)) if generate.apply
                )
        }
        Commands::Agent(command) => {
            matches!(&command.command, cli::AgentSubcommand::Init(args) if args.apply)
        }
        Commands::Skill(command) => match &command.command {
            cli::SkillSubcommand::Init(args) => args.apply,
            cli::SkillSubcommand::Install(args) => args.apply || (interactive && !args.dry_run),
            cli::SkillSubcommand::Create(args) => args.apply,
            cli::SkillSubcommand::Sync(args) => args.apply,
            cli::SkillSubcommand::Update(args) => args.apply,
            cli::SkillSubcommand::Uninstall(args) => args.apply,
            cli::SkillSubcommand::Status | cli::SkillSubcommand::Doctor => false,
        },
        Commands::SelfCommand(command) => match &command.command {
            cli::SelfSubcommand::Update(args) => args.apply,
            cli::SelfSubcommand::Skip(args) => args.apply,
            cli::SelfSubcommand::Check(_) => false,
        },
        Commands::External(_) => true,
        _ => false,
    }
}

fn command_label(command: &Commands) -> &'static str {
    match command {
        Commands::Init(_) => "init",
        Commands::New(_) => "new",
        Commands::Add(_) => "add capability",
        Commands::Apply(_) => "apply",
        Commands::Capability(command) => match &command.command {
            CapabilitySubcommand::Add(_) => "capability add",
            CapabilitySubcommand::Upgrade(_) => "capability upgrade",
            CapabilitySubcommand::Remove(_) => "capability remove",
            _ => "capability",
        },
        Commands::Pack(_) => "pack",
        Commands::Registry(_) => "registry",
        Commands::Source(_) => "source",
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
        Commands::Completion(_) => "completion",
        Commands::External(_) => "external",
    }
}

fn run(cli: Cli, context: &runtime::RunContext<'_>) -> RainyResult<CommandOutput> {
    if context.cancelled() {
        return Err(RainyError::action("CANCELLED", "command cancelled by user"));
    }
    let _execution_metadata = (
        context.output_mode,
        context.terminal.width,
        context.trace_id.as_deref(),
    );
    let workspace = context.workspace().to_path_buf();
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
        Commands::New(args) => {
            if args.git_url.is_some() && args.source.is_none() && args.template.is_none() {
                return Err(RainyError::config(
                    "PROJECT_GIT_URL_INVALID",
                    "--git-url is available only with --source or --template",
                ));
            }
            let dry_run = if args.template.is_some() || args.source.is_some() {
                resolve_template_init_mode(args.dry_run, args.apply)?
            } else {
                resolve_init_mode(args.dry_run, args.apply)?
            };
            let package = args
                .package
                .unwrap_or_else(|| "com.example.demo".to_string());
            if let Some(source_name) = args.source {
                source::create_project(source::SourceProjectOptions {
                    base_dir: workspace,
                    name: args.name,
                    package,
                    source: source_name,
                    template: args.template,
                    modules: args.module,
                    git_url: args.git_url,
                    dry_run,
                    interactive: context.terminal.interactive,
                    no_color: !context.terminal.color,
                    progress: context.progress,
                })
            } else if let Some(template) = args.template {
                project_template::create_project(project_template::ProjectTemplateOptions {
                    base_dir: workspace,
                    name: args.name,
                    package,
                    template,
                    catalog_path: args.template_config,
                    git_url: args.git_url,
                    dry_run,
                    progress: context.progress,
                })
            } else {
                init::init_app(init::InitOptions {
                    base_dir: workspace,
                    name: args.name,
                    package,
                    preset: Some("spring-nextjs".to_string()),
                    golden_path: Some(
                        args.golden_path
                            .unwrap_or_else(|| "spring-nextjs-saas".to_string()),
                    ),
                    dry_run,
                })
            }
        }
        Commands::Source(command) => {
            source::handle_source_command(&workspace, command, context.progress)
        }
        Commands::Add(command) => match command.command {
            AddSubcommand::Capability(args) => add_capability(&workspace, args),
        },
        Commands::Apply(args) => apply_plan_command(&workspace, args),
        Commands::Capability(command) => match command.command {
            CapabilitySubcommand::Add(args) => add_capability(&workspace, args),
            CapabilitySubcommand::List => registry::capability_list(&workspace),
            CapabilitySubcommand::Explain { id } => registry::capability_explain(&workspace, &id),
            CapabilitySubcommand::Installed => config::capability_installed(&workspace),
            CapabilitySubcommand::Graph => registry::capability_graph(&workspace),
            CapabilitySubcommand::Upgrade(args) => upgrade_capability(&workspace, args),
            CapabilitySubcommand::Remove(args) => remove_capability(&workspace, args),
        },
        Commands::Pack(command) => registry::handle_pack_command(
            &workspace,
            command,
            context.terminal.interactive,
            !context.terminal.color,
            context.progress,
        ),
        Commands::Registry(command) => registry::handle_registry_command(
            &workspace,
            command,
            context.terminal.interactive,
            !context.terminal.color,
            context.progress,
        ),
        Commands::Defaults(command) => defaults::handle_defaults_command(command),
        Commands::Doctor(args) => doctor::doctor_command(
            &workspace,
            args.scope,
            args.capability.as_deref(),
            args.network,
            context.progress,
        ),
        Commands::Verify(args) => verify::verify_command(
            &workspace,
            &args.profile,
            args.capability.as_deref(),
            context.progress,
        ),
        Commands::Evidence(args) => {
            let (format, dry_run, apply) = match args.command {
                Some(EvidenceSubcommand::Generate(generate)) => (
                    generate.format.or(args.format),
                    generate.dry_run || args.dry_run,
                    generate.apply || args.apply,
                ),
                None => (args.format, args.dry_run, args.apply),
            };
            let apply = resolve_apply_flags(dry_run, apply)?;
            evidence::generate_command(&workspace, format.unwrap_or(EvidenceFormat::All), apply)
        }
        Commands::Plugin(command) => {
            plugin::handle_plugin_command(&workspace, command, allow_native_plugin)
        }
        Commands::Agent(command) => agent::handle_agent_command(&workspace, command),
        Commands::Skill(command) => skills::handle_skill_command(
            &workspace,
            command,
            context.progress,
            context.terminal.interactive,
            !context.terminal.color,
        ),
        Commands::Conformance(command) => conformance::handle_conformance_command(command),
        Commands::Schema(command) => schema::handle_schema_command(command),
        Commands::SelfCommand(command) => update::handle_self_command(command),
        Commands::Completion(command) => generate_completion(command),
        Commands::External(args) => {
            plugin::run_external(&workspace, args.args, allow_native_plugin)
        }
    }
}

fn color_disabled(explicit: bool) -> bool {
    explicit
        || std::env::var_os("NO_COLOR").is_some()
        || std::env::var("TERM").is_ok_and(|term| term.eq_ignore_ascii_case("dumb"))
}

fn generate_completion(command: cli::CompletionCommand) -> RainyResult<CommandOutput> {
    let (name, shell) = match command.shell {
        cli::CompletionShell::Bash => ("bash", clap_complete::Shell::Bash),
        cli::CompletionShell::Elvish => ("elvish", clap_complete::Shell::Elvish),
        cli::CompletionShell::Fish => ("fish", clap_complete::Shell::Fish),
        cli::CompletionShell::Powershell => ("powershell", clap_complete::Shell::PowerShell),
        cli::CompletionShell::Zsh => ("zsh", clap_complete::Shell::Zsh),
    };
    let mut definition = Cli::command();
    let mut bytes = Vec::new();
    clap_complete::generate(shell, &mut definition, "rainy", &mut bytes);
    let script = String::from_utf8(bytes).map_err(|error| {
        RainyError::action(
            "COMPLETION_GENERATION_FAILED",
            format!("generated {name} completion was not UTF-8: {error}"),
        )
    })?;
    Ok(CommandOutput::Completion {
        shell: name.to_string(),
        script,
    })
}

fn resolve_init_mode(dry_run: bool, apply: bool) -> RainyResult<bool> {
    if dry_run && apply {
        return Err(RainyError::config(
            "APPLY_MODE_CONFLICT",
            "--dry-run and --apply cannot be used together",
        ));
    }
    Ok(dry_run || !apply)
}

fn resolve_template_init_mode(dry_run: bool, apply: bool) -> RainyResult<bool> {
    if dry_run && apply {
        return Err(RainyError::config(
            "APPLY_MODE_CONFLICT",
            "--dry-run and --apply cannot be used together",
        ));
    }
    Ok(dry_run || !apply)
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
