use clap::{Args, Parser, Subcommand, ValueEnum};
use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "rainy")]
#[command(version)]
#[command(about = "Rainy CLI capability orchestration")]
pub struct Cli {
    /// Project root; defaults to the current directory.
    #[arg(long, global = true)]
    pub workspace: Option<PathBuf>,

    /// Emit machine-readable JSON output.
    #[arg(long, global = true)]
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
    Init(InitCommand),
    New(NewCommand),
    Add(AddCommand),
    Apply(ApplyCommand),
    Capability(CapabilityCommand),
    Pack(PackCommand),
    Doctor(DoctorCommand),
    Verify(VerifyCommand),
    Evidence(EvidenceCommand),
    Plugin(PluginCommand),
    Agent(AgentCommand),
    Skill(SkillCommand),
    Conformance(ConformanceCommand),
    Schema(SchemaCommand),
    #[command(name = "self")]
    SelfCommand(SelfCommand),
    #[command(external_subcommand)]
    External(Vec<OsString>),
}

#[derive(Debug, Args)]
pub struct InitCommand {
    #[command(subcommand)]
    pub command: InitSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum InitSubcommand {
    App(InitAppArgs),
}

#[derive(Debug, Args)]
pub struct InitAppArgs {
    pub name: String,

    #[arg(long)]
    pub preset: Option<String>,

    #[arg(long, default_value = "com.example.demo")]
    pub package: String,

    #[arg(long)]
    pub dry_run: bool,

    #[arg(long)]
    pub apply: bool,
}

#[derive(Debug, Args)]
pub struct NewCommand {
    pub name: String,

    #[arg(long, default_value = "spring-nextjs-saas")]
    pub golden_path: String,

    #[arg(long)]
    pub package: Option<String>,

    #[arg(long)]
    pub dry_run: bool,

    #[arg(long)]
    pub apply: bool,
}

#[derive(Debug, Args)]
pub struct AddCommand {
    #[command(subcommand)]
    pub command: AddSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum AddSubcommand {
    Capability(AddCapabilityArgs),
}

#[derive(Debug, Args)]
pub struct AddCapabilityArgs {
    pub id: String,

    #[arg(long)]
    pub provider: Option<String>,

    #[arg(long)]
    pub dry_run: bool,

    #[arg(long)]
    pub apply: bool,

    #[arg(long)]
    pub output_plan: Option<PathBuf>,

    #[arg(long)]
    pub plan: Option<PathBuf>,

    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct ApplyCommand {
    #[arg(long)]
    pub plan: PathBuf,

    #[arg(long)]
    pub dry_run: bool,

    #[arg(long)]
    pub apply: bool,

    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct CapabilityCommand {
    #[command(subcommand)]
    pub command: CapabilitySubcommand,
}

#[derive(Debug, Subcommand)]
pub enum CapabilitySubcommand {
    List,
    Explain { id: String },
    Graph,
    Installed,
    Upgrade(CapabilityChangeArgs),
    Remove(CapabilityChangeArgs),
}

#[derive(Debug, Args)]
pub struct CapabilityChangeArgs {
    pub id: String,

    #[arg(long)]
    pub dry_run: bool,

    #[arg(long)]
    pub apply: bool,

    #[arg(long)]
    pub output_plan: Option<PathBuf>,

    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct PackCommand {
    #[command(subcommand)]
    pub command: PackSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum PackSubcommand {
    List,
    Inspect { id: String },
    Install(PackInstallArgs),
    Update(PackUpdateArgs),
    Sign(PackPathArgs),
    Verify(PackPathArgs),
}

#[derive(Debug, Args)]
pub struct PackInstallArgs {
    pub source: String,

    #[arg(long)]
    pub dry_run: bool,

    #[arg(long)]
    pub apply: bool,
}

#[derive(Debug, Args)]
pub struct PackUpdateArgs {
    #[arg(long)]
    pub dry_run: bool,

    #[arg(long)]
    pub apply: bool,
}

#[derive(Debug, Args)]
pub struct PackPathArgs {
    pub path: PathBuf,
}

#[derive(Debug, Args)]
pub struct DoctorCommand {
    #[arg(long)]
    pub capability: Option<String>,
}

#[derive(Debug, Args)]
pub struct VerifyCommand {
    #[arg(long, default_value = "local")]
    pub profile: String,

    #[arg(long)]
    pub capability: Option<String>,
}

#[derive(Debug, Args)]
pub struct EvidenceCommand {
    #[command(subcommand)]
    pub command: Option<EvidenceSubcommand>,

    #[arg(long, value_enum)]
    pub format: Option<EvidenceFormat>,
}

#[derive(Debug, Subcommand)]
pub enum EvidenceSubcommand {
    Generate(EvidenceGenerateArgs),
}

#[derive(Debug, Args)]
pub struct EvidenceGenerateArgs {
    #[arg(long, value_enum)]
    pub format: Option<EvidenceFormat>,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum EvidenceFormat {
    Markdown,
    Json,
    All,
}

#[derive(Debug, Args)]
pub struct PluginCommand {
    #[command(subcommand)]
    pub command: PluginSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum PluginSubcommand {
    List,
    Inspect { id: String },
    Install(PluginInstallArgs),
    Call(PluginCallArgs),
}

#[derive(Debug, Args)]
pub struct PluginInstallArgs {
    pub source: String,

    #[arg(long)]
    pub dry_run: bool,

    #[arg(long)]
    pub apply: bool,
}

#[derive(Debug, Args)]
pub struct PluginCallArgs {
    pub id: String,
    pub action: String,

    #[arg(long)]
    pub input: Option<PathBuf>,

    #[arg(long)]
    pub dry_run: bool,

    #[arg(long)]
    pub apply: bool,
}

#[derive(Debug, Args)]
pub struct AgentCommand {
    #[command(subcommand)]
    pub command: AgentSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum AgentSubcommand {
    Init,
    Context,
}

#[derive(Debug, Args)]
#[command(
    about = "Manage project-scoped AI agent skills",
    long_about = "Manage a project-scoped AI Skill profile for supported agent hosts.\n\nThe default profile is comet, which combines Rainy with OpenSpec, Superpowers, and Comet. Mutating commands preview changes by default and write files only when --apply or --yes is supplied.",
    after_help = "QUICK START:\n  Preview the default Comet profile:\n    rainy skill init\n\n  Apply the previewed profile:\n    rainy skill init --apply\n\n  Install only the Rainy Skill (no Node.js required):\n    rainy skill init --profile rainy --apply\n\n  Check an installed profile:\n    rainy skill status\n    rainy skill doctor\n\nRun 'rainy skill <COMMAND> --help' for command-specific examples."
)]
pub struct SkillCommand {
    #[command(subcommand)]
    pub command: SkillSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum SkillSubcommand {
    #[command(
        about = "Create and install a project Skill profile",
        long_about = "Create rainy-skills.yaml and install the selected project-scoped Skills.\n\nWithout --apply or --yes, this command only previews the managed paths and prints the exact Rainy command that applies the plan. The comet profile requires Node.js 20+, npx, and Git; the rainy profile has no Node.js dependency.",
        after_help = "EXAMPLES:\n  Preview the default Comet profile for Codex:\n    rainy skill init\n\n  Apply the default profile:\n    rainy skill init --apply\n\n  --yes is an alias for --apply:\n    rainy skill init --yes\n\n  Install only Rainy's Skill for Codex:\n    rainy skill init --profile rainy --target codex --apply\n\n  Install for multiple hosts:\n    rainy skill init --target codex,claude,cursor --language zh --apply\n\n  Inspect the machine-readable preview:\n    rainy skill init --dry-run --json"
    )]
    Init(SkillInitArgs),
    #[command(
        about = "Install or repair the configured Skill profile",
        long_about = "Install the profile already declared in rainy-skills.yaml and refresh skills.lock.\n\nThe command previews changes by default. Use --apply or --yes to write files. Use --force only after reviewing local changes reported as drift.",
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
        long_about = "Refresh Rainy-managed Skills and update the pinned Comet package when --comet-version is supplied.\n\nThe command previews changes by default. Use --apply or --yes to write files. Comet versions must be exact semantic versions.",
        after_help = "EXAMPLES:\n  Preview an update using the configured versions:\n    rainy skill update\n\n  Preview a pinned Comet update:\n    rainy skill update --comet-version 0.4.0-beta.6\n\n  Apply the pinned update:\n    rainy skill update --comet-version 0.4.0-beta.6 --apply"
    )]
    Update(SkillUpdateArgs),
    #[command(
        about = "Remove the configured project Skill profile",
        long_about = "Remove Rainy-managed Skill directories, rainy-skills.yaml, and skills.lock. Other Rainy project configuration and user-authored agent content are preserved.\n\nThe command previews removal by default. Use --apply or --yes to remove files.",
        after_help = "EXAMPLES:\n  Preview removal:\n    rainy skill uninstall\n\n  Apply removal:\n    rainy skill uninstall --apply\n\n  Remove after reviewing managed-file drift:\n    rainy skill uninstall --force --apply"
    )]
    Uninstall(SkillChangeArgs),
}

#[derive(Debug, Clone, ValueEnum)]
pub enum SkillProfile {
    Rainy,
    Comet,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum SkillLanguage {
    En,
    Zh,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum SkillTarget {
    Codex,
    Claude,
    Cursor,
    GithubCopilot,
    Gemini,
    Opencode,
}

#[derive(Debug, Args)]
pub struct SkillInitArgs {
    /// Skill bundle to manage: comet integrates Rainy, OpenSpec, Superpowers, and Comet;
    /// rainy installs only the Rainy Skill.
    #[arg(long, value_enum, default_value = "comet")]
    pub profile: SkillProfile,

    /// Language used by generated agent instructions.
    #[arg(long, value_enum, default_value = "zh")]
    pub language: SkillLanguage,

    /// Agent hosts to install into; repeat this option or use comma-separated values.
    #[arg(long, value_enum, value_delimiter = ',', default_value = "codex")]
    pub target: Vec<SkillTarget>,

    /// Exact Comet package version used by the comet profile.
    #[arg(long, default_value = "0.4.0-beta.6")]
    pub comet_version: String,

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
    #[arg(long)]
    pub comet_version: Option<String>,

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
pub struct ConformanceCommand {
    #[command(subcommand)]
    pub command: ConformanceSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum ConformanceSubcommand {
    Check(ConformanceCheckArgs),
}

#[derive(Debug, Args)]
pub struct ConformanceCheckArgs {
    #[arg(long)]
    pub path: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct SchemaCommand {
    #[command(subcommand)]
    pub command: SchemaSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum SchemaSubcommand {
    List,
    Validate(SchemaValidateArgs),
}

#[derive(Debug, Args)]
pub struct SchemaValidateArgs {
    #[arg(long)]
    pub schema: String,

    #[arg(long)]
    pub file: PathBuf,
}

#[derive(Debug, Args)]
pub struct SelfCommand {
    #[command(subcommand)]
    pub command: SelfSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum SelfSubcommand {
    Check(SelfCheckArgs),
    Update(SelfUpdateArgs),
    Skip(SelfSkipArgs),
}

#[derive(Debug, Args)]
pub struct SelfCheckArgs {
    #[arg(long)]
    pub repo: Option<String>,
}

#[derive(Debug, Args)]
pub struct SelfUpdateArgs {
    #[arg(long)]
    pub force: bool,

    #[arg(long)]
    pub version: Option<String>,

    #[arg(long)]
    pub repo: Option<String>,
}

#[derive(Debug, Args)]
pub struct SelfSkipArgs {
    pub version: Option<String>,

    #[arg(long)]
    pub repo: Option<String>,
}
