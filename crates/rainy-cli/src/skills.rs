use crate::agent;
use crate::cli::{
    SkillChangeArgs, SkillCommand, SkillInitArgs, SkillLanguage, SkillProfile, SkillSubcommand,
    SkillTarget, SkillUpdateArgs,
};
use crate::config;
use crate::error::{RainyError, RainyResult};
use crate::output::CommandOutput;
use crate::progress::ProgressReporter;
use chrono::{DateTime, Utc};
use dialoguer::{
    Confirm, MultiSelect, Select,
    theme::{ColorfulTheme, SimpleTheme, Theme},
};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::Component;
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

const PROFILE_PATH: &str = "rainy-skills.yaml";
const LOCK_PATH: &str = "skills.lock";
const COMET_PACKAGE: &str = "@rpamis/comet";
const SKILLS_PACKAGE: &str = "skills";
const SUPERPOWERS_PACKAGE: &str = "obra/superpowers";
const DEFAULT_SKILLS_VERSION: &str = "1.5.20";
const DEFAULT_SUPERPOWERS_VERSION: &str = "5.1.0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillProfileConfig {
    pub api_version: String,
    pub kind: String,
    pub profile: String,
    pub scope: String,
    pub language: String,
    pub targets: Vec<String>,
    pub packages: SkillPackages,
    pub policy: SkillPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillPackages {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comet: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superpowers: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillPolicy {
    pub auto_transition: bool,
    pub require_apply_approval: bool,
    pub verify_profile: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillLock {
    pub api_version: String,
    pub kind: String,
    pub lockfile_version: u32,
    pub profile: String,
    pub scope: String,
    pub language: String,
    pub targets: Vec<String>,
    pub rainy_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comet: Option<LockedPackage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<LockedPackage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superpowers: Option<LockedPackage>,
    pub managed_skills: Vec<ManagedSkill>,
    #[serde(default)]
    pub upstream_skills: Vec<UpstreamSkill>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installer_output_digest: Option<String>,
    pub installed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockedPackage {
    pub package: String,
    pub version: String,
    pub runner: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedSkill {
    pub name: String,
    pub target: String,
    pub path: String,
    pub digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamSkill {
    pub name: String,
    pub target: String,
    pub paths: Vec<String>,
    pub managed_by: String,
    pub digest: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillReport {
    pub protocol_version: String,
    pub status: String,
    pub operation: String,
    pub profile: String,
    pub scope: String,
    pub language: String,
    pub targets: Vec<String>,
    pub changed_files: Vec<String>,
    pub apply_command: Vec<String>,
    pub command: Vec<String>,
    pub checks: Vec<SkillCheck>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillCheck {
    pub id: String,
    pub status: String,
    pub message: String,
}

pub fn handle_skill_command(
    workspace: &Path,
    command: SkillCommand,
    progress: &ProgressReporter,
    interactive: bool,
    no_color: bool,
) -> RainyResult<CommandOutput> {
    match command.command {
        SkillSubcommand::Init(args) => init(workspace, args, progress, interactive, no_color),
        SkillSubcommand::Install(args) => install(workspace, args, progress, interactive, no_color),
        SkillSubcommand::Sync => sync(workspace, progress),
        SkillSubcommand::Status => status(workspace, progress),
        SkillSubcommand::Doctor => doctor(workspace, progress),
        SkillSubcommand::Update(args) => update(workspace, args, progress),
        SkillSubcommand::Uninstall(args) => uninstall(workspace, args, progress),
    }
}

pub fn context_summary(workspace: &Path) -> RainyResult<Option<String>> {
    if !workspace.join(PROFILE_PATH).is_file() {
        return Ok(None);
    }
    let profile = load_profile(workspace)?;
    let mut summary = format!(
        "- Profile: `{}`; language: `{}`; targets: {}.\n",
        profile.profile,
        profile.language,
        profile.targets.join(", ")
    );
    if profile.profile == "comet" {
        summary.push_str(
            "- Use Comet for phase orchestration, OpenSpec for intent, Superpowers for engineering method, and Rainy for executable changes.\n",
        );
        summary.push_str(
            "- Comet transitions never approve Rainy `--apply`; keep `auto_transition` disabled.\n",
        );
    }
    summary.push_str("- Check with `rainy skill status` and `rainy skill doctor`.\n");
    Ok(Some(summary))
}

fn init(
    workspace: &Path,
    args: SkillInitArgs,
    progress: &ProgressReporter,
    interactive: bool,
    no_color: bool,
) -> RainyResult<CommandOutput> {
    progress.detail("Validating workspace and requested Skill profile");
    config::load_config(workspace)?;
    let mut apply = resolve_apply_flags(args.dry_run, args.apply)?;
    let profile_path = workspace.join(PROFILE_PATH);
    let profile_exists = profile_path.exists();
    let desired = if profile_exists && args.profile.is_none() && args.target.is_empty() {
        load_profile(workspace)?
    } else {
        profile_from_args(workspace, &args, interactive, no_color)?
    };

    if profile_exists {
        let current = load_profile(workspace)?;
        if current != desired {
            return Err(RainyError::config(
                "SKILL_PROFILE_CHANGE_REQUIRES_UNINSTALL",
                "a different skill profile is already configured; uninstall it before changing profile, language, target, or pinned package versions",
            ));
        }
    }

    if interactive && !args.dry_run && !args.apply {
        apply = prompt_install_confirmation(&desired, profile_exists, no_color)?;
    }

    if profile_exists && !apply {
        return Ok(CommandOutput::Skill {
            report: report(
                "init",
                "configured",
                &desired,
                Vec::new(),
                vec![
                    "rainy".to_string(),
                    "skill".to_string(),
                    "install".to_string(),
                    "--apply".to_string(),
                ],
                Vec::new(),
                Vec::new(),
            ),
        });
    }

    if !apply {
        progress.detail("Building the Skill installation preview");
        return Ok(CommandOutput::Skill {
            report: planned_report(
                "init",
                &desired,
                init_apply_command(&desired, args.force),
                comet_display(&desired, CometAction::Install),
            ),
        });
    }

    let (mut changed_files, output_digest) =
        apply_install(workspace, &desired, args.force, false, progress)?;
    progress.detail("Validating installed Skills and building skills.lock");
    let lock = build_lock(workspace, &desired, output_digest)?;
    progress.detail("Writing rainy-skills.yaml and skills.lock");
    write_yaml_atomic(&profile_path, &desired)?;
    write_yaml_atomic(&workspace.join(LOCK_PATH), &lock)?;
    changed_files.push(PROFILE_PATH.to_string());
    changed_files.push(LOCK_PATH.to_string());
    progress.detail("Refreshing Rainy-managed agent context");
    agent::sync_skills_command(workspace)?;
    changed_files.push("AGENTS.md".to_string());
    changed_files.sort();
    changed_files.dedup();

    Ok(CommandOutput::Skill {
        report: completed_report("init", &desired, changed_files),
    })
}

fn install(
    workspace: &Path,
    args: SkillChangeArgs,
    progress: &ProgressReporter,
    interactive: bool,
    no_color: bool,
) -> RainyResult<CommandOutput> {
    progress.detail("Loading and validating rainy-skills.yaml");
    config::load_config(workspace)?;
    let mut apply = resolve_apply_flags(args.dry_run, args.apply)?;
    let profile = load_profile(workspace)?;
    if interactive && !args.dry_run && !args.apply {
        apply = prompt_install_confirmation(&profile, true, no_color)?;
    }
    if !apply {
        progress.detail("Building the Skill installation preview");
        return Ok(CommandOutput::Skill {
            report: planned_report(
                "install",
                &profile,
                change_apply_command("install", args.force),
                comet_display(&profile, CometAction::Install),
            ),
        });
    }

    let (mut changed_files, output_digest) =
        apply_install(workspace, &profile, args.force, false, progress)?;
    progress.detail("Building and writing the normalized profile and skills.lock");
    let lock = build_lock(workspace, &profile, output_digest)?;
    write_yaml_atomic(&workspace.join(PROFILE_PATH), &profile)?;
    write_yaml_atomic(&workspace.join(LOCK_PATH), &lock)?;
    changed_files.push(PROFILE_PATH.to_string());
    changed_files.push(LOCK_PATH.to_string());
    progress.detail("Refreshing Rainy-managed agent context");
    agent::sync_skills_command(workspace)?;
    changed_files.push("AGENTS.md".to_string());
    changed_files.sort();
    changed_files.dedup();

    Ok(CommandOutput::Skill {
        report: completed_report("install", &profile, changed_files),
    })
}

fn sync(workspace: &Path, progress: &ProgressReporter) -> RainyResult<CommandOutput> {
    progress.detail("Refreshing Rainy-managed agent context files");
    if !workspace.join(PROFILE_PATH).is_file() {
        return agent::sync_skills_command(workspace);
    }
    let profile = load_profile(workspace)?;
    agent::sync_skills_command(workspace)?;
    Ok(CommandOutput::Skill {
        report: completed_report(
            "sync",
            &profile,
            vec![
                "AGENTS.md".to_string(),
                ".enterprise-agent/context.md".to_string(),
                ".enterprise-agent/capabilities.md".to_string(),
                ".enterprise-agent/commands.md".to_string(),
            ],
        ),
    })
}

fn status(workspace: &Path, progress: &ProgressReporter) -> RainyResult<CommandOutput> {
    progress.detail("Comparing Skill profile, lock, and installed files");
    let profile = load_profile(workspace)?;
    let checks = inspect(workspace, &profile, false)?;
    let status = if checks.iter().any(|check| check.status == "fail") {
        "degraded"
    } else {
        "ok"
    };
    Ok(CommandOutput::Skill {
        report: report(
            "status",
            status,
            &profile,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            checks,
        ),
    })
}

fn doctor(workspace: &Path, progress: &ProgressReporter) -> RainyResult<CommandOutput> {
    progress.detail("Checking Skill files, tools, policy, and lock state");
    let profile = load_profile(workspace)?;
    let checks = inspect(workspace, &profile, true)?;
    let report = report(
        "doctor",
        if checks.iter().any(|check| check.status == "fail") {
            "failed"
        } else {
            "passed"
        },
        &profile,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        checks,
    );
    if report.status == "failed" {
        let failures = report
            .checks
            .iter()
            .filter(|check| check.status == "fail")
            .map(|check| format!("{} ({})", check.id, check.message))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(RainyError::doctor(
            "SKILL_DOCTOR_FAILED",
            format!(
                "Skill diagnostics failed: {failures}; run `rainy skill status` for detailed checks"
            ),
        ));
    }
    Ok(CommandOutput::Skill { report })
}

fn update(
    workspace: &Path,
    args: SkillUpdateArgs,
    progress: &ProgressReporter,
) -> RainyResult<CommandOutput> {
    progress.detail("Loading and validating the configured Skill profile");
    config::load_config(workspace)?;
    let apply = resolve_apply_flags(args.dry_run, args.apply)?;
    let mut profile = load_profile(workspace)?;
    if let Some(version) = args.comet_version {
        if profile.profile != "comet" {
            return Err(RainyError::config(
                "SKILL_COMET_VERSION_UNUSED",
                "--comet-version is only valid for the comet profile",
            ));
        }
        validate_comet_version(&version)?;
        profile.packages.comet = Some(format!("{COMET_PACKAGE}@{version}"));
    }
    if let Some(version) = args.skills_version {
        require_comet_profile(&profile, "--skills-version")?;
        validate_exact_version("skills CLI", &version)?;
        profile.packages.skills = Some(format!("{SKILLS_PACKAGE}@{version}"));
    }
    if let Some(version) = args.superpowers_version {
        require_comet_profile(&profile, "--superpowers-version")?;
        validate_exact_version("Superpowers", &version)?;
        profile.packages.superpowers = Some(format!("{SUPERPOWERS_PACKAGE}@{version}"));
    }
    if !apply {
        progress.detail("Building the Skill update preview");
        return Ok(CommandOutput::Skill {
            report: planned_report(
                "update",
                &profile,
                update_apply_command(&profile, args.force),
                comet_display(&profile, CometAction::Update),
            ),
        });
    }

    let (mut changed_files, output_digest) =
        apply_install(workspace, &profile, args.force, true, progress)?;
    progress.detail("Writing the updated profile and skills.lock");
    write_yaml_atomic(&workspace.join(PROFILE_PATH), &profile)?;
    changed_files.push(PROFILE_PATH.to_string());
    let lock = build_lock(workspace, &profile, output_digest)?;
    write_yaml_atomic(&workspace.join(LOCK_PATH), &lock)?;
    changed_files.push(LOCK_PATH.to_string());
    progress.detail("Refreshing Rainy-managed agent context");
    agent::sync_skills_command(workspace)?;
    changed_files.push("AGENTS.md".to_string());
    changed_files.sort();
    changed_files.dedup();

    Ok(CommandOutput::Skill {
        report: completed_report("update", &profile, changed_files),
    })
}

fn uninstall(
    workspace: &Path,
    args: SkillChangeArgs,
    progress: &ProgressReporter,
) -> RainyResult<CommandOutput> {
    progress.detail("Loading and validating the configured Skill profile");
    config::load_config(workspace)?;
    let apply = resolve_apply_flags(args.dry_run, args.apply)?;
    let profile = load_profile(workspace)?;
    if !apply {
        progress.detail("Building the Skill removal preview");
        return Ok(CommandOutput::Skill {
            report: planned_report(
                "uninstall",
                &profile,
                change_apply_command("uninstall", args.force),
                comet_display(&profile, CometAction::Uninstall),
            ),
        });
    }

    let lock = load_lock(workspace).ok();
    progress.detail("Checking managed files for local drift");
    validate_managed_skills(workspace, lock.as_ref(), args.force)?;
    validate_upstream_skills(workspace, lock.as_ref(), args.force)?;
    if lock.is_none() {
        validate_unlocked_rainy_skills(workspace, &profile, args.force)?;
    }
    if profile.profile == "comet" {
        progress.detail("Running the upstream Comet uninstaller");
        run_comet(workspace, &profile, CometAction::Uninstall)?;
    }

    progress.detail("Removing Rainy-managed Skill files");
    let mut changed_files = Vec::new();
    let names = if profile.profile == "comet" {
        vec!["rainy-cli", "rainy-comet"]
    } else {
        vec!["rainy-cli"]
    };
    for target in &profile.targets {
        for name in &names {
            let path = skills_root(workspace, target)?.join(name);
            if path.exists() {
                std::fs::remove_dir_all(&path)?;
                changed_files.push(relative_string(workspace, &path));
            }
        }
    }
    if let Some(lock) = &lock {
        for skill in &lock.upstream_skills {
            if !matches!(skill.managed_by.as_str(), "comet" | "rainy") {
                continue;
            }
            for relative in &skill.paths {
                let path = workspace.join(relative);
                if path.exists() {
                    std::fs::remove_dir_all(&path)?;
                    changed_files.push(relative.clone());
                }
            }
        }
    }
    if remove_superpowers_local_lock(workspace)? {
        changed_files.push("skills-lock.json".to_string());
    }
    for path in [workspace.join(LOCK_PATH), workspace.join(PROFILE_PATH)] {
        if path.exists() {
            std::fs::remove_file(&path)?;
            changed_files.push(relative_string(workspace, &path));
        }
    }
    changed_files.sort();

    Ok(CommandOutput::Skill {
        report: completed_report("uninstall", &profile, changed_files),
    })
}

fn profile_from_args(
    workspace: &Path,
    args: &SkillInitArgs,
    interactive: bool,
    no_color: bool,
) -> RainyResult<SkillProfileConfig> {
    validate_comet_version(&args.comet_version)?;
    validate_exact_version("skills CLI", &args.skills_version)?;
    validate_exact_version("Superpowers", &args.superpowers_version)?;
    let selected_profile = match args.profile {
        Some(profile) => profile,
        None if interactive => prompt_profile(no_color)?,
        None => SkillProfile::Comet,
    };
    let selected_targets = if !args.target.is_empty() {
        args.target.clone()
    } else if interactive {
        prompt_targets(workspace, no_color)?
    } else {
        vec![SkillTarget::Codex]
    };
    let profile = profile_name(&selected_profile).to_string();
    let mut targets = selected_targets
        .iter()
        .map(|target| target_name(target).to_string())
        .collect::<Vec<_>>();
    targets.push("universal".to_string());
    targets.sort();
    targets.dedup();
    if targets.is_empty() {
        return Err(RainyError::config(
            "SKILL_TARGET_REQUIRED",
            "at least one skill target is required",
        ));
    }
    Ok(SkillProfileConfig {
        api_version: "rainy.dev/v1".to_string(),
        kind: "SkillProfile".to_string(),
        profile: profile.clone(),
        scope: "project".to_string(),
        language: language_name(&args.language).to_string(),
        targets,
        packages: SkillPackages {
            comet: (profile == "comet").then(|| format!("{COMET_PACKAGE}@{}", args.comet_version)),
            skills: (profile == "comet")
                .then(|| format!("{SKILLS_PACKAGE}@{}", args.skills_version)),
            superpowers: (profile == "comet")
                .then(|| format!("{SUPERPOWERS_PACKAGE}@{}", args.superpowers_version)),
        },
        policy: SkillPolicy {
            auto_transition: false,
            require_apply_approval: true,
            verify_profile: "ci".to_string(),
        },
    })
}

fn prompt_profile(no_color: bool) -> RainyResult<SkillProfile> {
    let colorful = ColorfulTheme::default();
    let simple = SimpleTheme;
    let theme: &dyn Theme = if no_color { &simple } else { &colorful };
    let choices = [
        "Complete workflow  Rainy + OpenSpec + Superpowers + Comet",
        "Rainy only         Rainy CLI execution and approval Skill",
    ];
    eprintln!();
    eprintln!("Skill setup");
    eprintln!("  Use arrow keys to move, then press Enter.");
    let selected = Select::with_theme(theme)
        .with_prompt("Select the Skill bundle")
        .items(&choices)
        .default(0)
        .interact()
        .map_err(|error| {
            RainyError::config(
                "SKILL_SELECTION_FAILED",
                format!("could not read the Skill bundle selection: {error}"),
            )
        })?;
    Ok(if selected == 0 {
        SkillProfile::Comet
    } else {
        SkillProfile::Rainy
    })
}

fn prompt_targets(workspace: &Path, no_color: bool) -> RainyResult<Vec<SkillTarget>> {
    let colorful = ColorfulTheme::default();
    let simple = SimpleTheme;
    let theme: &dyn Theme = if no_color { &simple } else { &colorful };
    let targets = [
        SkillTarget::Codex,
        SkillTarget::Claude,
        SkillTarget::Cursor,
        SkillTarget::GithubCopilot,
        SkillTarget::Gemini,
        SkillTarget::Opencode,
    ];
    let labels = [
        "Codex            (uses universal .agents/skills)",
        "Claude Code      (.claude/skills)",
        "Cursor           (.cursor/skills)",
        "GitHub Copilot   (.github/skills)",
        "Gemini CLI       (.gemini/skills)",
        "OpenCode         (.opencode/skills)",
    ];
    let detected = targets
        .iter()
        .map(|target| target_detected(workspace, target))
        .collect::<Vec<_>>();
    let defaults = if detected.iter().any(|value| *value) {
        detected
    } else {
        targets
            .iter()
            .map(|target| matches!(target, SkillTarget::Codex))
            .collect()
    };
    eprintln!();
    eprintln!("  Always included: Universal (.agents/skills)");
    eprintln!("  Use Up/Down to move, Space to select, and Enter to confirm.");
    let selected = MultiSelect::with_theme(theme)
        .with_prompt("Select target agent hosts")
        .items(&labels)
        .defaults(&defaults)
        .interact()
        .map_err(|error| {
            RainyError::config(
                "SKILL_SELECTION_FAILED",
                format!("could not read the target platform selection: {error}"),
            )
        })?;
    Ok(selected.into_iter().map(|index| targets[index]).collect())
}

fn prompt_install_confirmation(
    profile: &SkillProfileConfig,
    existing: bool,
    no_color: bool,
) -> RainyResult<bool> {
    let colorful = ColorfulTheme::default();
    let simple = SimpleTheme;
    let theme: &dyn Theme = if no_color { &simple } else { &colorful };
    eprintln!();
    eprintln!("Installation review");
    eprintln!(
        "  Bundle   {}",
        if profile.profile == "comet" {
            "Complete workflow"
        } else {
            "Rainy only"
        }
    );
    eprintln!("  Targets  {}", profile.targets.join(", "));
    eprintln!(
        "  Skills   {}",
        if profile.profile == "comet" {
            "Rainy CLI, Rainy Comet, OpenSpec, Superpowers, Comet"
        } else {
            "Rainy CLI"
        }
    );
    Confirm::with_theme(theme)
        .with_prompt(if existing {
            "Install or repair this configured Skill bundle now?"
        } else {
            "Install the selected Skill bundle now?"
        })
        .default(true)
        .interact()
        .map_err(|error| {
            RainyError::config(
                "SKILL_SELECTION_FAILED",
                format!("could not read the installation confirmation: {error}"),
            )
        })
}

fn target_detected(workspace: &Path, target: &SkillTarget) -> bool {
    match target {
        SkillTarget::Universal => workspace.join(".agents").exists(),
        SkillTarget::Codex => {
            workspace.join(".agents").exists() || workspace.join(".codex").exists()
        }
        SkillTarget::Claude => {
            workspace.join(".claude").exists() || workspace.join("CLAUDE.md").exists()
        }
        SkillTarget::Cursor => workspace.join(".cursor").exists(),
        SkillTarget::GithubCopilot => {
            workspace.join(".github/copilot-instructions.md").exists()
                || workspace.join(".github/instructions").exists()
                || workspace.join(".github/skills").exists()
        }
        SkillTarget::Gemini => workspace.join(".gemini").exists(),
        SkillTarget::Opencode => workspace.join(".opencode").exists(),
    }
}

fn load_profile(workspace: &Path) -> RainyResult<SkillProfileConfig> {
    let path = workspace.join(PROFILE_PATH);
    if !path.is_file() {
        return Err(RainyError::config(
            "SKILL_PROFILE_NOT_FOUND",
            format!("{PROFILE_PATH} not found; run rainy skill init first"),
        ));
    }
    let mut profile: SkillProfileConfig = serde_yaml::from_str(&std::fs::read_to_string(path)?)?;
    if !profile.targets.iter().any(|target| target == "universal") {
        profile.targets.push("universal".to_string());
        profile.targets.sort();
        profile.targets.dedup();
    }
    if profile.profile == "comet" {
        profile
            .packages
            .skills
            .get_or_insert_with(|| format!("{SKILLS_PACKAGE}@{DEFAULT_SKILLS_VERSION}"));
        profile
            .packages
            .superpowers
            .get_or_insert_with(|| format!("{SUPERPOWERS_PACKAGE}@{DEFAULT_SUPERPOWERS_VERSION}"));
    }
    validate_profile(&profile)?;
    Ok(profile)
}

fn validate_profile(profile: &SkillProfileConfig) -> RainyResult<()> {
    if profile.api_version != "rainy.dev/v1" || profile.kind != "SkillProfile" {
        return Err(RainyError::config(
            "SKILL_PROFILE_INVALID",
            "skill profile must use apiVersion rainy.dev/v1 and kind SkillProfile",
        ));
    }
    if !matches!(profile.profile.as_str(), "rainy" | "comet") {
        return Err(RainyError::config(
            "SKILL_PROFILE_INVALID",
            format!("unsupported skill profile: {}", profile.profile),
        ));
    }
    if profile.scope != "project" {
        return Err(RainyError::config(
            "SKILL_SCOPE_UNSUPPORTED",
            "only project-scoped skill profiles are supported",
        ));
    }
    if !matches!(profile.language.as_str(), "en" | "zh") {
        return Err(RainyError::config(
            "SKILL_LANGUAGE_INVALID",
            format!("unsupported skill language: {}", profile.language),
        ));
    }
    if profile.targets.is_empty() {
        return Err(RainyError::config(
            "SKILL_TARGET_REQUIRED",
            "at least one skill target is required",
        ));
    }
    if profile.targets.iter().collect::<BTreeSet<_>>().len() != profile.targets.len() {
        return Err(RainyError::config(
            "SKILL_TARGET_DUPLICATE",
            "skill targets must be unique",
        ));
    }
    for target in &profile.targets {
        target_relative_root(target)?;
    }
    if profile.profile == "comet" {
        let package = profile.packages.comet.as_deref().ok_or_else(|| {
            RainyError::config(
                "SKILL_COMET_PACKAGE_REQUIRED",
                "comet profile requires packages.comet",
            )
        })?;
        comet_version(profile)?;
        if !package.starts_with(&format!("{COMET_PACKAGE}@")) {
            return Err(RainyError::config(
                "SKILL_COMET_PACKAGE_INVALID",
                format!("Comet package must be pinned as {COMET_PACKAGE}@<exact-version>"),
            ));
        }
        skills_version(profile)?;
        superpowers_version(profile)?;
    }
    if profile.policy.auto_transition {
        return Err(RainyError::config(
            "SKILL_AUTO_TRANSITION_DENIED",
            "Rainy-managed Comet profiles require policy.autoTransition: false",
        ));
    }
    if !profile.policy.require_apply_approval {
        return Err(RainyError::config(
            "SKILL_APPLY_APPROVAL_REQUIRED",
            "Rainy-managed profiles require policy.requireApplyApproval: true",
        ));
    }
    if profile.policy.verify_profile.trim().is_empty() {
        return Err(RainyError::config(
            "SKILL_VERIFY_PROFILE_REQUIRED",
            "policy.verifyProfile must not be empty",
        ));
    }
    Ok(())
}

fn validate_comet_version(version: &str) -> RainyResult<()> {
    Version::parse(version).map_err(|error| {
        RainyError::config(
            "SKILL_COMET_VERSION_INVALID",
            format!("Comet version must be an exact SemVer value: {error}"),
        )
    })?;
    Ok(())
}

fn validate_exact_version(name: &str, version: &str) -> RainyResult<()> {
    Version::parse(version).map_err(|error| {
        RainyError::config(
            "SKILL_PACKAGE_VERSION_INVALID",
            format!("{name} version must be an exact SemVer value: {error}"),
        )
    })?;
    Ok(())
}

fn require_comet_profile(profile: &SkillProfileConfig, option: &str) -> RainyResult<()> {
    if profile.profile != "comet" {
        return Err(RainyError::config(
            "SKILL_COMET_OPTION_UNUSED",
            format!("{option} is only valid for the comet profile"),
        ));
    }
    Ok(())
}

fn comet_version(profile: &SkillProfileConfig) -> RainyResult<String> {
    let package = profile.packages.comet.as_deref().ok_or_else(|| {
        RainyError::config(
            "SKILL_COMET_PACKAGE_REQUIRED",
            "comet profile requires packages.comet",
        )
    })?;
    let prefix = format!("{COMET_PACKAGE}@");
    let version = package.strip_prefix(&prefix).ok_or_else(|| {
        RainyError::config(
            "SKILL_COMET_PACKAGE_INVALID",
            format!("Comet package must be pinned as {COMET_PACKAGE}@<exact-version>"),
        )
    })?;
    validate_comet_version(version)?;
    Ok(version.to_string())
}

fn skills_version(profile: &SkillProfileConfig) -> RainyResult<String> {
    pinned_package_version(
        profile.packages.skills.as_deref(),
        SKILLS_PACKAGE,
        "skills",
        "SKILL_SKILLS_PACKAGE_REQUIRED",
        "skills CLI",
    )
}

fn superpowers_version(profile: &SkillProfileConfig) -> RainyResult<String> {
    pinned_package_version(
        profile.packages.superpowers.as_deref(),
        SUPERPOWERS_PACKAGE,
        "superpowers",
        "SKILL_SUPERPOWERS_PACKAGE_REQUIRED",
        "Superpowers",
    )
}

fn pinned_package_version(
    package: Option<&str>,
    expected_package: &str,
    profile_key: &str,
    required_code: &str,
    display_name: &str,
) -> RainyResult<String> {
    let package = package.ok_or_else(|| {
        RainyError::config(
            required_code,
            format!("comet profile requires packages.{profile_key}"),
        )
    })?;
    let prefix = format!("{expected_package}@");
    let version = package.strip_prefix(&prefix).ok_or_else(|| {
        RainyError::config(
            "SKILL_PACKAGE_INVALID",
            format!("{display_name} package must be pinned as {expected_package}@<exact-version>"),
        )
    })?;
    validate_exact_version(display_name, version)?;
    Ok(version.to_string())
}

fn apply_install(
    workspace: &Path,
    profile: &SkillProfileConfig,
    force: bool,
    overwrite_upstream: bool,
    progress: &ProgressReporter,
) -> RainyResult<(Vec<String>, Option<String>)> {
    progress.detail("Checking prerequisites and managed-file drift");
    validate_profile(profile)?;
    if profile.profile == "comet" {
        check_comet_prerequisites()?;
    }
    let lock = load_lock(workspace).ok();
    validate_managed_skills(workspace, lock.as_ref(), force)?;
    validate_upstream_skills(workspace, lock.as_ref(), force)?;
    progress.detail("Installing bundled Rainy Skills for selected agent hosts");
    let mut changed_files = install_rainy_skills(workspace, profile, lock.as_ref(), force)?;

    let output_digest = if profile.profile == "comet" {
        let action = if overwrite_upstream {
            CometAction::Update
        } else {
            CometAction::Install
        };
        let early_superpowers_digest = if matches!(action, CometAction::Install) {
            progress.detail("Installing Rainy's pinned Superpowers Skill library");
            Some(run_superpowers(workspace, profile)?)
        } else {
            None
        };
        progress.detail(if overwrite_upstream {
            "Running the pinned upstream Comet updater"
        } else {
            "Running the pinned upstream Comet installer"
        });
        let comet_digest = run_comet(workspace, profile, action)?;
        progress.detail("Applying Rainy's safe Comet policy configuration");
        configure_comet(workspace)?;
        changed_files.push(".comet/config.yaml".to_string());
        let superpowers_digest = if let Some(digest) = early_superpowers_digest {
            digest
        } else {
            progress.detail("Refreshing Rainy's pinned Superpowers Skill library");
            run_superpowers(workspace, profile)?
        };
        for target in &profile.targets {
            if let Some((_, paths)) = scan_upstream_for_target(workspace, target)?
                .into_iter()
                .find(|(name, _)| name == "superpowers")
            {
                changed_files.extend(paths.iter().map(|path| relative_string(workspace, path)));
            }
        }
        if workspace.join("skills-lock.json").is_file() {
            changed_files.push("skills-lock.json".to_string());
        }
        Some(combine_digests(&comet_digest, &superpowers_digest))
    } else {
        None
    };
    progress.detail("Consolidating managed Skills into each platform's canonical directory");
    changed_files.extend(normalize_skill_layout(workspace, profile, force)?);

    Ok((changed_files, output_digest))
}

fn install_rainy_skills(
    workspace: &Path,
    profile: &SkillProfileConfig,
    lock: Option<&SkillLock>,
    force: bool,
) -> RainyResult<Vec<String>> {
    let source_root = crate::bundled_assets::skills_path()?;
    let names = if profile.profile == "comet" {
        vec!["rainy-cli", "rainy-comet"]
    } else {
        vec!["rainy-cli"]
    };
    let mut changed_files = Vec::new();
    for target in &profile.targets {
        let root = skills_root(workspace, target)?;
        std::fs::create_dir_all(&root)?;
        for name in &names {
            let source = source_root.join(name);
            if !source.join("SKILL.md").is_file() {
                return Err(RainyError::config(
                    "SKILL_ASSET_MISSING",
                    format!("bundled skill is missing: {name}"),
                ));
            }
            let destination = root.join(name);
            if destination.exists() && !force {
                let owned_by_lock = lock.is_some_and(|lock| {
                    let relative = relative_string(workspace, &destination);
                    lock.managed_skills
                        .iter()
                        .any(|skill| skill.path == relative)
                });
                let matches_source = directory_digest(&destination)? == directory_digest(&source)?;
                if !owned_by_lock && !matches_source {
                    return Err(RainyError::config(
                        "SKILL_TARGET_ALREADY_EXISTS",
                        format!(
                            "{} already exists but is not owned by skills.lock and does not match the bundled Skill; inspect it or rerun with --force",
                            relative_string(workspace, &destination)
                        ),
                    ));
                }
            }
            replace_directory(&source, &destination)?;
            changed_files.push(relative_string(workspace, &destination));
        }
    }
    Ok(changed_files)
}

fn normalize_skill_layout(
    workspace: &Path,
    profile: &SkillProfileConfig,
    force: bool,
) -> RainyResult<Vec<String>> {
    let mut changed_files = Vec::new();
    for target in &profile.targets {
        if target != "codex" {
            continue;
        }
        let canonical = workspace.join(".agents/skills");
        std::fs::create_dir_all(&canonical)?;
        let superpowers_names = locked_superpowers_names(workspace)?;
        for legacy_relative in [".codex/skills", ".agent/skills"] {
            let legacy = workspace.join(legacy_relative);
            if !legacy.is_dir() {
                continue;
            }
            let mut managed_names =
                BTreeSet::from(["rainy-cli".to_string(), "rainy-comet".to_string()]);
            for (_, paths) in scan_upstream(&legacy, &superpowers_names)? {
                managed_names.extend(paths.into_iter().filter_map(|path| {
                    path.file_name()
                        .map(|name| name.to_string_lossy().to_string())
                }));
            }
            for name in managed_names {
                let source = legacy.join(&name);
                if !source.is_dir() || !source.join("SKILL.md").is_file() {
                    continue;
                }
                let destination = canonical.join(&name);
                if destination.is_dir() {
                    let same_content =
                        directory_digest(&source)? == directory_digest(&destination)?;
                    if !same_content && !force {
                        return Err(RainyError::config(
                            "SKILL_LAYOUT_CONFLICT",
                            format!(
                                "{} and {} contain different copies of the same managed Skill; review them and rerun with --force to keep the canonical copy",
                                relative_string(workspace, &source),
                                relative_string(workspace, &destination)
                            ),
                        ));
                    }
                    std::fs::remove_dir_all(&source)?;
                    changed_files.push(relative_string(workspace, &source));
                } else {
                    replace_directory(&source, &destination)?;
                    std::fs::remove_dir_all(&source)?;
                    changed_files.push(relative_string(workspace, &source));
                    changed_files.push(relative_string(workspace, &destination));
                }
            }
            remove_empty_directory(&legacy)?;
        }
    }
    changed_files.sort();
    changed_files.dedup();
    Ok(changed_files)
}

fn remove_empty_directory(path: &Path) -> RainyResult<()> {
    if path.is_dir() && std::fs::read_dir(path)?.next().is_none() {
        std::fs::remove_dir(path)?;
    }
    Ok(())
}

fn build_lock(
    workspace: &Path,
    profile: &SkillProfileConfig,
    installer_output_digest: Option<String>,
) -> RainyResult<SkillLock> {
    let mut managed_skills = Vec::new();
    let mut upstream_skills = Vec::new();
    let expected_rainy = if profile.profile == "comet" {
        vec!["rainy-cli", "rainy-comet"]
    } else {
        vec!["rainy-cli"]
    };
    for target in &profile.targets {
        let root = skills_root(workspace, target)?;
        for name in &expected_rainy {
            let path = root.join(name);
            managed_skills.push(ManagedSkill {
                name: (*name).to_string(),
                target: target.clone(),
                path: relative_string(workspace, &path),
                digest: directory_digest(&path)?,
            });
        }
        if profile.profile == "comet" {
            for (name, paths) in scan_upstream_for_target(workspace, target)? {
                let digest = paths_digest(&paths)?;
                let managed_by = if name == "superpowers" {
                    "rainy"
                } else {
                    "comet"
                };
                upstream_skills.push(UpstreamSkill {
                    name,
                    target: target.clone(),
                    paths: paths
                        .iter()
                        .map(|path| relative_string(workspace, path))
                        .collect(),
                    managed_by: managed_by.to_string(),
                    digest,
                });
            }
        }
    }
    if profile.profile == "comet" {
        assert_required_upstream(profile, &upstream_skills)?;
    }
    managed_skills.sort_by(|left, right| left.path.cmp(&right.path));
    upstream_skills
        .sort_by(|left, right| (&left.target, &left.name).cmp(&(&right.target, &right.name)));
    let comet = if profile.profile == "comet" {
        Some(LockedPackage {
            package: COMET_PACKAGE.to_string(),
            version: comet_version(profile)?,
            runner: if std::env::var_os("RAINY_COMET_BIN").is_some() {
                "custom".to_string()
            } else {
                "npx".to_string()
            },
        })
    } else {
        None
    };
    let skills = if profile.profile == "comet" {
        Some(LockedPackage {
            package: SKILLS_PACKAGE.to_string(),
            version: skills_version(profile)?,
            runner: if std::env::var_os("RAINY_SKILLS_BIN").is_some() {
                "custom".to_string()
            } else {
                "npx".to_string()
            },
        })
    } else {
        None
    };
    let superpowers = if profile.profile == "comet" {
        Some(LockedPackage {
            package: SUPERPOWERS_PACKAGE.to_string(),
            version: superpowers_version(profile)?,
            runner: "skills".to_string(),
        })
    } else {
        None
    };
    Ok(SkillLock {
        api_version: "rainy.dev/v1".to_string(),
        kind: "SkillLock".to_string(),
        lockfile_version: 1,
        profile: profile.profile.clone(),
        scope: profile.scope.clone(),
        language: profile.language.clone(),
        targets: profile.targets.clone(),
        rainy_version: env!("CARGO_PKG_VERSION").to_string(),
        comet,
        skills,
        superpowers,
        managed_skills,
        upstream_skills,
        installer_output_digest,
        installed_at: Utc::now(),
    })
}

fn load_lock(workspace: &Path) -> RainyResult<SkillLock> {
    let path = workspace.join(LOCK_PATH);
    if !path.is_file() {
        return Err(RainyError::config(
            "SKILL_LOCK_NOT_FOUND",
            format!("{LOCK_PATH} not found; run rainy skill install --apply"),
        ));
    }
    let lock: SkillLock = serde_yaml::from_str(&std::fs::read_to_string(path)?)?;
    validate_lock(&lock)?;
    Ok(lock)
}

fn validate_lock(lock: &SkillLock) -> RainyResult<()> {
    if lock.api_version != "rainy.dev/v1" || lock.kind != "SkillLock" || lock.lockfile_version != 1
    {
        return Err(RainyError::config(
            "SKILL_LOCK_INVALID",
            "skills.lock has an unsupported identity or lockfileVersion",
        ));
    }
    for skill in &lock.managed_skills {
        validate_locked_path(&skill.path)?;
        let expected = Path::new(target_relative_root(&skill.target)?).join(&skill.name);
        let legacy_codex = skill.target == "codex"
            && Path::new(&skill.path) == Path::new(".codex/skills").join(&skill.name);
        if Path::new(&skill.path) != expected && !legacy_codex {
            return Err(RainyError::config(
                "SKILL_LOCK_PATH_INVALID",
                format!(
                    "managed Skill path does not match its target and name: {}",
                    skill.path
                ),
            ));
        }
        validate_digest(&skill.digest)?;
    }
    for skill in &lock.upstream_skills {
        let roots = upstream_relative_roots(&skill.target)?;
        for path in &skill.paths {
            validate_locked_path(path)?;
            if !roots.iter().any(|root| Path::new(path).starts_with(root)) {
                return Err(RainyError::config(
                    "SKILL_LOCK_PATH_INVALID",
                    format!(
                        "upstream Skill path is outside target {}: {path}",
                        skill.target
                    ),
                ));
            }
        }
        validate_digest(&skill.digest)?;
    }
    if let Some(digest) = &lock.installer_output_digest {
        validate_digest(digest)?;
    }
    for (locked, expected, name) in [
        (lock.comet.as_ref(), COMET_PACKAGE, "Comet"),
        (lock.skills.as_ref(), SKILLS_PACKAGE, "skills CLI"),
        (
            lock.superpowers.as_ref(),
            SUPERPOWERS_PACKAGE,
            "Superpowers",
        ),
    ] {
        if let Some(locked) = locked {
            if locked.package != expected {
                return Err(RainyError::config(
                    "SKILL_LOCK_PACKAGE_INVALID",
                    format!("locked {name} package must be {expected}"),
                ));
            }
            validate_exact_version(name, &locked.version)?;
        }
    }
    Ok(())
}

fn validate_locked_path(path: &str) -> RainyResult<()> {
    let path = Path::new(path);
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(RainyError::config(
            "SKILL_LOCK_PATH_INVALID",
            format!(
                "Skill lock paths must be normalized workspace-relative paths: {}",
                path.display()
            ),
        ));
    }
    Ok(())
}

fn validate_digest(digest: &str) -> RainyResult<()> {
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(RainyError::config(
            "SKILL_LOCK_DIGEST_INVALID",
            "Skill lock digests must be lowercase SHA-256 values",
        ));
    }
    Ok(())
}

fn validate_managed_skills(
    workspace: &Path,
    lock: Option<&SkillLock>,
    force: bool,
) -> RainyResult<()> {
    let Some(lock) = lock else {
        return Ok(());
    };
    for skill in &lock.managed_skills {
        let path = workspace.join(&skill.path);
        if !path.exists() {
            continue;
        }
        let actual = directory_digest(&path)?;
        if actual != skill.digest && !force {
            return Err(RainyError::config(
                "SKILL_MANAGED_FILES_MODIFIED",
                format!(
                    "{} was modified after installation; review it and rerun with --force to overwrite or remove it",
                    skill.path
                ),
            ));
        }
    }
    Ok(())
}

fn validate_upstream_skills(
    workspace: &Path,
    lock: Option<&SkillLock>,
    force: bool,
) -> RainyResult<()> {
    let Some(lock) = lock else {
        return Ok(());
    };
    for skill in &lock.upstream_skills {
        if !matches!(skill.managed_by.as_str(), "comet" | "rainy") {
            continue;
        }
        let paths = skill
            .paths
            .iter()
            .map(|path| workspace.join(path))
            .collect::<Vec<_>>();
        if paths.iter().any(|path| !path.is_dir()) {
            continue;
        }
        if paths_digest(&paths)? != skill.digest && !force {
            return Err(RainyError::config(
                "SKILL_UPSTREAM_FILES_MODIFIED",
                format!(
                    "{} Skills for {} were modified after installation; review them and rerun with --force to overwrite or remove them",
                    skill.name, skill.target
                ),
            ));
        }
    }
    Ok(())
}

fn validate_unlocked_rainy_skills(
    workspace: &Path,
    profile: &SkillProfileConfig,
    force: bool,
) -> RainyResult<()> {
    if force {
        return Ok(());
    }
    let source_root = crate::bundled_assets::skills_path()?;
    let names = if profile.profile == "comet" {
        vec!["rainy-cli", "rainy-comet"]
    } else {
        vec!["rainy-cli"]
    };
    for target in &profile.targets {
        let root = skills_root(workspace, target)?;
        for name in &names {
            let destination = root.join(name);
            if destination.is_dir()
                && directory_digest(&destination)? != directory_digest(&source_root.join(name))?
            {
                return Err(RainyError::config(
                    "SKILL_MANAGED_FILES_MODIFIED",
                    format!(
                        "{} has no lock and differs from the bundled Skill; inspect it and rerun with --force",
                        relative_string(workspace, &destination)
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn inspect(
    workspace: &Path,
    profile: &SkillProfileConfig,
    include_prerequisites: bool,
) -> RainyResult<Vec<SkillCheck>> {
    let mut checks = Vec::new();
    let lock = match load_lock(workspace) {
        Ok(lock) => {
            checks.push(pass("lock", format!("{LOCK_PATH} is readable")));
            Some(lock)
        }
        Err(error) => {
            let body = error.body();
            checks.push(fail("lock", format!("{}: {}", body.code, body.message)));
            None
        }
    };

    if include_prerequisites && profile.profile == "comet" {
        checks.extend(comet_prerequisite_checks());
    }

    if let Some(lock) = &lock {
        if lock.profile != profile.profile
            || lock.language != profile.language
            || lock.targets != profile.targets
            || !locked_packages_match(lock, profile)?
        {
            checks.push(fail(
                "lock.profile",
                "skills.lock does not match rainy-skills.yaml",
            ));
        } else {
            checks.push(pass("lock.profile", "profile and lock agree"));
        }
        for skill in &lock.managed_skills {
            let path = workspace.join(&skill.path);
            if !path.is_dir() {
                checks.push(fail(
                    format!("managed.{}.{}", skill.target, skill.name),
                    format!("{} is missing", skill.path),
                ));
                continue;
            }
            let actual = directory_digest(&path)?;
            if actual == skill.digest {
                checks.push(pass(
                    format!("managed.{}.{}", skill.target, skill.name),
                    format!("{} matches its locked digest", skill.path),
                ));
            } else {
                checks.push(fail(
                    format!("managed.{}.{}", skill.target, skill.name),
                    format!("{} differs from its locked digest", skill.path),
                ));
            }
        }
        for skill in &lock.upstream_skills {
            let paths = skill
                .paths
                .iter()
                .map(|path| workspace.join(path))
                .collect::<Vec<_>>();
            if paths.iter().any(|path| !path.is_dir()) {
                checks.push(fail(
                    format!("upstream-lock.{}.{}", skill.target, skill.name),
                    format!("one or more locked {} paths are missing", skill.name),
                ));
                continue;
            }
            let actual = paths_digest(&paths)?;
            if actual == skill.digest {
                checks.push(pass(
                    format!("upstream-lock.{}.{}", skill.target, skill.name),
                    format!("locked {} skill content matches", skill.name),
                ));
            } else {
                checks.push(fail(
                    format!("upstream-lock.{}.{}", skill.target, skill.name),
                    format!("locked {} skill content has drifted", skill.name),
                ));
            }
        }
    }
    if profile.profile == "comet" {
        for target in &profile.targets {
            let found = scan_upstream_for_target(workspace, target)?;
            let names = found
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<BTreeSet<_>>();
            for name in ["comet", "openspec", "superpowers"] {
                if names.contains(name) {
                    checks.push(pass(
                        format!("upstream.{target}.{name}"),
                        format!("{name} skills are installed for {target}"),
                    ));
                } else {
                    checks.push(fail(
                        format!("upstream.{target}.{name}"),
                        format!("{name} skills are missing for {target}"),
                    ));
                }
            }
        }
        checks.push(check_comet_policy(workspace)?);
    }
    Ok(checks)
}

fn locked_packages_match(lock: &SkillLock, profile: &SkillProfileConfig) -> RainyResult<bool> {
    if profile.profile != "comet" {
        return Ok(lock.comet.is_none() && lock.skills.is_none() && lock.superpowers.is_none());
    }
    let comet_version = comet_version(profile)?;
    let skills_version = skills_version(profile)?;
    let superpowers_version = superpowers_version(profile)?;
    Ok(lock
        .comet
        .as_ref()
        .is_some_and(|package| package.version == comet_version)
        && lock
            .skills
            .as_ref()
            .is_some_and(|package| package.version == skills_version)
        && lock
            .superpowers
            .as_ref()
            .is_some_and(|package| package.version == superpowers_version))
}

fn scan_upstream(
    root: &Path,
    superpowers_names: &BTreeSet<String>,
) -> RainyResult<Vec<(String, Vec<PathBuf>)>> {
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut comet = Vec::new();
    let mut openspec = Vec::new();
    let mut superpowers = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if !entry.path().is_dir() || !entry.path().join("SKILL.md").is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name == "comet" || name.starts_with("comet-") {
            comet.push(entry.path());
        } else if name.starts_with("openspec-") {
            openspec.push(entry.path());
        } else if superpowers_names.contains(&name)
            || matches!(
                name.as_str(),
                "using-superpowers"
                    | "brainstorming"
                    | "dispatching-parallel-agents"
                    | "writing-plans"
                    | "writing-skills"
                    | "test-driven-development"
                    | "systematic-debugging"
                    | "subagent-driven-development"
                    | "verification-before-completion"
                    | "requesting-code-review"
                    | "receiving-code-review"
                    | "executing-plans"
                    | "using-git-worktrees"
                    | "finishing-a-development-branch"
            )
        {
            superpowers.push(entry.path());
        }
    }
    let mut result = Vec::new();
    if !comet.is_empty() {
        comet.sort();
        result.push(("comet".to_string(), comet));
    }
    if !openspec.is_empty() {
        openspec.sort();
        result.push(("openspec".to_string(), openspec));
    }
    if !superpowers.is_empty() {
        superpowers.sort();
        result.push(("superpowers".to_string(), superpowers));
    }
    Ok(result)
}

fn scan_upstream_for_target(
    workspace: &Path,
    target: &str,
) -> RainyResult<Vec<(String, Vec<PathBuf>)>> {
    let mut found = BTreeMap::<String, Vec<PathBuf>>::new();
    let superpowers_names = locked_superpowers_names(workspace)?;
    for root in upstream_roots(workspace, target)? {
        for (name, paths) in scan_upstream(&root, &superpowers_names)? {
            found.entry(name).or_default().extend(paths);
        }
    }
    for paths in found.values_mut() {
        paths.sort();
        paths.dedup();
    }
    Ok(found.into_iter().collect())
}

fn locked_superpowers_names(workspace: &Path) -> RainyResult<BTreeSet<String>> {
    let path = workspace.join("skills-lock.json");
    if !path.is_file() {
        return Ok(BTreeSet::new());
    }
    let root: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    let Some(skills) = root.get("skills").and_then(serde_json::Value::as_object) else {
        return Ok(BTreeSet::new());
    };
    Ok(skills
        .iter()
        .filter(|(_, value)| is_superpowers_lock_entry(value))
        .map(|(name, _)| name.clone())
        .collect())
}

fn is_superpowers_lock_entry(value: &serde_json::Value) -> bool {
    ["source", "sourceUrl"]
        .iter()
        .filter_map(|key| value.get(key).and_then(serde_json::Value::as_str))
        .any(|source| source.contains("obra/superpowers"))
}

fn remove_superpowers_local_lock(workspace: &Path) -> RainyResult<bool> {
    let path = workspace.join("skills-lock.json");
    if !path.is_file() {
        return Ok(false);
    }
    let mut root: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
    let Some(skills) = root
        .get_mut("skills")
        .and_then(serde_json::Value::as_object_mut)
    else {
        return Ok(false);
    };
    let before = skills.len();
    skills.retain(|_, value| !is_superpowers_lock_entry(value));
    if skills.len() == before {
        return Ok(false);
    }
    write_json_atomic(&path, &root)?;
    Ok(true)
}

fn assert_required_upstream(
    profile: &SkillProfileConfig,
    upstream: &[UpstreamSkill],
) -> RainyResult<()> {
    for target in &profile.targets {
        for name in ["comet", "openspec", "superpowers"] {
            if !upstream
                .iter()
                .any(|skill| skill.target == *target && skill.name == name)
            {
                return Err(RainyError::config(
                    "SKILL_UPSTREAM_INCOMPLETE",
                    format!(
                        "the managed installer completed but did not install {name} skills for {target}; run rainy skill doctor and retry with rainy skill install --apply"
                    ),
                ));
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum CometAction {
    Install,
    Update,
    Uninstall,
}

fn run_comet(
    workspace: &Path,
    profile: &SkillProfileConfig,
    action: CometAction,
) -> RainyResult<String> {
    let (program, prefix) = comet_program(profile)?;
    let mut command = Command::new(program);
    command.args(prefix);
    command.args(comet_args(workspace, profile, action));
    command.current_dir(workspace);
    command.env("COMET_AUTO_TRANSITION", "false");
    let output = command.output().map_err(|error| {
        RainyError::config(
            "SKILL_COMET_EXEC_FAILED",
            format!("failed to start Comet: {error}"),
        )
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(RainyError::config(
            "SKILL_COMET_FAILED",
            format!(
                "Comet exited with {}: {}{}",
                output.status,
                truncate(stderr.trim(), 3000),
                if stdout.trim().is_empty() {
                    String::new()
                } else {
                    format!("; stdout: {}", truncate(stdout.trim(), 1000))
                }
            ),
        ));
    }
    let mut hasher = Sha256::new();
    hasher.update(&output.stdout);
    hasher.update(&output.stderr);
    Ok(format!("{:x}", hasher.finalize()))
}

fn run_superpowers(workspace: &Path, profile: &SkillProfileConfig) -> RainyResult<String> {
    let (program, prefix) = skills_program(profile)?;
    let mut command = Command::new(program);
    command.args(prefix);
    command.args(superpowers_args(profile)?);
    command.current_dir(workspace);
    let output = command.output().map_err(|error| {
        RainyError::config(
            "SKILL_SUPERPOWERS_EXEC_FAILED",
            format!("failed to start the Superpowers installer: {error}"),
        )
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(RainyError::config(
            "SKILL_SUPERPOWERS_FAILED",
            format!(
                "Superpowers installer exited with {}: {}{}",
                output.status,
                truncate(stderr.trim(), 3000),
                if stdout.trim().is_empty() {
                    String::new()
                } else {
                    format!("; stdout: {}", truncate(stdout.trim(), 1000))
                }
            ),
        ));
    }
    let mut hasher = Sha256::new();
    hasher.update(&output.stdout);
    hasher.update(&output.stderr);
    Ok(format!("{:x}", hasher.finalize()))
}

fn skills_program(profile: &SkillProfileConfig) -> RainyResult<(OsString, Vec<OsString>)> {
    if let Some(path) = std::env::var_os("RAINY_SKILLS_BIN") {
        return Ok((path, Vec::new()));
    }
    let package = profile.packages.skills.as_deref().ok_or_else(|| {
        RainyError::config(
            "SKILL_SKILLS_PACKAGE_REQUIRED",
            "comet profile requires packages.skills",
        )
    })?;
    let executable = if cfg!(windows) { "npx.cmd" } else { "npx" };
    Ok((
        OsString::from(executable),
        vec![
            OsString::from("--yes"),
            OsString::from("--package"),
            OsString::from(package),
            OsString::from("skills"),
        ],
    ))
}

fn superpowers_args(profile: &SkillProfileConfig) -> RainyResult<Vec<OsString>> {
    let version = superpowers_version(profile)?;
    let mut args = vec![
        OsString::from("add"),
        OsString::from(format!(
            "https://github.com/{SUPERPOWERS_PACKAGE}/tree/v{version}/skills"
        )),
        OsString::from("--yes"),
        OsString::from("--copy"),
    ];
    for target in &profile.targets {
        args.push(OsString::from("--agent"));
        args.push(OsString::from(skills_agent_name(target)?));
    }
    Ok(args)
}

fn skills_agent_name(target: &str) -> RainyResult<&'static str> {
    match target {
        "universal" => Ok("universal"),
        "codex" => Ok("codex"),
        "claude" => Ok("claude-code"),
        "cursor" => Ok("cursor"),
        "github-copilot" => Ok("github-copilot"),
        "gemini" => Ok("gemini-cli"),
        "opencode" => Ok("opencode"),
        _ => Err(RainyError::config(
            "SKILL_TARGET_UNSUPPORTED",
            format!("unsupported skill target: {target}"),
        )),
    }
}

fn combine_digests(left: &str, right: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(left.as_bytes());
    hasher.update(right.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn comet_program(profile: &SkillProfileConfig) -> RainyResult<(OsString, Vec<OsString>)> {
    if let Some(path) = std::env::var_os("RAINY_COMET_BIN") {
        return Ok((path, Vec::new()));
    }
    let package = profile.packages.comet.as_deref().ok_or_else(|| {
        RainyError::config(
            "SKILL_COMET_PACKAGE_REQUIRED",
            "comet profile requires packages.comet",
        )
    })?;
    let executable = if cfg!(windows) { "npx.cmd" } else { "npx" };
    Ok((
        OsString::from(executable),
        vec![
            OsString::from("--yes"),
            OsString::from("--package"),
            OsString::from(package),
            OsString::from("comet"),
        ],
    ))
}

fn comet_args(
    workspace: &Path,
    profile: &SkillProfileConfig,
    action: CometAction,
) -> Vec<OsString> {
    match action {
        CometAction::Install | CometAction::Update => vec![
            OsString::from("init"),
            workspace.as_os_str().to_os_string(),
            OsString::from("--yes"),
            OsString::from("--scope"),
            OsString::from("project"),
            OsString::from("--language"),
            OsString::from(&profile.language),
            OsString::from(if matches!(action, CometAction::Update) {
                "--overwrite"
            } else {
                "--skip-existing"
            }),
            OsString::from("--json"),
        ],
        CometAction::Uninstall => vec![
            OsString::from("uninstall"),
            workspace.as_os_str().to_os_string(),
            OsString::from("--force"),
            OsString::from("--scope"),
            OsString::from("project"),
            OsString::from("--json"),
        ],
    }
}

fn comet_display(profile: &SkillProfileConfig, action: CometAction) -> Vec<String> {
    if profile.profile != "comet" {
        return Vec::new();
    }
    let program = std::env::var("RAINY_COMET_BIN").unwrap_or_else(|_| "npx".to_string());
    let mut values = vec![program];
    if std::env::var_os("RAINY_COMET_BIN").is_none()
        && let Some(package) = profile.packages.comet.as_deref()
    {
        values.extend([
            "--yes".to_string(),
            "--package".to_string(),
            package.to_string(),
            "comet".to_string(),
        ]);
    }
    values.extend(
        match action {
            CometAction::Install => vec![
                "init",
                "<workspace>",
                "--yes",
                "--scope",
                "project",
                "--language",
                &profile.language,
                "--skip-existing",
                "--json",
            ],
            CometAction::Update => vec![
                "init",
                "<workspace>",
                "--yes",
                "--scope",
                "project",
                "--language",
                &profile.language,
                "--overwrite",
                "--json",
            ],
            CometAction::Uninstall => vec![
                "uninstall",
                "<workspace>",
                "--force",
                "--scope",
                "project",
                "--json",
            ],
        }
        .into_iter()
        .map(str::to_string),
    );
    if !matches!(action, CometAction::Uninstall)
        && let Ok(mut superpowers) = superpowers_display(profile)
    {
        values.push("&&".to_string());
        values.append(&mut superpowers);
    }
    values
}

fn superpowers_display(profile: &SkillProfileConfig) -> RainyResult<Vec<String>> {
    let program = std::env::var("RAINY_SKILLS_BIN").unwrap_or_else(|_| "npx".to_string());
    let mut values = vec![program];
    if std::env::var_os("RAINY_SKILLS_BIN").is_none() {
        values.extend([
            "--yes".to_string(),
            "--package".to_string(),
            profile.packages.skills.clone().ok_or_else(|| {
                RainyError::config(
                    "SKILL_SKILLS_PACKAGE_REQUIRED",
                    "comet profile requires packages.skills",
                )
            })?,
            "skills".to_string(),
        ]);
    }
    values.extend(
        superpowers_args(profile)?
            .into_iter()
            .map(|value| value.to_string_lossy().to_string()),
    );
    Ok(values)
}

fn check_comet_prerequisites() -> RainyResult<()> {
    let failed = comet_prerequisite_checks()
        .into_iter()
        .filter(|check| check.status == "fail")
        .map(|check| check.message)
        .collect::<Vec<_>>();
    if failed.is_empty() {
        Ok(())
    } else {
        Err(RainyError::config(
            "SKILL_PREREQUISITE_MISSING",
            failed.join("; "),
        ))
    }
}

fn comet_prerequisite_checks() -> Vec<SkillCheck> {
    let custom_comet = std::env::var_os("RAINY_COMET_BIN").is_some();
    let custom_skills = std::env::var_os("RAINY_SKILLS_BIN").is_some();
    if custom_comet && custom_skills {
        return vec![
            pass(
                "comet.runner",
                "using the command configured by RAINY_COMET_BIN",
            ),
            pass(
                "superpowers.runner",
                "using the command configured by RAINY_SKILLS_BIN",
            ),
        ];
    }
    let mut checks = Vec::new();
    match command_version("node") {
        Ok(raw) => {
            let version = raw.trim().trim_start_matches('v');
            match Version::parse(version) {
                Ok(version) if version.major >= 20 => checks.push(pass(
                    "prerequisite.node",
                    format!("Node.js {version} satisfies >=20"),
                )),
                Ok(version) => checks.push(fail(
                    "prerequisite.node",
                    format!("Node.js {version} is too old; Comet requires >=20"),
                )),
                Err(_) => checks.push(fail(
                    "prerequisite.node",
                    format!("cannot parse Node.js version: {raw}"),
                )),
            }
        }
        Err(message) => checks.push(fail("prerequisite.node", message)),
    }
    if custom_comet {
        checks.push(pass(
            "comet.runner",
            "using the command configured by RAINY_COMET_BIN",
        ));
    }
    if custom_skills {
        checks.push(pass(
            "superpowers.runner",
            "using the command configured by RAINY_SKILLS_BIN",
        ));
    }
    for command in ["npx", "git"] {
        if command == "npx" && custom_comet && custom_skills {
            continue;
        }
        match command_version(command) {
            Ok(version) => checks.push(pass(
                format!("prerequisite.{command}"),
                format!("{command} is available ({})", version.trim()),
            )),
            Err(message) => checks.push(fail(format!("prerequisite.{command}"), message)),
        }
    }
    checks
}

fn command_version(program: &str) -> Result<String, String> {
    let executable = if cfg!(windows) && program == "npx" {
        "npx.cmd"
    } else {
        program
    };
    let output = Command::new(executable)
        .arg("--version")
        .output()
        .map_err(|error| format!("{program} is not available: {error}"))?;
    if !output.status.success() {
        return Err(format!("{program} --version exited with {}", output.status));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn configure_comet(workspace: &Path) -> RainyResult<()> {
    let path = workspace.join(".comet/config.yaml");
    let mut root = if path.is_file() {
        serde_yaml::from_str::<serde_yaml::Value>(&std::fs::read_to_string(&path)?)?
    } else {
        serde_yaml::Value::Mapping(serde_yaml::Mapping::new())
    };
    let mapping = root.as_mapping_mut().ok_or_else(|| {
        RainyError::config(
            "SKILL_COMET_CONFIG_INVALID",
            ".comet/config.yaml must contain a YAML mapping",
        )
    })?;
    mapping.insert(
        serde_yaml::Value::String("auto_transition".to_string()),
        serde_yaml::Value::Bool(false),
    );
    write_yaml_atomic(&path, &root)
}

fn check_comet_policy(workspace: &Path) -> RainyResult<SkillCheck> {
    let path = workspace.join(".comet/config.yaml");
    if !path.is_file() {
        return Ok(fail(
            "policy.auto-transition",
            ".comet/config.yaml is missing",
        ));
    }
    let root: serde_yaml::Value = serde_yaml::from_str(&std::fs::read_to_string(path)?)?;
    let value = root
        .as_mapping()
        .and_then(|mapping| mapping.get(serde_yaml::Value::String("auto_transition".to_string())))
        .and_then(serde_yaml::Value::as_bool);
    if value == Some(false) {
        Ok(pass(
            "policy.auto-transition",
            "Comet auto_transition is disabled",
        ))
    } else {
        Ok(fail(
            "policy.auto-transition",
            "Comet auto_transition must be false for Rainy-managed workflows",
        ))
    }
}

fn skills_root(workspace: &Path, target: &str) -> RainyResult<PathBuf> {
    Ok(workspace.join(target_relative_root(target)?))
}

fn upstream_roots(workspace: &Path, target: &str) -> RainyResult<Vec<PathBuf>> {
    let mut roots = upstream_relative_roots(target)?
        .into_iter()
        .map(|root| workspace.join(root))
        .collect::<Vec<_>>();
    roots.sort();
    roots.dedup();
    Ok(roots)
}

fn upstream_relative_roots(target: &str) -> RainyResult<Vec<&'static Path>> {
    let target_root = Path::new(target_relative_root(target)?);
    let mut roots = vec![target_root];
    if target == "codex" {
        roots.push(Path::new(".codex/skills"));
        roots.push(Path::new(".agent/skills"));
    } else if target == "cursor" {
        roots.push(Path::new(".agents/skills"));
    }
    Ok(roots)
}

fn target_relative_root(target: &str) -> RainyResult<&'static str> {
    match target {
        "universal" => Ok(".agents/skills"),
        "codex" => Ok(".agents/skills"),
        "claude" => Ok(".claude/skills"),
        "cursor" => Ok(".cursor/skills"),
        "github-copilot" => Ok(".github/skills"),
        "gemini" => Ok(".gemini/skills"),
        "opencode" => Ok(".opencode/skills"),
        _ => Err(RainyError::config(
            "SKILL_TARGET_UNSUPPORTED",
            format!("unsupported skill target: {target}"),
        )),
    }
}

fn profile_name(profile: &SkillProfile) -> &'static str {
    match profile {
        SkillProfile::Rainy => "rainy",
        SkillProfile::Comet => "comet",
    }
}

fn language_name(language: &SkillLanguage) -> &'static str {
    match language {
        SkillLanguage::En => "en",
        SkillLanguage::Zh => "zh",
    }
}

fn target_name(target: &SkillTarget) -> &'static str {
    match target {
        SkillTarget::Universal => "universal",
        SkillTarget::Codex => "codex",
        SkillTarget::Claude => "claude",
        SkillTarget::Cursor => "cursor",
        SkillTarget::GithubCopilot => "github-copilot",
        SkillTarget::Gemini => "gemini",
        SkillTarget::Opencode => "opencode",
    }
}

fn resolve_apply_flags(dry_run: bool, apply: bool) -> RainyResult<bool> {
    if dry_run && apply {
        return Err(RainyError::plan(
            "APPLY_MODE_CONFLICT",
            "--dry-run cannot be combined with --apply or --yes",
        ));
    }
    Ok(apply)
}

fn planned_report(
    operation: &str,
    profile: &SkillProfileConfig,
    apply_command: Vec<String>,
    command: Vec<String>,
) -> SkillReport {
    let mut changed_files = vec![PROFILE_PATH.to_string(), LOCK_PATH.to_string()];
    for target in &profile.targets {
        if let Ok(root) = target_relative_root(target) {
            changed_files.push(format!("{root}/rainy-cli"));
            if profile.profile == "comet" {
                changed_files.push(format!("{root}/rainy-comet"));
            }
        }
    }
    if profile.profile == "comet" {
        changed_files.push(".comet/config.yaml".to_string());
    }
    report(
        operation,
        "dry-run",
        profile,
        changed_files,
        apply_command,
        command,
        Vec::new(),
    )
}

fn completed_report(
    operation: &str,
    profile: &SkillProfileConfig,
    changed_files: Vec<String>,
) -> SkillReport {
    report(
        operation,
        "applied",
        profile,
        changed_files,
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
}

fn report(
    operation: &str,
    status: &str,
    profile: &SkillProfileConfig,
    changed_files: Vec<String>,
    apply_command: Vec<String>,
    command: Vec<String>,
    checks: Vec<SkillCheck>,
) -> SkillReport {
    SkillReport {
        protocol_version: "rainy.skill-report.v1".to_string(),
        status: status.to_string(),
        operation: operation.to_string(),
        profile: profile.profile.clone(),
        scope: profile.scope.clone(),
        language: profile.language.clone(),
        targets: profile.targets.clone(),
        changed_files,
        apply_command,
        command,
        checks,
    }
}

fn init_apply_command(profile: &SkillProfileConfig, force: bool) -> Vec<String> {
    let mut command = vec![
        "rainy".to_string(),
        "skill".to_string(),
        "init".to_string(),
        "--profile".to_string(),
        profile.profile.clone(),
        "--language".to_string(),
        profile.language.clone(),
        "--target".to_string(),
        profile.targets.join(","),
    ];
    if let Some(package) = &profile.packages.comet
        && let Some(version) = package.strip_prefix(&format!("{COMET_PACKAGE}@"))
    {
        command.push("--comet-version".to_string());
        command.push(version.to_string());
    }
    append_upstream_version_flags(&mut command, profile);
    append_apply_flags(&mut command, force);
    command
}

fn update_apply_command(profile: &SkillProfileConfig, force: bool) -> Vec<String> {
    let mut command = vec![
        "rainy".to_string(),
        "skill".to_string(),
        "update".to_string(),
    ];
    if let Some(package) = &profile.packages.comet
        && let Some(version) = package.strip_prefix(&format!("{COMET_PACKAGE}@"))
    {
        command.push("--comet-version".to_string());
        command.push(version.to_string());
    }
    append_upstream_version_flags(&mut command, profile);
    append_apply_flags(&mut command, force);
    command
}

fn append_upstream_version_flags(command: &mut Vec<String>, profile: &SkillProfileConfig) {
    if let Some(package) = &profile.packages.skills
        && let Some(version) = package.strip_prefix(&format!("{SKILLS_PACKAGE}@"))
    {
        command.push("--skills-version".to_string());
        command.push(version.to_string());
    }
    if let Some(package) = &profile.packages.superpowers
        && let Some(version) = package.strip_prefix(&format!("{SUPERPOWERS_PACKAGE}@"))
    {
        command.push("--superpowers-version".to_string());
        command.push(version.to_string());
    }
}

fn change_apply_command(operation: &str, force: bool) -> Vec<String> {
    let mut command = vec![
        "rainy".to_string(),
        "skill".to_string(),
        operation.to_string(),
    ];
    append_apply_flags(&mut command, force);
    command
}

fn append_apply_flags(command: &mut Vec<String>, force: bool) {
    if force {
        command.push("--force".to_string());
    }
    command.push("--apply".to_string());
}

fn pass(id: impl Into<String>, message: impl Into<String>) -> SkillCheck {
    SkillCheck {
        id: id.into(),
        status: "pass".to_string(),
        message: message.into(),
    }
}

fn fail(id: impl Into<String>, message: impl Into<String>) -> SkillCheck {
    SkillCheck {
        id: id.into(),
        status: "fail".to_string(),
        message: message.into(),
    }
}

fn write_yaml_atomic(path: &Path, value: &impl Serialize) -> RainyResult<()> {
    let content = serde_yaml::to_string(value)?;
    write_text_atomic(path, &content)
}

fn write_json_atomic(path: &Path, value: &impl Serialize) -> RainyResult<()> {
    let mut content = serde_json::to_string_pretty(value)?;
    content.push('\n');
    write_text_atomic(path, &content)
}

fn write_text_atomic(path: &Path, content: &str) -> RainyResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("rainy-yaml");
    let temporary = path.with_file_name(format!(".{file_name}.rainy-new-{}", std::process::id()));
    std::fs::write(&temporary, content)?;
    if path.exists() {
        let backup =
            path.with_file_name(format!(".{file_name}.rainy-backup-{}", std::process::id()));
        if backup.exists() {
            std::fs::remove_file(&backup)?;
        }
        std::fs::rename(path, &backup)?;
        if let Err(error) = std::fs::rename(&temporary, path) {
            let _ = std::fs::rename(&backup, path);
            return Err(error.into());
        }
        std::fs::remove_file(backup)?;
    } else {
        std::fs::rename(temporary, path)?;
    }
    Ok(())
}

fn replace_directory(source: &Path, destination: &Path) -> RainyResult<()> {
    let parent = destination.parent().ok_or_else(|| {
        RainyError::config(
            "SKILL_TARGET_INVALID",
            format!("skill target has no parent: {}", destination.display()),
        )
    })?;
    std::fs::create_dir_all(parent)?;
    let name = destination
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("skill");
    let temporary = parent.join(format!(".{name}.rainy-new-{}", std::process::id()));
    let backup = parent.join(format!(".{name}.rainy-backup-{}", std::process::id()));
    if temporary.exists() {
        std::fs::remove_dir_all(&temporary)?;
    }
    if backup.exists() {
        std::fs::remove_dir_all(&backup)?;
    }
    copy_directory(source, &temporary)?;
    if destination.exists() {
        std::fs::rename(destination, &backup)?;
    }
    if let Err(error) = std::fs::rename(&temporary, destination) {
        if backup.exists() {
            let _ = std::fs::rename(&backup, destination);
        }
        return Err(error.into());
    }
    if backup.exists() {
        std::fs::remove_dir_all(backup)?;
    }
    Ok(())
}

fn copy_directory(source: &Path, destination: &Path) -> RainyResult<()> {
    for entry in WalkDir::new(source) {
        let entry = entry.map_err(|error| {
            RainyError::config(
                "SKILL_ASSET_READ_FAILED",
                format!("cannot read bundled skill asset: {error}"),
            )
        })?;
        let relative = entry.path().strip_prefix(source).map_err(|error| {
            RainyError::config(
                "SKILL_ASSET_READ_FAILED",
                format!("cannot resolve bundled skill asset: {error}"),
            )
        })?;
        let target = destination.join(relative);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(target)?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(entry.path(), target)?;
        } else {
            return Err(RainyError::config(
                "SKILL_ASSET_TYPE_UNSUPPORTED",
                format!(
                    "bundled Skill contains an unsupported file type: {}",
                    entry.path().display()
                ),
            ));
        }
    }
    Ok(())
}

fn directory_digest(path: &Path) -> RainyResult<String> {
    if !path.is_dir() {
        return Err(RainyError::config(
            "SKILL_DIRECTORY_MISSING",
            format!("skill directory is missing: {}", path.display()),
        ));
    }
    if std::fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(RainyError::config(
            "SKILL_SYMLINK_UNSUPPORTED",
            format!(
                "managed Skill directory must not be a symbolic link: {}",
                path.display()
            ),
        ));
    }
    let mut files = Vec::new();
    for entry in WalkDir::new(path) {
        let entry = entry.map_err(|error| {
            RainyError::config(
                "SKILL_DIGEST_FAILED",
                format!("cannot traverse managed Skill directory: {error}"),
            )
        })?;
        if entry.file_type().is_symlink() {
            return Err(RainyError::config(
                "SKILL_SYMLINK_UNSUPPORTED",
                format!(
                    "managed Skill content must not contain symbolic links: {}",
                    entry.path().display()
                ),
            ));
        }
        if entry.file_type().is_file() {
            files.push(entry.path().to_path_buf());
        }
    }
    files.sort();
    let mut hasher = Sha256::new();
    for file in files {
        let relative = file.strip_prefix(path).map_err(|error| {
            RainyError::config(
                "SKILL_DIGEST_FAILED",
                format!("cannot resolve skill file: {error}"),
            )
        })?;
        hasher.update(relative.to_string_lossy().as_bytes());
        hasher.update([0]);
        hasher.update(std::fs::read(file)?);
        hasher.update([0]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn paths_digest(paths: &[PathBuf]) -> RainyResult<String> {
    let mut paths = paths.to_vec();
    paths.sort();
    let mut hasher = Sha256::new();
    for path in paths {
        hasher.update(
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .as_bytes(),
        );
        hasher.update([0]);
        hasher.update(directory_digest(&path)?.as_bytes());
        hasher.update([0]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn relative_string(workspace: &Path, path: &Path) -> String {
    path.strip_prefix(workspace)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn truncate(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        value.to_string()
    } else {
        format!("{}...", value.chars().take(max).collect::<String>())
    }
}
