use crate::cli::{BuildCommand, BuildSubcommand};
use crate::config::{self, BuildSystem, BuildTarget};
use crate::error::{RainyError, RainyResult};
use crate::output::CommandOutput;
use crate::process::{self, Termination};
use globset::{Glob, GlobSetBuilder};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Duration;
use walkdir::WalkDir;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildOperation {
    Detect,
    Check,
    Package,
}

impl BuildOperation {
    fn as_str(self) -> &'static str {
        match self {
            Self::Detect => "detect",
            Self::Check => "check",
            Self::Package => "package",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildReport {
    pub protocol_version: String,
    pub status: String,
    pub target: String,
    pub operation: String,
    pub profile: String,
    pub system: String,
    pub root: String,
    pub executable: String,
    pub wrapper: bool,
    pub args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u128>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub stdout_truncated: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub stderr_truncated: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub timed_out: bool,
    pub artifacts: Vec<String>,
    pub warnings: Vec<String>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug)]
struct ResolvedBuild {
    system: BuildSystem,
    root: PathBuf,
    executable: PathBuf,
    wrapper: bool,
    warnings: Vec<String>,
}

pub fn handle_build_command(workspace: &Path, command: BuildCommand) -> RainyResult<CommandOutput> {
    let (operation, args) = match command.command {
        BuildSubcommand::Detect(args) => (BuildOperation::Detect, args),
        BuildSubcommand::Check(args) => (BuildOperation::Check, args),
        BuildSubcommand::Package(args) => (BuildOperation::Package, args),
    };
    Ok(CommandOutput::Build {
        report: run(workspace, &args.target, operation, &args.profile)?,
    })
}

pub fn run(
    workspace: &Path,
    target_name: &str,
    operation: BuildOperation,
    profile: &str,
) -> RainyResult<BuildReport> {
    if !matches!(profile, "local" | "ci") {
        return Err(RainyError::config(
            "BUILD_PROFILE_INVALID",
            format!("build profile must be local or ci, got {profile}"),
        ));
    }
    let config = config::load_config(workspace)?;
    let target = config.build.targets.get(target_name).ok_or_else(|| {
        RainyError::config(
            "BUILD_TARGET_NOT_FOUND",
            format!("build target not found: {target_name}"),
        )
    })?;
    let resolved = resolve_build(workspace, target, profile == "ci")?;
    let args = operation_args(operation, target, resolved.system);
    let root = relative_display(workspace, &resolved.root);
    let executable = relative_display(&resolved.root, &resolved.executable);

    if operation == BuildOperation::Detect {
        return Ok(BuildReport {
            protocol_version: "rainy.build.v1".to_string(),
            status: if resolved.warnings.is_empty() {
                "passed"
            } else {
                "warning"
            }
            .to_string(),
            target: target_name.to_string(),
            operation: operation.as_str().to_string(),
            profile: profile.to_string(),
            system: system_name(resolved.system).to_string(),
            root,
            executable,
            wrapper: resolved.wrapper,
            args,
            duration_ms: None,
            exit_code: None,
            stdout: None,
            stderr: None,
            stdout_truncated: false,
            stderr_truncated: false,
            timed_out: false,
            artifacts: collect_artifacts(workspace, target)?,
            warnings: resolved.warnings,
        });
    }

    let program = resolved.executable.to_string_lossy().to_string();
    let output = process::run(
        &program,
        &args,
        &resolved.root,
        Duration::from_secs(1800),
        process::DEFAULT_OUTPUT_LIMIT,
    )?;
    let timed_out = output.termination == Termination::TimedOut;
    let status = if output.success() {
        if resolved.warnings.is_empty() {
            "passed"
        } else {
            "warning"
        }
    } else {
        "failed"
    };
    Ok(BuildReport {
        protocol_version: "rainy.build.v1".to_string(),
        status: status.to_string(),
        target: target_name.to_string(),
        operation: operation.as_str().to_string(),
        profile: profile.to_string(),
        system: system_name(resolved.system).to_string(),
        root,
        executable,
        wrapper: resolved.wrapper,
        args,
        duration_ms: Some(output.duration.as_millis()),
        exit_code: output.status.and_then(|value| value.code()),
        stdout: Some(output.stdout),
        stderr: Some(output.stderr),
        stdout_truncated: output.stdout_truncated,
        stderr_truncated: output.stderr_truncated,
        timed_out,
        artifacts: collect_artifacts(workspace, target)?,
        warnings: resolved.warnings,
    })
}

fn resolve_build(workspace: &Path, target: &BuildTarget, ci: bool) -> RainyResult<ResolvedBuild> {
    let (system, root) = resolve_target_definition(workspace, target)?;
    let wrapper = find_wrapper(workspace, &root, system);
    if ci && wrapper.is_none() {
        return Err(RainyError::verify(
            "BUILD_WRAPPER_REQUIRED",
            format!(
                "{} target {} must use a checked-in wrapper in ci",
                system_name(system),
                target.root
            ),
        ));
    }
    let mut warnings = Vec::new();
    let (executable, wrapper) = if let Some(wrapper) = wrapper {
        (wrapper, true)
    } else {
        warnings.push(format!(
            "{} wrapper is missing; using the system executable for local verification",
            system_name(system)
        ));
        (PathBuf::from(system_program(system)), false)
    };
    let command_root = if wrapper {
        executable.parent().unwrap_or(&root).to_path_buf()
    } else {
        root
    };
    Ok(ResolvedBuild {
        system,
        root: command_root,
        executable,
        wrapper,
        warnings,
    })
}

pub(crate) fn resolve_target_definition(
    workspace: &Path,
    target: &BuildTarget,
) -> RainyResult<(BuildSystem, PathBuf)> {
    let root = safe_target_root(workspace, &target.root)?;
    let system = match target.system {
        BuildSystem::Auto => detect_system(workspace, &root)?,
        explicit => explicit,
    };
    Ok((system, root))
}

fn safe_target_root(workspace: &Path, value: &str) -> RainyResult<PathBuf> {
    let relative = Path::new(value);
    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(RainyError::config(
            "BUILD_ROOT_INVALID",
            format!("build root must stay inside the workspace: {value}"),
        ));
    }
    let root = workspace.join(relative);
    if !root.is_dir() {
        return Err(RainyError::config(
            "BUILD_ROOT_NOT_FOUND",
            format!("build root does not exist: {value}"),
        ));
    }
    Ok(root)
}

fn detect_system(workspace: &Path, root: &Path) -> RainyResult<BuildSystem> {
    let maven = find_marker(workspace, root, &["mvnw", "mvnw.cmd", "pom.xml"]);
    let gradle = find_marker(
        workspace,
        root,
        &["gradlew", "gradlew.bat", "settings.gradle", "build.gradle"],
    );
    match (maven, gradle) {
        (true, false) => Ok(BuildSystem::Maven),
        (false, true) => Ok(BuildSystem::Gradle),
        (true, true) => Err(RainyError::config(
            "BUILD_SYSTEM_AMBIGUOUS",
            "both Maven and Gradle markers apply to this build target; set build.targets.<name>.system",
        )),
        (false, false) => Err(RainyError::config(
            "BUILD_SYSTEM_NOT_FOUND",
            "no Maven or Gradle build descriptor was found for this target",
        )),
    }
}

fn find_marker(workspace: &Path, root: &Path, names: &[&str]) -> bool {
    ancestors_inside(workspace, root)
        .any(|candidate| names.iter().any(|name| candidate.join(name).is_file()))
}

fn find_wrapper(workspace: &Path, root: &Path, system: BuildSystem) -> Option<PathBuf> {
    let names: &[&str] = match system {
        BuildSystem::Maven => {
            if cfg!(windows) {
                &["mvnw.cmd", "mvnw"]
            } else {
                &["mvnw", "mvnw.cmd"]
            }
        }
        BuildSystem::Gradle => {
            if cfg!(windows) {
                &["gradlew.bat", "gradlew"]
            } else {
                &["gradlew", "gradlew.bat"]
            }
        }
        BuildSystem::Auto => return None,
    };
    ancestors_inside(workspace, root).find_map(|candidate| {
        names
            .iter()
            .map(|name| candidate.join(name))
            .find(|path| path.is_file())
    })
}

fn ancestors_inside<'a>(workspace: &'a Path, root: &'a Path) -> impl Iterator<Item = &'a Path> {
    root.ancestors()
        .take_while(move |candidate| candidate.starts_with(workspace))
}

fn operation_args(
    operation: BuildOperation,
    target: &BuildTarget,
    system: BuildSystem,
) -> Vec<String> {
    let configured = match operation {
        BuildOperation::Detect => return Vec::new(),
        BuildOperation::Check => &target.check_args,
        BuildOperation::Package => &target.package_args,
    };
    if !configured.is_empty() {
        return configured.clone();
    }
    let defaults: &[&str] = match (system, operation) {
        (BuildSystem::Maven, BuildOperation::Check) => {
            &["--batch-mode", "--no-transfer-progress", "verify"]
        }
        (BuildSystem::Maven, BuildOperation::Package) => &[
            "--batch-mode",
            "--no-transfer-progress",
            "package",
            "-DskipTests",
        ],
        (BuildSystem::Gradle, BuildOperation::Check) => &["--no-daemon", "clean", "check"],
        (BuildSystem::Gradle, BuildOperation::Package) => {
            &["--no-daemon", "assemble", "-x", "test"]
        }
        _ => return Vec::new(),
    };
    defaults
        .into_iter()
        .map(|value| (*value).to_string())
        .collect()
}

fn collect_artifacts(workspace: &Path, target: &BuildTarget) -> RainyResult<Vec<String>> {
    if target.artifacts.is_empty() {
        return Ok(Vec::new());
    }
    let mut builder = GlobSetBuilder::new();
    for pattern in &target.artifacts {
        builder.add(Glob::new(pattern).map_err(|error| {
            RainyError::config(
                "BUILD_ARTIFACT_PATTERN_INVALID",
                format!("invalid artifact pattern {pattern}: {error}"),
            )
        })?);
    }
    let patterns = builder
        .build()
        .map_err(|error| RainyError::config("BUILD_ARTIFACT_PATTERN_INVALID", error.to_string()))?;
    let root = workspace.join(&target.root);
    let mut artifacts = WalkDir::new(&root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| {
            let relative = entry.path().strip_prefix(&root).ok()?;
            patterns.is_match(relative).then(|| {
                entry
                    .path()
                    .strip_prefix(workspace)
                    .unwrap_or(entry.path())
                    .to_string_lossy()
                    .replace('\\', "/")
            })
        })
        .collect::<Vec<_>>();
    artifacts.sort();
    Ok(artifacts)
}

fn system_name(system: BuildSystem) -> &'static str {
    match system {
        BuildSystem::Auto => "auto",
        BuildSystem::Maven => "maven",
        BuildSystem::Gradle => "gradle",
    }
}

fn system_program(system: BuildSystem) -> &'static str {
    match system {
        BuildSystem::Maven => "mvn",
        BuildSystem::Gradle => "gradle",
        BuildSystem::Auto => "",
    }
}

fn relative_display(base: &Path, path: &Path) -> String {
    path.strip_prefix(base)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn detects_maven_gradle_and_ambiguity() {
        let temp = TempDir::new().expect("tempdir");
        let root = temp.path();
        fs::write(root.join("pom.xml"), "<project/>").expect("pom");
        assert_eq!(
            detect_system(root, root).expect("maven"),
            BuildSystem::Maven
        );
        fs::write(root.join("settings.gradle"), "").expect("settings");
        assert!(matches!(
            detect_system(root, root),
            Err(RainyError::Config { code, .. }) if code == "BUILD_SYSTEM_AMBIGUOUS"
        ));
        fs::remove_file(root.join("pom.xml")).expect("remove pom");
        assert_eq!(
            detect_system(root, root).expect("gradle"),
            BuildSystem::Gradle
        );
    }

    #[test]
    fn rejects_build_roots_outside_workspace() {
        let temp = TempDir::new().expect("tempdir");
        assert!(matches!(
            safe_target_root(temp.path(), "../outside"),
            Err(RainyError::Config { code, .. }) if code == "BUILD_ROOT_INVALID"
        ));
    }
}
