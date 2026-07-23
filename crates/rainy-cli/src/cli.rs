use crate::progress::ProgressMode;
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "rainy")]
#[command(version)]
#[command(
    about = "Orchestrate application capabilities, packs, plugins, and AI agent tooling",
    long_about = "Rainy manages capability-driven application projects from initialization through verification and evidence generation.\n\nArguments shown as <VALUE> are required values. Options shown in [brackets] are optional. Run 'rainy <COMMAND> --help' for command-specific arguments and examples.",
    after_help = "QUICK START:\n  Create a project:\n    rainy new demo-saas --apply\n\n  Inspect available capabilities:\n    rainy capability list\n\n  Preview and apply a capability:\n    rainy add capability minio-file-storage --dry-run\n    rainy add capability minio-file-storage --apply\n\n  Validate the workspace:\n    rainy doctor\n    rainy verify --profile ci\n\nRun 'rainy <COMMAND> --help' for command-specific examples."
)]
pub struct Cli {
    /// Project root; defaults to the current directory.
    #[arg(long, global = true, value_name = "PROJECT_DIR")]
    pub workspace: Option<PathBuf>,

    /// Emit machine-readable JSON output.
    #[arg(long, global = true, value_name = "TRACE_ID")]
    pub json: bool,

    /// Disable ANSI color output.
    #[arg(long, global = true)]
    pub no_color: bool,

    /// Attach a caller-provided trace identifier to audit records.
    #[arg(long, global = true)]
    pub trace_id: Option<String>,

    /// Enable verbose diagnostic output.
    #[arg(long, global = true)]
    pub verbose: bool,

    /// Suppress non-essential output.
    #[arg(long, global = true)]
    pub quiet: bool,

    /// Progress display mode: auto uses an interactive terminal only.
    #[arg(
        long,
        global = true,
        value_enum,
        value_name = "MODE",
        default_value = "auto",
        env = "RAINY_PROGRESS"
    )]
    pub progress: ProgressMode,

    /// Allow plugins that execute as unrestricted host processes.
    #[arg(
        long,
        global = true,
        env = "RAINY_ALLOW_NATIVE_PLUGIN",
        value_parser = clap::builder::BoolishValueParser::new()
    )]
    pub allow_native_plugin: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Initialize a Rainy application using a preset.
    Init(InitCommand),
    /// Create a new Golden Path application workspace.
    New(NewCommand),
    /// Add a capability and generate or execute its change plan.
    Add(AddCommand),
    /// Apply a previously generated change plan.
    Apply(ApplyCommand),
    /// Discover and manage application capabilities.
    Capability(CapabilityCommand),
    /// Discover, install, sign, and verify capability packs.
    Pack(PackCommand),
    /// Manage named local, Git, HTTP, and archive registries.
    Registry(RegistryCommand),
    /// Diagnose workspace configuration and capability health.
    Doctor(DoctorCommand),
    /// Run workspace and capability verification profiles.
    Verify(VerifyCommand),
    /// Generate audit and delivery evidence reports.
    Evidence(EvidenceCommand),
    /// Discover, install, and invoke Rainy plugins.
    Plugin(PluginCommand),
    /// Generate AI agent context for the current workspace.
    Agent(AgentCommand),
    Skill(SkillCommand),
    /// Check packs and plugins against Rainy protocols.
    Conformance(ConformanceCommand),
    /// List and validate Rainy document schemas.
    Schema(SchemaCommand),
    /// Check, install, or skip Rainy CLI updates.
    #[command(name = "self")]
    SelfCommand(SelfCommand),
    #[command(external_subcommand)]
    External(Vec<OsString>),
}

#[derive(Debug, Args)]
#[command(
    about = "Initialize a Rainy application using a preset",
    after_help = "EXAMPLES:\n  Initialize an application:\n    rainy init app demo-saas --preset spring-nextjs --apply\n\n  Preview without writing files:\n    rainy init app demo-saas --dry-run"
)]
pub struct InitCommand {
    #[command(subcommand)]
    pub command: InitSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum InitSubcommand {
    #[command(
        about = "Initialize an application workspace",
        after_help = "EXAMPLES:\n  Initialize with the default package name:\n    rainy init app demo-saas --apply\n\n  Select a preset and Java package:\n    rainy init app demo-saas --preset spring-nextjs --package com.example.demo --apply\n\n  Preview generated files:\n    rainy init app demo-saas --dry-run --json"
    )]
    App(InitAppArgs),
}

#[derive(Debug, Args)]
pub struct InitAppArgs {
    /// Application directory and project name.
    #[arg(value_name = "APP_NAME")]
    pub name: String,

    /// Project preset to initialize.
    #[arg(long, value_name = "PRESET")]
    pub preset: Option<String>,

    /// Base application package or namespace.
    #[arg(long, value_name = "PACKAGE", default_value = "com.example.demo")]
    pub package: String,

    /// Preview generated files without writing them.
    #[arg(long)]
    pub dry_run: bool,

    /// Write the generated application files.
    #[arg(long)]
    pub apply: bool,
}

#[derive(Debug, Args)]
#[command(
    about = "Create a new Golden Path application workspace",
    long_about = "Create a new application from a Golden Path template. The command previews the generated workspace unless --apply is supplied.",
    after_help = "EXAMPLES:\n  Preview the default Golden Path:\n    rainy new demo-saas --dry-run\n\n  Create the application:\n    rainy new demo-saas --golden-path spring-nextjs-saas --package com.example.demo --apply\n\n  Inspect the plan as JSON:\n    rainy new demo-saas --dry-run --json"
)]
pub struct NewCommand {
    /// Application directory and project name.
    #[arg(value_name = "APP_NAME")]
    pub name: String,

    /// Golden Path template identifier.
    #[arg(long, value_name = "GOLDEN_PATH", default_value = "spring-nextjs-saas")]
    pub golden_path: String,

    /// Base application package or namespace.
    #[arg(long, value_name = "PACKAGE")]
    pub package: Option<String>,

    /// Preview generated files without writing them.
    #[arg(long)]
    pub dry_run: bool,

    /// Write the generated application files.
    #[arg(long)]
    pub apply: bool,
}

#[derive(Debug, Args)]
#[command(
    about = "Add a capability and generate or execute its change plan",
    after_help = "EXAMPLES:\n  Preview a capability change:\n    rainy add capability minio-file-storage --provider minio --dry-run\n\n  Apply the capability change:\n    rainy add capability minio-file-storage --provider minio --apply"
)]
pub struct AddCommand {
    #[command(subcommand)]
    pub command: AddSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum AddSubcommand {
    #[command(
        about = "Add a capability to the workspace",
        after_help = "EXAMPLES:\n  Preview a provider selection:\n    rainy add capability minio-file-storage --provider minio --dry-run\n\n  Save the generated plan for review:\n    rainy add capability minio-file-storage --output-plan plans/minio.json\n\n  Apply a reviewed plan:\n    rainy add capability minio-file-storage --plan plans/minio.json --apply"
    )]
    Capability(AddCapabilityArgs),
}

#[derive(Debug, Args)]
pub struct AddCapabilityArgs {
    /// Capability identifier declared by an available pack.
    #[arg(value_name = "CAPABILITY_ID")]
    pub id: String,

    /// Provider implementation to select.
    #[arg(long, value_name = "PROVIDER")]
    pub provider: Option<String>,

    /// Preview the change plan without writing files.
    #[arg(long)]
    pub dry_run: bool,

    /// Apply the generated or supplied change plan.
    #[arg(long)]
    pub apply: bool,

    /// Write the generated plan to this JSON file.
    #[arg(long, value_name = "PLAN_FILE")]
    pub output_plan: Option<PathBuf>,

    /// Use an existing plan instead of generating one.
    #[arg(long, value_name = "PLAN_FILE")]
    pub plan: Option<PathBuf>,

    /// Continue after explicitly reviewing detected conflicts.
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
#[command(
    about = "Apply a previously generated Rainy change plan",
    after_help = "EXAMPLES:\n  Preview a saved plan:\n    rainy apply --plan plans/minio.json --dry-run\n\n  Apply a reviewed plan:\n    rainy apply --plan plans/minio.json --apply\n\n  Continue after reviewing conflicts:\n    rainy apply --plan plans/minio.json --apply --force"
)]
pub struct ApplyCommand {
    /// Rainy JSON change plan to execute.
    #[arg(long, value_name = "PLAN_FILE")]
    pub plan: PathBuf,

    /// Preview plan effects without writing files.
    #[arg(long)]
    pub dry_run: bool,

    /// Execute the plan and write changes.
    #[arg(long)]
    pub apply: bool,

    /// Continue after explicitly reviewing detected conflicts.
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
#[command(
    about = "Discover and manage application capabilities",
    after_help = "EXAMPLES:\n  List available capabilities:\n    rainy capability list\n\n  Explain one capability:\n    rainy capability explain minio-file-storage\n\n  Show installed capabilities:\n    rainy capability installed\n\nRun 'rainy capability <COMMAND> --help' for more examples."
)]
pub struct CapabilityCommand {
    #[command(subcommand)]
    pub command: CapabilitySubcommand,
}

#[derive(Debug, Subcommand)]
pub enum CapabilitySubcommand {
    #[command(
        about = "List capabilities available from loaded packs",
        after_help = "EXAMPLES:\n  List capabilities:\n    rainy capability list\n\n  Return structured output:\n    rainy capability list --json"
    )]
    List,
    #[command(
        about = "Explain a capability, its providers, and requirements",
        after_help = "EXAMPLES:\n  Explain a capability:\n    rainy capability explain minio-file-storage\n\n  Return structured output:\n    rainy capability explain minio-file-storage --json"
    )]
    Explain {
        /// Capability identifier to explain.
        #[arg(value_name = "CAPABILITY_ID")]
        id: String,
    },
    #[command(
        about = "Show the capability dependency graph",
        after_help = "EXAMPLES:\n  Print the dependency graph:\n    rainy capability graph\n\n  Return graph data as JSON:\n    rainy capability graph --json"
    )]
    Graph,
    #[command(
        about = "List capabilities installed in the workspace",
        after_help = "EXAMPLES:\n  List installed capabilities:\n    rainy capability installed\n\n  Return structured output:\n    rainy capability installed --json"
    )]
    Installed,
    #[command(
        about = "Upgrade an installed capability",
        after_help = "EXAMPLES:\n  Preview an upgrade:\n    rainy capability upgrade minio-file-storage --dry-run\n\n  Apply the upgrade:\n    rainy capability upgrade minio-file-storage --apply"
    )]
    Upgrade(CapabilityChangeArgs),
    #[command(
        about = "Remove an installed capability",
        after_help = "EXAMPLES:\n  Preview capability removal:\n    rainy capability remove minio-file-storage --dry-run\n\n  Apply capability removal:\n    rainy capability remove minio-file-storage --apply"
    )]
    Remove(CapabilityChangeArgs),
}

#[derive(Debug, Args)]
pub struct CapabilityChangeArgs {
    /// Installed capability identifier.
    #[arg(value_name = "CAPABILITY_ID")]
    pub id: String,

    /// Preview the change plan without writing files.
    #[arg(long)]
    pub dry_run: bool,

    /// Apply the capability change.
    #[arg(long)]
    pub apply: bool,

    /// Write the generated plan to this JSON file.
    #[arg(long, value_name = "PLAN_FILE")]
    pub output_plan: Option<PathBuf>,

    /// Continue after explicitly reviewing detected conflicts.
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
#[command(
    about = "Discover, install, sign, and verify capability packs",
    after_help = "EXAMPLES:\n  List loaded packs:\n    rainy pack list\n\n  Inspect a pack:\n    rainy pack inspect minio-file-storage\n\n  Preview installing a local or remote pack:\n    rainy pack install ./community-packs/minio-file-storage --dry-run\n\nRun 'rainy pack <COMMAND> --help' for more examples."
)]
pub struct PackCommand {
    #[command(subcommand)]
    pub command: PackSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum PackSubcommand {
    #[command(
        about = "List loaded capability packs",
        after_help = "EXAMPLES:\n  List packs:\n    rainy pack list\n\n  Return structured output:\n    rainy pack list --json"
    )]
    List,
    #[command(
        about = "Inspect a capability pack",
        after_help = "EXAMPLES:\n  Inspect a pack:\n    rainy pack inspect minio-file-storage\n\n  Return structured output:\n    rainy pack inspect minio-file-storage --json"
    )]
    Inspect {
        /// Pack identifier to inspect.
        #[arg(value_name = "PACK_ID")]
        id: String,
    },
    #[command(
        about = "Install a pack from a directory, Git repository, or HTTPS source",
        after_help = "EXAMPLES:\n  Preview a local pack installation:\n    rainy pack install ./community-packs/minio-file-storage --dry-run\n\n  Install the pack:\n    rainy pack install ./community-packs/minio-file-storage --apply"
    )]
    Install(PackInstallArgs),
    #[command(
        about = "Refresh installed packs from their pinned sources",
        after_help = "EXAMPLES:\n  Preview pack updates:\n    rainy pack update --dry-run\n\n  Apply pack updates:\n    rainy pack update --apply"
    )]
    Update(PackUpdateArgs),
    #[command(
        about = "Sign a capability pack",
        after_help = "EXAMPLES:\n  Sign a local pack:\n    rainy pack sign ./community-packs/minio-file-storage"
    )]
    Sign(PackPathArgs),
    #[command(
        about = "Verify a capability pack signature and contents",
        after_help = "EXAMPLES:\n  Verify a local pack:\n    rainy pack verify ./community-packs/minio-file-storage"
    )]
    Verify(PackPathArgs),
}

#[derive(Debug, Args)]
pub struct PackInstallArgs {
    /// Local directory, Git source, or HTTPS registry source.
    #[arg(value_name = "PACK_SOURCE")]
    pub source: String,

    /// Stable registry name; generated from the source when omitted.
    #[arg(long, value_name = "REGISTRY_NAME")]
    pub name: Option<String>,

    /// Git branch, tag, or commit to resolve and lock.
    #[arg(long = "ref", value_name = "GIT_REF")]
    pub reference: Option<String>,

    /// Expected SHA-256 for a .tar.gz, .tgz, or .zip archive.
    #[arg(long, value_name = "SHA256")]
    pub sha256: Option<String>,

    /// Pull only named pack modules. Repeat or pass comma-separated names.
    #[arg(long, value_name = "PACK", value_delimiter = ',')]
    pub module: Vec<String>,

    /// Pull every pack module exposed by the source.
    #[arg(long, conflicts_with = "module")]
    pub all: bool,

    /// Install Skill exports from selected pack modules.
    #[arg(long, requires = "apply")]
    pub install_skills: bool,

    /// Agent hosts for exported Skills. Repeat or pass comma-separated values.
    #[arg(
        long,
        value_enum,
        value_name = "AGENT_HOST",
        value_delimiter = ',',
        requires = "install_skills"
    )]
    pub target: Vec<SkillTarget>,

    /// Replace only reviewed enterprise Skill drift.
    #[arg(long)]
    pub force: bool,

    /// Preview installation without changing registry state.
    #[arg(long)]
    pub dry_run: bool,

    /// Install the pack and update pinned state.
    #[arg(long)]
    pub apply: bool,
}

#[derive(Debug, Args)]
pub struct PackUpdateArgs {
    /// Preview updates without changing registry state.
    #[arg(long)]
    pub dry_run: bool,

    /// Download and apply available pack updates.
    #[arg(long)]
    pub apply: bool,
}

#[derive(Debug, Args)]
pub struct PackPathArgs {
    /// Pack directory to process.
    #[arg(value_name = "PACK_DIR")]
    pub path: PathBuf,
}

#[derive(Debug, Args)]
#[command(
    about = "Manage named local, Git, HTTP, and archive registries",
    long_about = "Configure multiple enterprise registries, synchronize selected pack modules, and lock resolved Git commits or archive digests. Registry changes preview unless --apply is supplied.",
    after_help = "EXAMPLES:\n  List configured registries:\n    rainy registry list\n\n  Add a GitLab registry:\n    rainy registry add company git+ssh://git@gitlab.example.com/platform/rainy-packs.git --ref main --apply\n\n  Add a verified archive registry:\n    rainy registry add security https://downloads.example.com/security-packs.tar.gz --sha256 <SHA256> --apply\n\n  Pull selected modules:\n    rainy registry sync company --module service-baseline,observability --apply\n\n  Pull all modules from all registries:\n    rainy registry sync --all-registries --all --apply\n\nRun 'rainy registry <COMMAND> --help' for command-specific examples."
)]
pub struct RegistryCommand {
    #[command(subcommand)]
    pub command: RegistrySubcommand,
}

#[derive(Debug, Subcommand)]
pub enum RegistrySubcommand {
    #[command(
        about = "List configured registries and synchronization state",
        after_help = "EXAMPLES:\n  List registries:\n    rainy registry list\n\n  Return structured output:\n    rainy registry list --json"
    )]
    List,
    #[command(
        about = "Add or replace a named registry",
        after_help = "EXAMPLES:\n  Add GitHub or GitLab source:\n    rainy registry add company git+https://git.example.com/platform/packs.git --ref v1.2.0 --apply\n\n  Add an archive with explicit checksum:\n    rainy registry add company-release https://downloads.example.com/packs-v1.2.0.zip --sha256 <SHA256> --apply\n\n  Preview local source configuration:\n    rainy registry add local ./enterprise-packs --dry-run"
    )]
    Add(RegistryAddArgs),
    #[command(
        about = "Synchronize one or all configured registries",
        after_help = "EXAMPLES:\n  Preview one registry synchronization:\n    rainy registry sync company --all --dry-run\n\n  Pull selected modules:\n    rainy registry sync company --module service-baseline,company-skills --apply\n\n  Install exported Skills for selected agent hosts:\n    rainy registry sync company --module company-skills --install-skills --target codex,cursor --apply\n\n  Pull every configured registry:\n    rainy registry sync --all-registries --all --apply"
    )]
    Sync(RegistrySyncArgs),
    #[command(
        about = "Remove a registry from the current project",
        long_about = "Remove a registry configuration and lock entry from the current project. Shared content under RAINY_HOME/registries is retained for other projects.",
        after_help = "EXAMPLES:\n  Preview removal:\n    rainy registry remove company --dry-run\n\n  Remove the project association:\n    rainy registry remove company --apply"
    )]
    Remove(RegistryRemoveArgs),
    #[command(
        about = "Validate registry configuration, cache, locks, and modules",
        after_help = "EXAMPLES:\n  Diagnose all registries:\n    rainy registry doctor\n\n  Diagnose one registry:\n    rainy registry doctor company\n\n  Return structured checks:\n    rainy registry doctor --json"
    )]
    Doctor(RegistryDoctorArgs),
}

#[derive(Debug, Args)]
pub struct RegistryAddArgs {
    /// Stable registry alias used by sync, locks, and qualified module names.
    #[arg(value_name = "REGISTRY_NAME")]
    pub name: String,

    /// Local directory, git+ URL, HTTP index, or archive URL.
    #[arg(value_name = "SOURCE")]
    pub source: String,

    /// Git branch, tag, or commit to resolve and lock.
    #[arg(long = "ref", value_name = "GIT_REF")]
    pub reference: Option<String>,

    /// Expected archive SHA-256. When omitted, Rainy fetches SOURCE.sha256.
    #[arg(long, value_name = "SHA256")]
    pub sha256: Option<String>,

    /// Search order hint; higher values are displayed first but never silently override conflicts.
    #[arg(long, value_name = "NUMBER", default_value_t = 0)]
    pub priority: i32,

    /// Preview configuration without writing files.
    #[arg(long)]
    pub dry_run: bool,

    /// Persist the registry configuration.
    #[arg(long)]
    pub apply: bool,
}

#[derive(Debug, Args)]
pub struct RegistrySyncArgs {
    /// Registry alias. Omit only with --all-registries.
    #[arg(
        value_name = "REGISTRY_NAME",
        required_unless_present = "all_registries"
    )]
    pub name: Option<String>,

    /// Synchronize every configured registry.
    #[arg(long, conflicts_with = "name")]
    pub all_registries: bool,

    /// Pull only named pack modules. Repeat or pass comma-separated names.
    #[arg(long, value_name = "PACK", value_delimiter = ',')]
    pub module: Vec<String>,

    /// Pull every module exposed by each selected registry.
    #[arg(long, conflicts_with = "module")]
    pub all: bool,

    /// Install Skill exports from synchronized pack modules.
    #[arg(long, requires = "apply")]
    pub install_skills: bool,

    /// Agent hosts for exported Skills. Repeat or pass comma-separated values.
    #[arg(
        long,
        value_enum,
        value_name = "AGENT_HOST",
        value_delimiter = ',',
        requires = "install_skills"
    )]
    pub target: Vec<SkillTarget>,

    /// Replace only reviewed enterprise Skill drift.
    #[arg(long)]
    pub force: bool,

    /// Preview synchronization without changing caches or locks.
    #[arg(long)]
    pub dry_run: bool,

    /// Download, verify, and atomically replace registry caches.
    #[arg(long)]
    pub apply: bool,
}

#[derive(Debug, Args)]
pub struct RegistryRemoveArgs {
    /// Registry alias to remove.
    #[arg(value_name = "REGISTRY_NAME")]
    pub name: String,

    /// Preview removal without writing files.
    #[arg(long)]
    pub dry_run: bool,

    /// Remove configuration, lock entry, and managed cache.
    #[arg(long)]
    pub apply: bool,
}

#[derive(Debug, Args)]
pub struct RegistryDoctorArgs {
    /// Optional registry alias to diagnose.
    #[arg(value_name = "REGISTRY_NAME")]
    pub name: Option<String>,
}

#[derive(Debug, Args)]
#[command(
    about = "Diagnose workspace configuration and capability health",
    long_about = "Check required project files, capability locks, generated artifacts, development secrets, and capability-provided diagnostics. The command does not modify the workspace.",
    after_help = "EXAMPLES:\n  Diagnose the complete workspace:\n    rainy doctor\n\n  Diagnose one capability:\n    rainy doctor --capability minio-file-storage\n\n  Use structured output in CI or automation:\n    rainy doctor --json"
)]
pub struct DoctorCommand {
    /// Limit diagnostics to one capability identifier.
    #[arg(long, value_name = "CAPABILITY_ID")]
    pub capability: Option<String>,
}

#[derive(Debug, Args)]
#[command(
    about = "Run workspace and capability verification profiles",
    long_about = "Execute validation steps declared by the project and installed capabilities. The local profile tolerates unavailable development tools where possible; the ci profile is a strict release gate.",
    after_help = "EXAMPLES:\n  Run local development verification:\n    rainy verify --profile local\n\n  Run strict CI verification:\n    rainy verify --profile ci\n\n  Verify one capability:\n    rainy verify --profile local --capability minio-file-storage"
)]
pub struct VerifyCommand {
    /// Verification profile, such as local or ci.
    #[arg(long, value_name = "PROFILE", default_value = "local")]
    pub profile: String,

    /// Limit verification to one capability identifier.
    #[arg(long, value_name = "CAPABILITY_ID")]
    pub capability: Option<String>,
}

#[derive(Debug, Args)]
#[command(
    about = "Generate audit and delivery evidence reports",
    after_help = "EXAMPLES:\n  Generate the default evidence report:\n    rainy evidence generate\n\n  Generate Markdown and JSON reports:\n    rainy evidence generate --format all\n\n  Compatibility form without the generate subcommand:\n    rainy evidence --format json"
)]
pub struct EvidenceCommand {
    #[command(subcommand)]
    pub command: Option<EvidenceSubcommand>,

    /// Output format when the generate subcommand is omitted.
    #[arg(long, value_enum, value_name = "FORMAT")]
    pub format: Option<EvidenceFormat>,
}

#[derive(Debug, Subcommand)]
pub enum EvidenceSubcommand {
    #[command(
        about = "Generate evidence from configuration, health checks, and changes",
        after_help = "EXAMPLES:\n  Generate the default report:\n    rainy evidence generate\n\n  Generate Markdown only:\n    rainy evidence generate --format markdown\n\n  Generate all supported formats:\n    rainy evidence generate --format all"
    )]
    Generate(EvidenceGenerateArgs),
}

#[derive(Debug, Args)]
pub struct EvidenceGenerateArgs {
    /// Evidence output format.
    #[arg(long, value_enum, value_name = "FORMAT")]
    pub format: Option<EvidenceFormat>,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum EvidenceFormat {
    Markdown,
    Json,
    All,
}

#[derive(Debug, Args)]
#[command(
    about = "Discover, install, and invoke Rainy plugins",
    after_help = "EXAMPLES:\n  List installed plugins:\n    rainy plugin list\n\n  Inspect a plugin:\n    rainy plugin inspect echo\n\n  Preview a plugin action:\n    rainy plugin call echo write-example --dry-run\n\nRun 'rainy plugin <COMMAND> --help' for more examples."
)]
pub struct PluginCommand {
    #[command(subcommand)]
    pub command: PluginSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum PluginSubcommand {
    #[command(
        about = "List installed plugins",
        after_help = "EXAMPLES:\n  List plugins:\n    rainy plugin list\n\n  Return structured output:\n    rainy plugin list --json"
    )]
    List,
    #[command(
        about = "Inspect a plugin manifest and available actions",
        after_help = "EXAMPLES:\n  Inspect a plugin:\n    rainy plugin inspect echo\n\n  Return structured output:\n    rainy plugin inspect echo --json"
    )]
    Inspect {
        /// Plugin identifier to inspect.
        #[arg(value_name = "PLUGIN_ID")]
        id: String,
    },
    #[command(
        about = "Install a plugin from a local or remote source",
        after_help = "EXAMPLES:\n  Preview local plugin installation:\n    rainy plugin install ./path/to/plugin --dry-run\n\n  Install the plugin:\n    rainy plugin install ./path/to/plugin --apply"
    )]
    Install(PluginInstallArgs),
    #[command(
        about = "Invoke a declared plugin action",
        after_help = "EXAMPLES:\n  Preview an action:\n    rainy plugin call echo write-example --dry-run\n\n  Supply JSON input and apply the action:\n    rainy plugin call echo write-example --input request.json --apply"
    )]
    Call(PluginCallArgs),
}

#[derive(Debug, Args)]
pub struct PluginInstallArgs {
    /// Local directory, Git source, or HTTPS plugin source.
    #[arg(value_name = "PLUGIN_SOURCE")]
    pub source: String,

    /// Preview installation without changing plugin state.
    #[arg(long)]
    pub dry_run: bool,

    /// Install the plugin and update pinned state.
    #[arg(long)]
    pub apply: bool,
}

#[derive(Debug, Args)]
pub struct PluginCallArgs {
    /// Installed plugin identifier.
    #[arg(value_name = "PLUGIN_ID")]
    pub id: String,
    /// Action name declared by the plugin.
    #[arg(value_name = "ACTION")]
    pub action: String,

    /// JSON file passed to the plugin action.
    #[arg(long, value_name = "INPUT_FILE")]
    pub input: Option<PathBuf>,

    /// Preview action effects without writing files.
    #[arg(long)]
    pub dry_run: bool,

    /// Execute the plugin action and permit declared writes.
    #[arg(long)]
    pub apply: bool,
}

#[derive(Debug, Args)]
#[command(
    about = "Generate AI agent context for the current workspace",
    after_help = "EXAMPLES:\n  Initialize Rainy's managed AGENTS.md block:\n    rainy agent init\n\n  Print the generated agent context:\n    rainy agent context\n\nRun 'rainy agent <COMMAND> --help' for command-specific details."
)]
pub struct AgentCommand {
    #[command(subcommand)]
    pub command: AgentSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum AgentSubcommand {
    #[command(
        about = "Create or refresh Rainy's managed AGENTS.md block",
        after_help = "EXAMPLES:\n  Initialize agent context in the current workspace:\n    rainy agent init\n\n  Initialize another workspace:\n    rainy --workspace ./demo-saas agent init"
    )]
    Init,
    #[command(
        about = "Print the generated Rainy agent context",
        after_help = "EXAMPLES:\n  Print agent context:\n    rainy agent context\n\n  Return structured output:\n    rainy agent context --json"
    )]
    Context,
}

#[derive(Debug, Args)]
#[command(
    about = "Manage project-scoped AI agent skills",
    long_about = "Manage a project-scoped AI Skill profile for supported agent hosts.\n\nUniversal .agents/skills is always included. Interactive terminals can select one or more additional target hosts, then explicitly confirm installation. Non-interactive callers default to the comet bundle for Codex plus Universal and preview unless --apply or --yes is supplied.",
    after_help = "QUICK START:\n  Interactively select a bundle and target hosts:\n    rainy skill init\n\n  Apply the previewed profile:\n    rainy skill init --apply\n\n  Install only the Rainy Skill (no Node.js required):\n    rainy skill init --profile rainy --target codex --apply\n\n  Check an installed profile:\n    rainy skill status\n    rainy skill doctor\n\nRun 'rainy skill <COMMAND> --help' for command-specific examples."
)]
pub struct SkillCommand {
    #[command(subcommand)]
    pub command: SkillSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum SkillSubcommand {
    #[command(
        about = "Create and install a project Skill profile",
        long_about = "Create rainy-skills.yaml and install the selected project-scoped Skills.\n\nWhen --profile or --target is omitted in a terminal, Rainy opens keyboard-driven selectors and asks whether to install the reviewed selection now. Choosing no, using --dry-run, or running non-interactively without --apply only previews the plan. The comet profile requires Node.js 20+, npx, and Git; the rainy profile has no Node.js dependency.",
        after_help = "EXAMPLES:\n  Interactively select the Skill bundle and target hosts:\n    rainy skill init\n\n  Apply the interactive selection:\n    rainy skill init --apply\n\n  --yes is an alias for --apply:\n    rainy skill init --yes\n\n  Install only Rainy's Skill for Codex:\n    rainy skill init --profile rainy --target codex --apply\n\n  Install for multiple hosts without prompting:\n    rainy skill init --profile comet --target codex,claude,cursor --language zh --apply\n\n  Inspect the machine-readable non-interactive preview:\n    rainy skill init --dry-run --json"
    )]
    Init(SkillInitArgs),
    #[command(
        about = "Install or repair the configured Skill profile",
        long_about = "Install the profile already declared in rainy-skills.yaml and refresh skills.lock.\n\nInteractive terminals display the configured bundle and ask for installation confirmation. Non-interactive callers preview unless --apply or --yes is supplied. Use --force only after reviewing local changes reported as drift.",
        after_help = "EXAMPLES:\n  Preview installation:\n    rainy skill install\n\n  Apply installation:\n    rainy skill install --apply\n\n  Repair reviewed managed-file drift:\n    rainy skill install --force --apply"
    )]
    Install(SkillChangeArgs),
    #[command(
        about = "Refresh Rainy-managed agent context files",
        long_about = "Refresh the Rainy-managed blocks in AGENTS.md and enterprise agent context files while preserving user-authored content outside those blocks.",
        after_help = "EXAMPLES:\n  Refresh agent context:\n    rainy skill sync\n\n  Return a machine-readable report:\n    rainy skill sync --json"
    )]
    Sync,
    #[command(
        about = "Show installed Skill state and drift",
        long_about = "Compare rainy-skills.yaml, skills.lock, and installed Skill files. This command does not modify the workspace.",
        after_help = "EXAMPLES:\n  Show profile status:\n    rainy skill status\n\n  Return a machine-readable report:\n    rainy skill status --json"
    )]
    Status,
    #[command(
        about = "Validate Skill files, tools, policy, and lock state",
        long_about = "Run full Skill diagnostics, including managed-file integrity and Comet prerequisites when the comet profile is selected. This command does not modify the workspace and exits non-zero when a check fails.",
        after_help = "EXAMPLES:\n  Run all diagnostics:\n    rainy skill doctor\n\n  Use structured output in CI:\n    rainy skill doctor --json"
    )]
    Doctor,
    #[command(
        about = "Update the configured Skill profile",
        long_about = "Refresh Rainy-managed Skills and optionally update the pinned Comet, skills CLI, or Superpowers versions.\n\nThe command previews changes by default. Use --apply or --yes to write files. Package versions must be exact semantic versions.",
        after_help = "EXAMPLES:\n  Preview an update using the configured versions:\n    rainy skill update\n\n  Preview a pinned Comet update:\n    rainy skill update --comet-version 0.4.0-beta.6\n\n  Apply a pinned Superpowers update:\n    rainy skill update --superpowers-version 5.1.0 --apply\n\n  Update all managed upstream versions:\n    rainy skill update --comet-version 0.4.0-beta.6 --skills-version 1.5.20 --superpowers-version 5.1.0 --apply"
    )]
    Update(SkillUpdateArgs),
    #[command(
        about = "Remove the configured project Skill profile",
        long_about = "Remove Rainy-managed Skill directories, rainy-skills.yaml, and skills.lock. Other Rainy project configuration and user-authored agent content are preserved.\n\nThe command previews removal by default. Use --apply or --yes to remove files.",
        after_help = "EXAMPLES:\n  Preview removal:\n    rainy skill uninstall\n\n  Apply removal:\n    rainy skill uninstall --apply\n\n  Remove after reviewing managed-file drift:\n    rainy skill uninstall --force --apply"
    )]
    Uninstall(SkillChangeArgs),
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum SkillProfile {
    Rainy,
    Comet,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum SkillLanguage {
    En,
    Zh,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum SkillTarget {
    Universal,
    Codex,
    Claude,
    Cursor,
    GithubCopilot,
    Gemini,
    Opencode,
}

#[derive(Debug, Args)]
pub struct SkillInitArgs {
    /// Skill bundle to manage. Interactive terminals prompt when omitted; scripts default to
    /// comet, which includes Rainy, OpenSpec, Superpowers, and Comet.
    #[arg(long, value_enum, value_name = "PROFILE")]
    pub profile: Option<SkillProfile>,

    /// Language used by generated agent instructions.
    #[arg(long, value_enum, value_name = "LANGUAGE", default_value = "zh")]
    pub language: SkillLanguage,

    /// Agent hosts to install into. Universal .agents/skills is always included. Interactive
    /// terminals show a multi-select when omitted; scripts default to Codex. Repeat this option
    /// or use comma-separated values.
    #[arg(long, value_enum, value_name = "AGENT_HOST", value_delimiter = ',')]
    pub target: Vec<SkillTarget>,

    /// Exact Comet package version used by the comet profile.
    #[arg(long, value_name = "VERSION", default_value = "0.4.0-beta.6")]
    pub comet_version: String,

    /// Exact npm skills CLI version used to install Superpowers.
    #[arg(long, value_name = "VERSION", default_value = "1.5.20")]
    pub skills_version: String,

    /// Exact Superpowers release version installed by the comet profile.
    #[arg(long, value_name = "VERSION", default_value = "5.1.0")]
    pub superpowers_version: String,

    /// Preview managed paths without writing files (this is the default mode).
    #[arg(long)]
    pub dry_run: bool,

    /// Apply the planned changes; --yes is a compatibility alias.
    #[arg(long, visible_alias = "yes")]
    pub apply: bool,

    /// Repair reviewed managed-file drift or an incomplete prior installation.
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct SkillChangeArgs {
    /// Preview managed paths without writing files (this is the default mode).
    #[arg(long)]
    pub dry_run: bool,

    /// Apply the planned changes; --yes is a compatibility alias.
    #[arg(long, visible_alias = "yes")]
    pub apply: bool,

    /// Continue only after reviewing reported managed-file drift.
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct SkillUpdateArgs {
    /// New exact Comet version; valid only for the comet profile.
    #[arg(long, value_name = "VERSION")]
    pub comet_version: Option<String>,

    /// New exact npm skills CLI version; valid only for the comet profile.
    #[arg(long, value_name = "VERSION")]
    pub skills_version: Option<String>,

    /// New exact Superpowers release version; valid only for the comet profile.
    #[arg(long, value_name = "VERSION")]
    pub superpowers_version: Option<String>,

    /// Preview managed paths without writing files (this is the default mode).
    #[arg(long)]
    pub dry_run: bool,

    /// Apply the planned changes; --yes is a compatibility alias.
    #[arg(long, visible_alias = "yes")]
    pub apply: bool,

    /// Continue only after reviewing reported managed-file drift.
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
#[command(
    about = "Check packs and plugins against Rainy protocols",
    after_help = "EXAMPLES:\n  Check the current workspace:\n    rainy conformance check\n\n  Check a pack or plugin directory:\n    rainy conformance check --path ./community-packs\n\n  Return structured results:\n    rainy conformance check --path ./community-packs --json"
)]
pub struct ConformanceCommand {
    #[command(subcommand)]
    pub command: ConformanceSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum ConformanceSubcommand {
    #[command(
        about = "Validate pack and plugin protocol conformance",
        after_help = "EXAMPLES:\n  Check the current workspace:\n    rainy conformance check\n\n  Check a specific directory:\n    rainy conformance check --path ./community-packs\n\n  Use structured output in CI:\n    rainy conformance check --path ./community-packs --json"
    )]
    Check(ConformanceCheckArgs),
}

#[derive(Debug, Args)]
pub struct ConformanceCheckArgs {
    /// Pack, plugin, or containing directory to check.
    #[arg(long, value_name = "PATH")]
    pub path: Option<PathBuf>,
}

#[derive(Debug, Args)]
#[command(
    about = "List and validate Rainy document schemas",
    after_help = "EXAMPLES:\n  List built-in schemas:\n    rainy schema list\n\n  Validate a capability pack:\n    rainy schema validate --schema capability-pack --file pack.yaml\n\nRun 'rainy schema <COMMAND> --help' for more examples."
)]
pub struct SchemaCommand {
    #[command(subcommand)]
    pub command: SchemaSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum SchemaSubcommand {
    #[command(
        about = "List built-in Rainy schemas",
        after_help = "EXAMPLES:\n  List schemas:\n    rainy schema list\n\n  Return structured output:\n    rainy schema list --json"
    )]
    List,
    #[command(
        about = "Validate a document against a built-in schema",
        after_help = "EXAMPLES:\n  Validate a capability pack:\n    rainy schema validate --schema capability-pack --file pack.yaml\n\n  Validate a plugin manifest:\n    rainy schema validate --schema plugin-manifest --file plugin.json"
    )]
    Validate(SchemaValidateArgs),
}

#[derive(Debug, Args)]
pub struct SchemaValidateArgs {
    /// Built-in schema identifier.
    #[arg(long, value_name = "SCHEMA_ID")]
    pub schema: String,

    /// YAML or JSON document to validate.
    #[arg(long, value_name = "DOCUMENT_FILE")]
    pub file: PathBuf,
}

#[derive(Debug, Args)]
#[command(
    about = "Check, install, or skip Rainy CLI updates",
    after_help = "EXAMPLES:\n  Check for a new release:\n    rainy self check\n\n  Install the latest release:\n    rainy self update\n\n  Skip one offered version:\n    rainy self skip 0.3.6\n\nRun 'rainy self <COMMAND> --help' for update source and version options."
)]
pub struct SelfCommand {
    #[command(subcommand)]
    pub command: SelfSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum SelfSubcommand {
    #[command(
        about = "Check whether a newer Rainy CLI release is available",
        after_help = "EXAMPLES:\n  Check the default release source:\n    rainy self check\n\n  Check a different GitHub repository:\n    rainy self check --repo owner/repo\n\n  Return structured update information:\n    rainy self check --json"
    )]
    Check(SelfCheckArgs),
    #[command(
        about = "Download, verify, and install a Rainy CLI release",
        after_help = "EXAMPLES:\n  Install the latest release:\n    rainy self update\n\n  Install a specific release:\n    rainy self update --version v0.3.5\n\n  Use a different GitHub repository:\n    rainy self update --repo owner/repo --version v0.3.5\n\n  Reinstall the current version:\n    rainy self update --force"
    )]
    Update(SelfUpdateArgs),
    #[command(
        about = "Skip update notifications for one release",
        after_help = "EXAMPLES:\n  Skip a specific offered version:\n    rainy self skip 0.3.6\n\n  Skip the latest offered version:\n    rainy self skip\n\n  Use a different GitHub repository:\n    rainy self skip --repo owner/repo 0.3.6"
    )]
    Skip(SelfSkipArgs),
}

#[derive(Debug, Args)]
pub struct SelfCheckArgs {
    /// GitHub repository in owner/name form.
    #[arg(long, value_name = "OWNER/REPO")]
    pub repo: Option<String>,
}

#[derive(Debug, Args)]
pub struct SelfUpdateArgs {
    /// Reinstall even when the requested version is not newer.
    #[arg(long)]
    pub force: bool,

    /// Exact release version to install, such as v0.3.5.
    #[arg(long, value_name = "VERSION")]
    pub version: Option<String>,

    /// GitHub repository in owner/name form.
    #[arg(long, value_name = "OWNER/REPO")]
    pub repo: Option<String>,
}

#[derive(Debug, Args)]
pub struct SelfSkipArgs {
    /// Release version to suppress; defaults to the latest offered version.
    #[arg(value_name = "VERSION")]
    pub version: Option<String>,

    /// GitHub repository in owner/name form.
    #[arg(long, value_name = "OWNER/REPO")]
    pub repo: Option<String>,
}
