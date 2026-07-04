use clap::{Args, Parser, Subcommand, ValueEnum};
use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "rainy")]
#[command(version)]
#[command(about = "Rainy CLI capability orchestration")]
pub struct Cli {
    #[arg(long, global = true)]
    pub workspace: Option<PathBuf>,

    #[arg(long, global = true)]
    pub json: bool,

    #[arg(long, global = true)]
    pub no_color: bool,

    #[arg(long, global = true)]
    pub trace_id: Option<String>,

    #[arg(long, global = true)]
    pub verbose: bool,

    #[arg(long, global = true)]
    pub quiet: bool,

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
pub struct SkillCommand {
    #[command(subcommand)]
    pub command: SkillSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum SkillSubcommand {
    Sync,
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
    Check,
    Update(SelfUpdateArgs),
    Skip(SelfSkipArgs),
}

#[derive(Debug, Args)]
pub struct SelfUpdateArgs {
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct SelfSkipArgs {
    pub version: Option<String>,
}
