use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Output};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

static HTTP_PLUGIN_TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn progress_is_visible_on_demand_and_never_corrupts_json_or_quiet_output() {
    let output = run(&["--progress", "always", "capability", "list"]);
    let progress = String::from_utf8(output.stderr).expect("progress output");
    assert!(progress.contains("[1/4] Preparing capability"));
    assert!(progress.contains("[2/4] Running capability"));
    assert!(progress.contains("[4/4] Completed in"));

    let output = run(&["--progress", "always", "--json", "capability", "list"]);
    assert!(output.stderr.is_empty(), "JSON mode emitted progress");
    serde_json::from_slice::<serde_json::Value>(&output.stdout).expect("valid JSON output");

    let output = run(&["--progress", "always", "--quiet", "capability", "list"]);
    assert!(output.stderr.is_empty(), "quiet mode emitted progress");

    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "progress-demo", "--apply"]);
    let workspace = temp.path().join("progress-demo");
    let workspace = workspace.to_string_lossy().to_string();
    let output = run(&[
        "--workspace",
        &workspace,
        "--progress",
        "always",
        "skill",
        "init",
        "--profile",
        "rainy",
    ]);
    let progress = String::from_utf8(output.stderr).expect("skill progress");
    assert!(progress.contains("Validating workspace and requested Skill profile"));
    assert!(progress.contains("Building the Skill installation preview"));

    let help = String::from_utf8(run(&["--help"]).stdout).expect("top-level help");
    assert!(help.contains("--progress <MODE>"));
    assert!(help.contains("[possible values: auto, always, never]"));
}

#[test]
fn help_describes_every_command_and_leaf_with_business_placeholders_and_examples() {
    let help = String::from_utf8(run(&["--help"]).stdout).expect("top-level help");
    assert!(help.contains("Arguments shown as <VALUE> are required values"));
    assert!(help.contains("init         Initialize a Rainy application"));
    assert!(help.contains("self         Check, install, or skip Rainy CLI updates"));
    assert!(help.contains("--workspace <PROJECT_DIR>"));
    assert!(help.contains("QUICK START:"));

    let groups: &[&[&str]] = &[
        &["init"],
        &["add"],
        &["capability"],
        &["pack"],
        &["evidence"],
        &["plugin"],
        &["agent"],
        &["skill"],
        &["conformance"],
        &["schema"],
        &["self"],
    ];
    for path in groups {
        let mut args = path.to_vec();
        args.push("--help");
        let help = String::from_utf8(run(&args).stdout).expect("command group help");
        assert!(
            help.contains("EXAMPLES:") || help.contains("QUICK START:"),
            "missing examples for rainy {}",
            path.join(" ")
        );
    }

    let leaves: &[&[&str]] = &[
        &["init", "app"],
        &["new"],
        &["add", "capability"],
        &["apply"],
        &["capability", "list"],
        &["capability", "explain"],
        &["capability", "graph"],
        &["capability", "installed"],
        &["capability", "upgrade"],
        &["capability", "remove"],
        &["pack", "list"],
        &["pack", "inspect"],
        &["pack", "install"],
        &["pack", "update"],
        &["pack", "sign"],
        &["pack", "verify"],
        &["doctor"],
        &["verify"],
        &["evidence", "generate"],
        &["plugin", "list"],
        &["plugin", "inspect"],
        &["plugin", "install"],
        &["plugin", "call"],
        &["agent", "init"],
        &["agent", "context"],
        &["conformance", "check"],
        &["schema", "list"],
        &["schema", "validate"],
        &["self", "check"],
        &["self", "update"],
        &["self", "skip"],
    ];
    for path in leaves {
        let mut args = path.to_vec();
        args.push("--help");
        let help = String::from_utf8(run(&args).stdout).expect("leaf command help");
        let invocation = format!("rainy {}", path.join(" "));
        assert!(
            help.contains("EXAMPLES:"),
            "missing examples for {invocation}"
        );
        assert!(
            help.contains(&invocation),
            "missing runnable example for {invocation}"
        );
    }

    let capability_help =
        String::from_utf8(run(&["add", "capability", "--help"]).stdout).expect("add help");
    assert!(capability_help.contains("<CAPABILITY_ID>"));
    assert!(capability_help.contains("--output-plan <PLAN_FILE>"));

    let self_help =
        String::from_utf8(run(&["self", "update", "--help"]).stdout).expect("self help");
    assert!(self_help.contains("--repo <OWNER/REPO>"));
    assert!(self_help.contains("--version <VERSION>"));
}

#[test]
fn skill_help_explains_the_workflow_and_each_subcommand() {
    let help = String::from_utf8(run(&["skill", "--help"]).stdout).expect("skill help");
    assert!(help.contains("Manage a project-scoped AI Skill profile"));
    assert!(help.contains("Mutating commands preview changes by default"));
    assert!(help.contains("rainy skill init --apply"));
    assert!(help.contains("Run 'rainy skill <COMMAND> --help'"));

    for command in [
        "init",
        "install",
        "sync",
        "status",
        "doctor",
        "update",
        "uninstall",
    ] {
        let help = String::from_utf8(run(&["skill", command, "--help"]).stdout)
            .expect("skill subcommand help");
        assert!(help.contains("EXAMPLES:"), "missing examples for {command}");
        assert!(
            help.contains(&format!("rainy skill {command}")),
            "missing runnable example for {command}"
        );
    }

    let init_help =
        String::from_utf8(run(&["skill", "init", "--help"]).stdout).expect("skill init help");
    assert!(init_help.contains("--yes"));
    assert!(init_help.contains("alias for --apply"));
    assert!(init_help.contains("[default: comet]"));
    assert!(init_help.contains("[default: zh]"));
    assert!(init_help.contains("[default: codex]"));
}

fn rainy() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rainy"));
    command.env("RAINY_ALLOW_NATIVE_PLUGIN", "1");
    command
}

fn run(args: &[&str]) -> Output {
    run_with_env(args, &[])
}

fn run_without_external_tools(args: &[&str]) -> Output {
    run_with_env(args, &[("PATH", "")])
}

fn run_with_env(args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut command = rainy();
    command.args(args);
    for (key, value) in envs {
        command.env(key, value);
    }
    let output = command.output().expect("run rainy");
    if !output.status.success() {
        panic!(
            "rainy failed\nargs: {args:?}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    output
}

#[test]
fn self_check_reuses_the_persisted_release_mirror() {
    let temp = TempDir::new().expect("tempdir");
    let server_root = temp.path().join("server");
    let rainy_home = temp.path().join("rainy-home");
    write(&server_root.join("latest.txt"), "v9.9.9\n");
    let release_base = serve_static(server_root, 1);
    write(&rainy_home.join("release-source"), &release_base);

    let mut command = rainy();
    let output = command
        .args(["self", "check", "--json"])
        .env("RAINY_HOME", &rainy_home)
        .output()
        .expect("run mirrored self check");
    assert!(
        output.status.success(),
        "mirrored self check failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("update report JSON");
    assert_eq!(report["report"]["latestVersion"], "9.9.9");
    assert!(
        report["report"]["installCommand"]
            .as_str()
            .expect("install command")
            .contains(&release_base)
    );
}

#[test]
fn rainy_skill_profile_has_a_safe_project_lifecycle() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "skill-app"]);
    let app = temp.path().join("skill-app");
    let app_path = app.to_string_lossy().to_string();

    let preview = run(&[
        "--workspace",
        &app_path,
        "skill",
        "init",
        "--profile",
        "rainy",
        "--target",
        "codex",
        "--dry-run",
        "--json",
    ]);
    let preview_json: serde_json::Value =
        serde_json::from_slice(&preview.stdout).expect("skill preview json");
    assert_eq!(preview_json["type"], "skill");
    assert_eq!(preview_json["report"]["status"], "dry-run");
    assert_eq!(
        preview_json["report"]["applyCommand"],
        serde_json::json!([
            "rainy",
            "skill",
            "init",
            "--profile",
            "rainy",
            "--language",
            "zh",
            "--target",
            "codex",
            "--apply"
        ])
    );
    let report_path = temp.path().join("skill-preview-report.json");
    fs::write(
        &report_path,
        serde_json::to_vec_pretty(&preview_json["report"]).expect("serialize skill report"),
    )
    .expect("write skill report");
    run(&[
        "schema",
        "validate",
        "--schema",
        "skill-report",
        "--file",
        &report_path.to_string_lossy(),
        "--json",
    ]);
    let conflict = rainy()
        .args([
            "--workspace",
            &app_path,
            "skill",
            "init",
            "--profile",
            "rainy",
            "--dry-run",
            "--yes",
            "--json",
        ])
        .output()
        .expect("run conflicting skill apply modes");
    assert!(!conflict.status.success());
    assert!(String::from_utf8_lossy(&conflict.stderr).contains("APPLY_MODE_CONFLICT"));
    assert!(!app.join("rainy-skills.yaml").exists());
    assert!(!app.join("skills.lock").exists());

    run(&[
        "--workspace",
        &app_path,
        "skill",
        "init",
        "--profile",
        "rainy",
        "--target",
        "codex",
        "--yes",
        "--json",
    ]);
    assert!(app.join("rainy-skills.yaml").is_file());
    assert!(app.join("skills.lock").is_file());
    assert!(app.join(".codex/skills/rainy-cli/SKILL.md").is_file());
    assert!(!app.join(".codex/skills/rainy-comet").exists());

    run(&[
        "--workspace",
        &app_path,
        "schema",
        "validate",
        "--schema",
        "skill-profile",
        "--file",
        &app.join("rainy-skills.yaml").to_string_lossy(),
        "--json",
    ]);
    run(&[
        "--workspace",
        &app_path,
        "schema",
        "validate",
        "--schema",
        "skill-lock",
        "--file",
        &app.join("skills.lock").to_string_lossy(),
        "--json",
    ]);
    let doctor = run(&["--workspace", &app_path, "skill", "doctor", "--json"]);
    let doctor_json: serde_json::Value =
        serde_json::from_slice(&doctor.stdout).expect("skill doctor json");
    assert_eq!(doctor_json["report"]["status"], "passed");

    let lock_path = app.join("skills.lock");
    let valid_lock = fs::read_to_string(&lock_path).expect("valid skills lock");
    let unsafe_lock = valid_lock.replacen(
        "path: .codex/skills/rainy-cli",
        "path: ../outside/rainy-cli",
        1,
    );
    assert_ne!(valid_lock, unsafe_lock, "locked path fixture not found");
    fs::write(&lock_path, unsafe_lock).expect("unsafe skills lock");
    let rejected = rainy()
        .args(["--workspace", &app_path, "skill", "doctor", "--json"])
        .output()
        .expect("run unsafe lock doctor");
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("SKILL_LOCK_PATH_INVALID"));
    fs::write(&lock_path, valid_lock).expect("restore skills lock");

    let agents = app.join("AGENTS.md");
    let existing = fs::read_to_string(&agents).expect("AGENTS.md");
    fs::write(&agents, format!("{existing}\n<!-- user-content -->\n")).expect("extend AGENTS.md");
    run(&["--workspace", &app_path, "skill", "sync", "--json"]);
    let synced = fs::read_to_string(&agents).expect("synced AGENTS.md");
    assert!(synced.contains("<!-- user-content -->"));
    assert_eq!(count(&synced, "<!-- rainy:context:start -->"), 1);

    run(&[
        "--workspace",
        &app_path,
        "skill",
        "uninstall",
        "--apply",
        "--json",
    ]);
    assert!(!app.join("rainy-skills.yaml").exists());
    assert!(!app.join("skills.lock").exists());
    assert!(!app.join(".codex/skills/rainy-cli").exists());
}

#[cfg(unix)]
#[test]
fn comet_skill_profile_uses_pinned_upstream_and_detects_drift() {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "comet-app"]);
    let app = temp.path().join("comet-app");
    let app_path = app.to_string_lossy().to_string();
    let fake_comet = temp.path().join("fake-comet");
    fs::write(
        &fake_comet,
        r##"#!/bin/sh
set -eu
action="$1"
workspace="$2"
case "$action" in
  init)
    for name in comet comet-open; do
      mkdir -p "$workspace/.agents/skills/$name"
      printf '%s\n' '---' "name: $name" "description: test $name" '---' '' "# $name" > "$workspace/.agents/skills/$name/SKILL.md"
    done
    name=openspec-propose
    mkdir -p "$workspace/.codex/skills/$name"
    printf '%s\n' '---' "name: $name" "description: test $name" '---' '' "# $name" > "$workspace/.codex/skills/$name/SKILL.md"
    ;;
  uninstall)
    rm -rf "$workspace/.agents/skills/comet" "$workspace/.agents/skills/comet-open" "$workspace/.codex/skills/openspec-propose"
    ;;
  *)
    exit 2
    ;;
esac
printf '%s\n' '{"status":"ok"}'
"##,
    )
    .expect("fake comet");
    let mut permissions = fs::metadata(&fake_comet).expect("metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_comet, permissions).expect("permissions");
    let fake_path = fake_comet.to_string_lossy().to_string();

    let preview = run(&[
        "--workspace",
        &app_path,
        "skill",
        "init",
        "--profile",
        "comet",
        "--target",
        "codex",
        "--language",
        "zh",
    ]);
    let preview_text = String::from_utf8(preview.stdout).expect("Comet preview output");
    assert!(preview_text.contains("No files were changed."));
    assert!(preview_text.contains("Apply this plan:"));
    assert!(preview_text.contains("rainy skill init --profile comet"));
    assert!(preview_text.contains("Upstream command (runs only when applying):"));
    assert!(preview_text.contains("npx --yes --package @rpamis/comet@0.4.0-beta.6"));

    run_with_env(
        &[
            "--workspace",
            &app_path,
            "skill",
            "init",
            "--profile",
            "comet",
            "--target",
            "codex",
            "--language",
            "zh",
            "--apply",
            "--json",
        ],
        &[("RAINY_COMET_BIN", &fake_path)],
    );
    for path in [
        ".codex/skills/rainy-cli/SKILL.md",
        ".codex/skills/rainy-comet/SKILL.md",
        ".agents/skills/comet/SKILL.md",
        ".agents/skills/comet-open/SKILL.md",
        ".codex/skills/openspec-propose/SKILL.md",
        ".comet/config.yaml",
    ] {
        assert!(app.join(path).is_file(), "missing {path}");
    }
    let comet_config = fs::read_to_string(app.join(".comet/config.yaml")).expect("Comet config");
    assert!(comet_config.contains("auto_transition: false"));
    let lock = fs::read_to_string(app.join("skills.lock")).expect("skills lock");
    assert!(lock.contains("version: 0.4.0-beta.6"));
    assert!(lock.contains("name: openspec"));
    assert!(lock.contains(".agents/skills/comet"));
    assert!(!lock.contains("name: superpowers"));

    let doctor = run_with_env(
        &["--workspace", &app_path, "skill", "doctor", "--json"],
        &[("RAINY_COMET_BIN", &fake_path)],
    );
    let doctor = String::from_utf8_lossy(&doctor.stdout);
    assert!(doctor.contains("\"status\": \"passed\""));
    assert!(doctor.contains("\"status\": \"warn\""));
    assert!(doctor.contains("superpowers skills are optional"));

    fs::write(
        app.join(".codex/skills/rainy-comet/local-edit.txt"),
        "modified\n",
    )
    .expect("modify managed skill");
    let rejected = rainy()
        .args([
            "--workspace",
            &app_path,
            "skill",
            "update",
            "--apply",
            "--json",
        ])
        .env("RAINY_COMET_BIN", &fake_path)
        .output()
        .expect("run drifted update");
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("SKILL_MANAGED_FILES_MODIFIED"));

    run_with_env(
        &[
            "--workspace",
            &app_path,
            "skill",
            "update",
            "--comet-version",
            "0.4.0-beta.7",
            "--apply",
            "--force",
            "--json",
        ],
        &[("RAINY_COMET_BIN", &fake_path)],
    );
    let updated_lock = fs::read_to_string(app.join("skills.lock")).expect("updated lock");
    assert!(updated_lock.contains("version: 0.4.0-beta.7"));

    run_with_env(
        &[
            "--workspace",
            &app_path,
            "skill",
            "uninstall",
            "--apply",
            "--json",
        ],
        &[("RAINY_COMET_BIN", &fake_path)],
    );
    assert!(!app.join(".codex/skills/rainy-comet").exists());
    assert!(!app.join(".agents/skills/comet").exists());
    assert!(!app.join("rainy-skills.yaml").exists());
}

#[cfg(unix)]
#[test]
fn comet_skill_init_failure_is_retryable_without_force() {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "retry-app"]);
    let app = temp.path().join("retry-app");
    let app_path = app.to_string_lossy().to_string();
    let fake_comet = temp.path().join("fake-comet-retry");
    fs::write(
        &fake_comet,
        r##"#!/bin/sh
set -eu
workspace="$2"
mkdir -p "$workspace/.codex/skills/openspec-propose"
printf '%s\n' '---' 'name: openspec-propose' 'description: test' '---' > "$workspace/.codex/skills/openspec-propose/SKILL.md"
printf '%s\n' '{"status":"ok"}'
"##,
    )
    .expect("incomplete fake comet");
    let mut permissions = fs::metadata(&fake_comet).expect("metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_comet, permissions).expect("permissions");
    let fake_path = fake_comet.to_string_lossy().to_string();

    let failed = rainy()
        .args([
            "--workspace",
            &app_path,
            "skill",
            "init",
            "--profile",
            "comet",
            "--target",
            "codex",
            "--language",
            "zh",
            "--apply",
            "--json",
        ])
        .env("RAINY_COMET_BIN", &fake_path)
        .output()
        .expect("run incomplete init");
    assert!(!failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("SKILL_UPSTREAM_INCOMPLETE"));
    assert!(!app.join("rainy-skills.yaml").exists());
    assert!(!app.join("skills.lock").exists());

    // Simulate the partial state left by Rainy <= 0.3.7, which wrote the
    // profile before validating Comet's installed Skills.
    fs::write(
        app.join("rainy-skills.yaml"),
        r#"apiVersion: rainy.dev/v1
kind: SkillProfile
profile: comet
scope: project
language: zh
targets:
- codex
packages:
  comet: '@rpamis/comet@0.4.0-beta.6'
policy:
  autoTransition: false
  requireApplyApproval: true
  verifyProfile: ci
"#,
    )
    .expect("legacy partial profile");

    fs::write(
        &fake_comet,
        r##"#!/bin/sh
set -eu
workspace="$2"
mkdir -p "$workspace/.agents/skills/comet" "$workspace/.codex/skills/openspec-propose"
printf '%s\n' '---' 'name: comet' 'description: test' '---' > "$workspace/.agents/skills/comet/SKILL.md"
printf '%s\n' '---' 'name: openspec-propose' 'description: test' '---' > "$workspace/.codex/skills/openspec-propose/SKILL.md"
printf '%s\n' '{"status":"ok"}'
"##,
    )
    .expect("complete fake comet");

    run_with_env(
        &[
            "--workspace",
            &app_path,
            "skill",
            "init",
            "--profile",
            "comet",
            "--target",
            "codex",
            "--language",
            "zh",
            "--apply",
            "--json",
        ],
        &[("RAINY_COMET_BIN", &fake_path)],
    );
    assert!(app.join("rainy-skills.yaml").is_file());
    assert!(app.join("skills.lock").is_file());
}

#[cfg(unix)]
#[test]
fn native_plugins_require_explicit_trust() {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new().expect("tempdir");
    let plugin = temp.path().join("rainy-untrusted");
    fs::write(&plugin, "#!/bin/sh\necho should-not-run\n").expect("plugin");
    let mut permissions = fs::metadata(&plugin).expect("metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&plugin, permissions).expect("permissions");

    let output = Command::new(env!("CARGO_BIN_EXE_rainy"))
        .arg("untrusted")
        .env("PATH", temp.path())
        .env_remove("RAINY_ALLOW_NATIVE_PLUGIN")
        .output()
        .expect("run rainy");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("PLUGIN_NATIVE_NOT_TRUSTED"));

    let output = Command::new(env!("CARGO_BIN_EXE_rainy"))
        .args(["--allow-native-plugin", "untrusted"])
        .env("PATH", temp.path())
        .output()
        .expect("run trusted plugin outside project");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("PLUGIN_NATIVE_AUDIT_REQUIRED"));

    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "demo-saas"]);
    let app = temp.path().join("demo-saas");
    let config_path = app.join("rainy.yaml");
    let config = fs::read_to_string(&config_path).expect("config");
    fs::write(
        &config_path,
        config.replace("allowNativePlugins: false", "allowNativePlugins: true"),
    )
    .expect("enable native plugin policy");
    let output = Command::new(env!("CARGO_BIN_EXE_rainy"))
        .args(["--workspace", &app.to_string_lossy(), "untrusted"])
        .env("PATH", temp.path())
        .env_remove("RAINY_ALLOW_NATIVE_PLUGIN")
        .output()
        .expect("run policy-trusted plugin");
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("should-not-run"));
    let audit = fs::read_to_string(app.join(".rainy/audit.log")).expect("native plugin audit");
    assert!(audit.contains("\"command\":\"external\""));
}

#[test]
fn golden_path_add_minio_verify_and_evidence() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&[
        "--workspace",
        &root,
        "new",
        "demo-saas",
        "--golden-path",
        "spring-nextjs-saas",
        "--package",
        "com.example.demo",
    ]);
    let app = temp.path().join("demo-saas");
    let app_path = app.to_string_lossy().to_string();
    let generated_ci = fs::read_to_string(app.join(".github/workflows/ci.yml")).expect("ci yml");
    assert!(generated_ci.contains("actions/checkout@v5"));
    assert!(generated_ci.contains("actions/setup-java@v4"));
    assert!(generated_ci.contains("Install Maven"));
    assert!(generated_ci.contains("pnpm install --frozen-lockfile"));
    assert!(app.join("apps/frontend/pnpm-lock.yaml").exists());
    assert!(generated_ci.contains("Install Rainy CLI"));
    assert!(generated_ci.contains("~/.rainy/bin/rainy verify --profile ci --json"));

    run(&[
        "--workspace",
        &app_path,
        "add",
        "capability",
        "minio-file-storage",
        "--dry-run",
        "--json",
    ]);
    assert!(
        !app.join("apps/frontend/src/components/file-upload/FileUpload.tsx")
            .exists()
    );

    run(&[
        "--workspace",
        &app_path,
        "add",
        "capability",
        "minio-file-storage",
        "--apply",
    ]);
    run(&["--workspace", &app_path, "doctor"]);
    let audit_log = app.join(".rainy/audit.log");
    assert!(audit_log.exists());
    let audit = fs::read_to_string(&audit_log).expect("audit log");
    assert!(audit.contains("\"protocolVersion\":\"rainy.audit.v1\""));
    assert!(audit.contains("\"command\":\"add capability\""));
    assert!(audit.contains("\"status\":\"applied\""));
    let first_audit: serde_json::Value =
        serde_json::from_str(audit.lines().next().expect("audit record"))
            .expect("parse audit record");
    assert_eq!(first_audit["protocolVersion"], "rainy.audit.v1");
    let first_audit_path = app.join("first-audit.json");
    fs::write(
        &first_audit_path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&first_audit).expect("audit json")
        ),
    )
    .expect("write audit fixture");
    run(&[
        "schema",
        "validate",
        "--schema",
        "audit",
        "--file",
        &first_audit_path.to_string_lossy(),
    ]);
    let doctor = run(&["--workspace", &app_path, "doctor", "--json"]);
    assert!(String::from_utf8_lossy(&doctor.stdout).contains("DEFAULT_SECRET_VALUE"));
    run_without_external_tools(&["--workspace", &app_path, "verify", "--profile", "local"]);
    run_without_external_tools(&["--workspace", &app_path, "evidence", "generate"]);

    assert!(app.join("evidence/report.md").exists());
    assert!(app.join("evidence/report.json").exists());
    let evidence_md = fs::read_to_string(app.join("evidence/report.md")).expect("evidence md");
    assert!(evidence_md.contains("## Changes"));
    assert!(evidence_md.contains("## Risks"));
    assert!(evidence_md.contains("minio-file-storage"));
    let evidence_json: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(app.join("evidence/report.json")).expect("evidence json"),
    )
    .expect("parse evidence json");
    assert_eq!(evidence_json["protocolVersion"], "rainy.evidence.v1");
    assert!(
        evidence_json["capabilities"]
            .as_array()
            .expect("capabilities array")
            .iter()
            .any(|capability| capability == "minio-file-storage")
    );
    assert_eq!(
        count(
            &fs::read_to_string(app.join("apps/backend/pom.xml")).expect("pom"),
            "<artifactId>minio</artifactId>"
        ),
        1
    );

    let second = run(&[
        "--workspace",
        &app_path,
        "add",
        "capability",
        "minio-file-storage",
        "--apply",
    ]);
    assert!(String::from_utf8_lossy(&second.stdout).contains("already installed"));
}

#[test]
fn new_dry_run_json_does_not_create_project() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    let output = run(&[
        "--workspace",
        &root,
        "new",
        "demo-saas",
        "--golden-path",
        "spring-nextjs-saas",
        "--dry-run",
        "--json",
    ]);

    assert!(!temp.path().join("demo-saas").exists());
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("parse new dry-run json");
    assert_eq!(json["type"], "init");
    assert_eq!(json["status"], "dry-run");
    assert!(
        json["files"]
            .as_array()
            .expect("files array")
            .iter()
            .any(|file| file == "rainy.yaml")
    );
}

#[test]
fn standalone_binary_uses_embedded_packs_and_schemas() {
    let temp = TempDir::new().expect("tempdir");
    let cache = temp.path().join("asset-cache");
    for args in [
        &["capability", "list", "--json"][..],
        &["schema", "list", "--json"][..],
    ] {
        let output = rainy()
            .args(args)
            .current_dir(temp.path())
            .env("RAINY_FORCE_EMBEDDED_ASSETS", "1")
            .env("RAINY_ASSET_CACHE", &cache)
            .output()
            .expect("run standalone command");
        assert!(
            output.status.success(),
            "standalone command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let output = rainy()
        .args(["new", "standalone-app"])
        .current_dir(temp.path())
        .env("RAINY_FORCE_EMBEDDED_ASSETS", "1")
        .env("RAINY_ASSET_CACHE", &cache)
        .output()
        .expect("create standalone project");
    assert!(
        output.status.success(),
        "standalone init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let config = fs::read_to_string(temp.path().join("standalone-app/rainy.yaml"))
        .expect("standalone config");
    assert!(config.contains("sources: []"));
    assert!(!config.contains(&cache.to_string_lossy().to_string()));
    let output = rainy()
        .args([
            "--workspace",
            &temp.path().join("standalone-app").to_string_lossy(),
            "skill",
            "init",
            "--profile",
            "rainy",
            "--target",
            "codex",
            "--apply",
            "--json",
        ])
        .env("RAINY_FORCE_EMBEDDED_ASSETS", "1")
        .env("RAINY_ASSET_CACHE", &cache)
        .output()
        .expect("install embedded Rainy skill");
    assert!(
        output.status.success(),
        "embedded skill install failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        temp.path()
            .join("standalone-app/.codex/skills/rainy-cli/SKILL.md")
            .is_file()
    );
    assert!(
        cache
            .join(format!("rainy-cli-assets-{}", env!("CARGO_PKG_VERSION")))
            .join(".complete")
            .is_file()
    );
    assert!(
        cache
            .join(format!("rainy-cli-assets-{}", env!("CARGO_PKG_VERSION")))
            .join("integrations/skills/rainy-comet/SKILL.md")
            .is_file()
    );
}

#[test]
fn doctor_fails_when_installed_capability_artifact_is_missing() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "demo-saas"]);
    let app = temp.path().join("demo-saas");
    let app_path = app.to_string_lossy().to_string();

    run(&[
        "--workspace",
        &app_path,
        "add",
        "capability",
        "minio-file-storage",
        "--apply",
    ]);
    fs::remove_dir_all(app.join("apps/frontend/src/components/file-upload"))
        .expect("remove frontend upload artifact");

    let output = rainy()
        .args([
            "--workspace",
            &app_path,
            "doctor",
            "--capability",
            "minio-file-storage",
            "--json",
        ])
        .output()
        .expect("run rainy");

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("DOCTOR_FAILED"));
    assert!(stderr.contains("apps/frontend/src/components/file-upload"));
}

#[test]
fn verify_ci_profile_rejects_unknown_steps() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "demo-saas"]);
    let app = temp.path().join("demo-saas");
    let app_path = app.to_string_lossy().to_string();
    let rainy_yaml = app.join("rainy.yaml");
    let config = fs::read_to_string(&rainy_yaml).expect("read rainy.yaml");
    fs::write(
        &rainy_yaml,
        config.replace(
            "      - security-basic\n",
            "      - security-basic\n      - unknown-production-step\n",
        ),
    )
    .expect("write rainy.yaml");

    let output = rainy()
        .args([
            "--workspace",
            &app_path,
            "verify",
            "--profile",
            "ci",
            "--json",
        ])
        .output()
        .expect("run rainy");

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("VERIFY_FAILED"));
    assert!(stderr.contains("unknown-production-step"));
    assert!(stderr.contains("unknown verify step is not allowed in strict profile"));
}

#[test]
fn plan_file_apply_remove_upgrade_and_skill_sync() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "demo-saas"]);
    let app = temp.path().join("demo-saas");
    let app_path = app.to_string_lossy().to_string();
    let plan_path = app.join("plans/minio-plan.json");
    let plan = plan_path.to_string_lossy().to_string();

    run(&[
        "--workspace",
        &app_path,
        "add",
        "capability",
        "minio-file-storage",
        "--dry-run",
        "--output-plan",
        &plan,
    ]);
    assert!(plan_path.exists());
    run(&[
        "--workspace",
        &app_path,
        "apply",
        "--plan",
        &plan,
        "--dry-run",
    ]);
    assert!(
        !app.join("apps/frontend/src/components/file-upload/FileUpload.tsx")
            .exists()
    );

    run(&[
        "--workspace",
        &app_path,
        "apply",
        "--plan",
        &plan,
        "--apply",
    ]);
    assert!(
        app.join("apps/frontend/src/components/file-upload/FileUpload.tsx")
            .exists()
    );

    run(&[
        "--workspace",
        &app_path,
        "capability",
        "upgrade",
        "minio-file-storage",
        "--dry-run",
    ]);
    run_without_external_tools(&[
        "--workspace",
        &app_path,
        "verify",
        "--profile",
        "local",
        "--capability",
        "minio-file-storage",
    ]);

    run(&["--workspace", &app_path, "skill", "sync"]);
    assert!(app.join(".enterprise-agent/context.md").exists());
    assert!(app.join(".enterprise-agent/capabilities.md").exists());
    assert!(app.join(".enterprise-agent/commands.md").exists());

    run(&[
        "--workspace",
        &app_path,
        "capability",
        "remove",
        "minio-file-storage",
        "--dry-run",
    ]);
    assert!(
        app.join("apps/frontend/src/components/file-upload/FileUpload.tsx")
            .exists()
    );

    run(&[
        "--workspace",
        &app_path,
        "capability",
        "remove",
        "minio-file-storage",
        "--apply",
    ]);
    assert!(
        !app.join("apps/frontend/src/components/file-upload/FileUpload.tsx")
            .exists()
    );
    assert!(
        !fs::read_to_string(app.join("capability.lock"))
            .expect("lock")
            .contains("minio-file-storage:")
    );
}

#[test]
fn capability_dependencies_are_enforced() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "demo-saas"]);
    let app = temp.path().join("demo-saas");
    let app_path = app.to_string_lossy().to_string();

    let pack = app.join("dependency-packs/dependency-pack");
    write(
        &pack.join("pack.yaml"),
        r#"apiVersion: rainy.dev/v1
kind: CapabilityPack
metadata:
  name: dependency-pack
  version: 0.1.0
  owner: test
  description: Dependency pack
requires:
  rainy: ">=0.1.0"
exports:
  capabilities:
    - capabilities/base-capability.yaml
    - capabilities/dependent-capability.yaml
    - capabilities/missing-dependent.yaml
  validators: []
  skills: []
"#,
    );
    write(
        &pack.join("capabilities/base-capability.yaml"),
        r#"apiVersion: rainy.dev/v1
kind: Capability
id: base-capability
name: Base Capability
version: 0.1.0
description: Base capability.
dependsOn: []
providers: []
inputs: {}
actions:
  install: []
validations: []
doctor:
  checks: []
agentRules: []
"#,
    );
    write(
        &pack.join("capabilities/dependent-capability.yaml"),
        r#"apiVersion: rainy.dev/v1
kind: Capability
id: dependent-capability
name: Dependent Capability
version: 0.1.0
description: Depends on base capability.
dependsOn:
  - base-capability
providers: []
inputs: {}
actions:
  install: []
validations: []
doctor:
  checks: []
agentRules: []
"#,
    );
    write(
        &pack.join("capabilities/missing-dependent.yaml"),
        r#"apiVersion: rainy.dev/v1
kind: Capability
id: missing-dependent
name: Missing Dependent
version: 0.1.0
description: Depends on an unavailable capability.
dependsOn:
  - absent-capability
providers: []
inputs: {}
actions:
  install: []
validations: []
doctor:
  checks: []
agentRules: []
"#,
    );
    inject_registry_source(&app, "dependency-packs");

    let missing = rainy()
        .args([
            "--workspace",
            &app_path,
            "add",
            "capability",
            "missing-dependent",
            "--dry-run",
        ])
        .output()
        .expect("run rainy");
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("CAPABILITY_DEPENDENCY_MISSING"));

    run(&[
        "--workspace",
        &app_path,
        "add",
        "capability",
        "base-capability",
        "--apply",
    ]);
    run(&[
        "--workspace",
        &app_path,
        "add",
        "capability",
        "dependent-capability",
        "--apply",
    ]);

    let remove = rainy()
        .args([
            "--workspace",
            &app_path,
            "capability",
            "remove",
            "base-capability",
            "--apply",
        ])
        .output()
        .expect("run rainy");
    assert!(!remove.status.success());
    assert!(String::from_utf8_lossy(&remove.stderr).contains("CAPABILITY_DEPENDENT_INSTALLED"));
    assert!(
        fs::read_to_string(app.join("capability.lock"))
            .expect("lock")
            .contains("base-capability:")
    );
}

#[test]
fn provider_resolution_is_explicit_and_validated() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "demo-saas"]);
    let app = temp.path().join("demo-saas");
    let app_path = app.to_string_lossy().to_string();

    let pack = app.join("provider-packs/provider-pack");
    write(
        &pack.join("pack.yaml"),
        r#"apiVersion: rainy.dev/v1
kind: CapabilityPack
metadata:
  name: provider-pack
  version: 0.1.0
  owner: test
  description: Provider pack
requires:
  rainy: ">=0.1.0"
exports:
  capabilities:
    - capabilities/provider-default.yaml
    - capabilities/providerless.yaml
    - capabilities/provider-required.yaml
    - capabilities/provider-default-conflict.yaml
  validators: []
  skills: []
"#,
    );
    write(
        &pack.join("capabilities/provider-default.yaml"),
        r#"apiVersion: rainy.dev/v1
kind: Capability
id: provider-default
name: Provider Default
version: 0.1.0
description: Has a default provider.
dependsOn: []
providers:
  - id: minio
    default: true
  - id: s3
inputs: {}
actions:
  install: []
validations: []
doctor:
  checks: []
agentRules: []
"#,
    );
    write(
        &pack.join("capabilities/providerless.yaml"),
        r#"apiVersion: rainy.dev/v1
kind: Capability
id: providerless
name: Providerless
version: 0.1.0
description: Does not support providers.
dependsOn: []
providers: []
inputs: {}
actions:
  install: []
validations: []
doctor:
  checks: []
agentRules: []
"#,
    );
    write(
        &pack.join("capabilities/provider-required.yaml"),
        r#"apiVersion: rainy.dev/v1
kind: Capability
id: provider-required
name: Provider Required
version: 0.1.0
description: Requires an explicit provider.
dependsOn: []
providers:
  - id: minio
  - id: s3
inputs: {}
actions:
  install: []
validations: []
doctor:
  checks: []
agentRules: []
"#,
    );
    write(
        &pack.join("capabilities/provider-default-conflict.yaml"),
        r#"apiVersion: rainy.dev/v1
kind: Capability
id: provider-default-conflict
name: Provider Default Conflict
version: 0.1.0
description: Declares conflicting default providers.
dependsOn: []
providers:
  - id: minio
    default: true
  - id: s3
    default: true
inputs: {}
actions:
  install: []
validations: []
doctor:
  checks: []
agentRules: []
"#,
    );
    inject_registry_source(&app, "provider-packs");

    run(&[
        "--workspace",
        &app_path,
        "add",
        "capability",
        "provider-default",
        "--apply",
    ]);
    let lock: serde_yaml::Value =
        serde_yaml::from_str(&fs::read_to_string(app.join("capability.lock")).expect("lock"))
            .expect("parse lock");
    assert_eq!(
        lock["capabilities"]["provider-default"]["provider"].as_str(),
        Some("minio")
    );

    run(&[
        "--workspace",
        &app_path,
        "add",
        "capability",
        "provider-default",
        "--provider",
        "s3",
        "--apply",
    ]);
    let lock: serde_yaml::Value =
        serde_yaml::from_str(&fs::read_to_string(app.join("capability.lock")).expect("lock"))
            .expect("parse lock");
    assert_eq!(
        lock["capabilities"]["provider-default"]["provider"].as_str(),
        Some("s3")
    );

    let invalid = rainy()
        .args([
            "--workspace",
            &app_path,
            "add",
            "capability",
            "provider-default",
            "--provider",
            "gcs",
            "--dry-run",
        ])
        .output()
        .expect("run rainy");
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("CAPABILITY_PROVIDER_INVALID"));

    let unsupported = rainy()
        .args([
            "--workspace",
            &app_path,
            "add",
            "capability",
            "providerless",
            "--provider",
            "gcs",
            "--dry-run",
        ])
        .output()
        .expect("run rainy");
    assert!(!unsupported.status.success());
    assert!(
        String::from_utf8_lossy(&unsupported.stderr).contains("CAPABILITY_PROVIDER_UNSUPPORTED")
    );

    let required = rainy()
        .args([
            "--workspace",
            &app_path,
            "add",
            "capability",
            "provider-required",
            "--dry-run",
        ])
        .output()
        .expect("run rainy");
    assert!(!required.status.success());
    assert!(String::from_utf8_lossy(&required.stderr).contains("CAPABILITY_PROVIDER_REQUIRED"));

    let conflict = rainy()
        .args([
            "--workspace",
            &app_path,
            "add",
            "capability",
            "provider-default-conflict",
            "--dry-run",
        ])
        .output()
        .expect("run rainy");
    assert!(!conflict.status.success());
    assert!(
        String::from_utf8_lossy(&conflict.stderr).contains("CAPABILITY_PROVIDER_DEFAULT_CONFLICT")
    );
}

#[test]
fn unknown_template_variables_fail_planning() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "demo-saas"]);
    let app = temp.path().join("demo-saas");
    let app_path = app.to_string_lossy().to_string();

    let pack = app.join("bad-variable-packs/bad-variable");
    write(
        &pack.join("pack.yaml"),
        r#"apiVersion: rainy.dev/v1
kind: CapabilityPack
metadata:
  name: bad-variable
  version: 0.1.0
  owner: test
  description: Bad variable pack
requires:
  rainy: ">=0.1.0"
exports:
  capabilities:
    - capabilities/bad-variable.yaml
  validators: []
  skills: []
"#,
    );
    write(
        &pack.join("capabilities/bad-variable.yaml"),
        r#"apiVersion: rainy.dev/v1
kind: Capability
id: bad-variable
name: Bad Variable
version: 0.1.0
description: Uses an unknown template variable.
dependsOn: []
providers: []
inputs: {}
actions:
  install:
    - id: create-bad-file
      uses: file.create
      with:
        path: generated/{{ missing.value }}.txt
        content: should not render
validations: []
doctor:
  checks: []
agentRules: []
"#,
    );
    inject_registry_source(&app, "bad-variable-packs");

    let output = rainy()
        .args([
            "--workspace",
            &app_path,
            "add",
            "capability",
            "bad-variable",
            "--dry-run",
        ])
        .output()
        .expect("run rainy");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("VARIABLE_RENDER_FAILED"));
    assert!(!app.join("generated").exists());
}

#[test]
fn template_render_conflicts_require_force() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "demo-saas"]);
    let app = temp.path().join("demo-saas");
    let app_path = app.to_string_lossy().to_string();

    let pack = app.join("template-conflict-packs/template-conflict");
    write(
        &pack.join("pack.yaml"),
        r#"apiVersion: rainy.dev/v1
kind: CapabilityPack
metadata:
  name: template-conflict
  version: 0.1.0
  owner: test
  description: Template conflict pack
requires:
  rainy: ">=0.1.0"
exports:
  capabilities:
    - capabilities/template-conflict.yaml
  validators: []
  skills: []
"#,
    );
    write(
        &pack.join("capabilities/template-conflict.yaml"),
        r#"apiVersion: rainy.dev/v1
kind: Capability
id: template-conflict
name: Template Conflict
version: 0.1.0
description: Renders a conflicting template.
dependsOn: []
providers: []
inputs: {}
actions:
  install:
    - id: render-conflict
      uses: template.render
      with:
        template: templates/generated
        target: generated/conflict
validations: []
doctor:
  checks: []
agentRules: []
"#,
    );
    write(
        &pack.join("templates/generated/conflict.txt.hbs"),
        "from-template\n",
    );
    write(&app.join("generated/conflict/conflict.txt"), "manual\n");
    inject_registry_source(&app, "template-conflict-packs");

    let conflict = rainy()
        .args([
            "--workspace",
            &app_path,
            "add",
            "capability",
            "template-conflict",
            "--dry-run",
        ])
        .output()
        .expect("run rainy");
    assert!(!conflict.status.success());
    assert!(String::from_utf8_lossy(&conflict.stderr).contains("TEMPLATE_CONFLICT"));
    assert_eq!(
        fs::read_to_string(app.join("generated/conflict/conflict.txt")).expect("conflict file"),
        "manual\n"
    );

    run(&[
        "--workspace",
        &app_path,
        "add",
        "capability",
        "template-conflict",
        "--force",
        "--apply",
    ]);
    assert_eq!(
        fs::read_to_string(app.join("generated/conflict/conflict.txt")).expect("conflict file"),
        "from-template\n"
    );
}

#[test]
fn mutating_commands_default_to_dry_run_and_reject_conflicting_modes() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "demo-saas"]);
    let app = temp.path().join("demo-saas");
    let app_path = app.to_string_lossy().to_string();
    let plan_path = app.join("plans/minio-plan.json");
    let plan = plan_path.to_string_lossy().to_string();

    run(&[
        "--workspace",
        &app_path,
        "add",
        "capability",
        "minio-file-storage",
        "--output-plan",
        &plan,
    ]);
    assert!(plan_path.exists());
    assert!(
        !app.join("apps/frontend/src/components/file-upload/FileUpload.tsx")
            .exists()
    );

    run(&["--workspace", &app_path, "apply", "--plan", &plan]);
    assert!(
        !app.join("apps/frontend/src/components/file-upload/FileUpload.tsx")
            .exists()
    );

    let conflict = rainy()
        .args([
            "--workspace",
            &app_path,
            "apply",
            "--plan",
            &plan,
            "--dry-run",
            "--apply",
        ])
        .output()
        .expect("run rainy");
    assert!(!conflict.status.success());
    assert!(String::from_utf8_lossy(&conflict.stderr).contains("APPLY_MODE_CONFLICT"));

    let pack_conflict = rainy()
        .args([
            "--workspace",
            &app_path,
            "pack",
            "update",
            "--dry-run",
            "--apply",
        ])
        .output()
        .expect("run rainy");
    assert!(!pack_conflict.status.success());
    assert!(String::from_utf8_lossy(&pack_conflict.stderr).contains("APPLY_MODE_CONFLICT"));

    run(&[
        "--workspace",
        &app_path,
        "apply",
        "--plan",
        &plan,
        "--apply",
    ]);
    assert!(
        app.join("apps/frontend/src/components/file-upload/FileUpload.tsx")
            .exists()
    );
}

#[test]
fn apply_rolls_back_files_when_write_fails() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "demo-saas"]);
    let app = temp.path().join("demo-saas");
    let app_path = app.to_string_lossy().to_string();

    let pack = app.join("rollback-packs/rollback-pack");
    write(
        &pack.join("pack.yaml"),
        r#"apiVersion: rainy.dev/v1
kind: CapabilityPack
metadata:
  name: rollback-pack
  version: 0.1.0
  owner: test
  description: Rollback pack
requires:
  rainy: ">=0.1.0"
exports:
  capabilities:
    - capabilities/rollback-capability.yaml
  validators: []
  skills: []
"#,
    );
    write(
        &pack.join("capabilities/rollback-capability.yaml"),
        r#"apiVersion: rainy.dev/v1
kind: Capability
id: rollback-capability
name: Rollback Capability
version: 0.1.0
description: Fails during apply after one file write.
dependsOn: []
providers: []
inputs: {}
actions:
  install:
    - id: write-first
      uses: file.create
      with:
        path: generated/transaction/first.txt
        content: first
    - id: write-blocked-child
      uses: file.create
      with:
        path: blocked/child.txt
        content: child
validations: []
doctor:
  checks: []
agentRules: []
"#,
    );
    write(&app.join("blocked"), "blocking file\n");
    inject_registry_source(&app, "rollback-packs");

    let failed = rainy()
        .args([
            "--workspace",
            &app_path,
            "add",
            "capability",
            "rollback-capability",
            "--apply",
        ])
        .output()
        .expect("run rainy");
    assert!(!failed.status.success());
    assert!(!app.join("generated/transaction/first.txt").exists());
    assert_eq!(
        fs::read_to_string(app.join("blocked")).expect("blocking file"),
        "blocking file\n"
    );
    assert!(
        !fs::read_to_string(app.join("capability.lock"))
            .expect("lock")
            .contains("rollback-capability:")
    );
}

#[test]
fn pack_install_and_plugin_external_forwarding() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "demo-saas"]);
    let app = temp.path().join("demo-saas");
    let app_path = app.to_string_lossy().to_string();

    let pack = temp.path().join("custom-pack");
    write(
        &pack.join("pack.yaml"),
        r#"apiVersion: rainy.dev/v1
kind: CapabilityPack
metadata:
  name: custom-pack
  version: 0.1.0
  owner: test
  description: Custom pack
requires:
  rainy: ">=0.1.0"
exports:
  capabilities:
    - capabilities/custom-capability.yaml
  validators: []
  skills: []
"#,
    );
    write(
        &pack.join("capabilities/custom-capability.yaml"),
        r#"apiVersion: rainy.dev/v1
kind: Capability
id: custom-capability
name: Custom Capability
version: 0.1.0
description: Test capability installed from a local pack.
dependsOn: []
providers: []
inputs: {}
actions:
  install: []
validations: []
doctor:
  checks: []
agentRules: []
"#,
    );

    let pack_path = pack.to_string_lossy().to_string();
    run(&[
        "--workspace",
        &app_path,
        "pack",
        "install",
        &pack_path,
        "--apply",
    ]);
    let list = run(&["--workspace", &app_path, "capability", "list", "--json"]);
    assert!(String::from_utf8_lossy(&list.stdout).contains("custom-capability"));
    run(&["--workspace", &app_path, "pack", "update"]);

    let plugin_source = temp.path().join("plugins");
    write(
        &plugin_source.join("rainy-echo"),
        "#!/bin/sh\necho plugin:$*\n",
    );
    write(
        &plugin_source.join("plugin.json"),
        r#"{
  "protocolVersion": "rainy.plugin.v1",
  "name": "echo",
  "version": "0.1.0",
  "description": "Echo test plugin",
  "commands": [
    {
      "name": "echo",
      "description": "Echo arguments"
    }
  ],
  "actions": [],
  "permissions": {
    "fs": {
      "read": ["rainy.yaml"],
      "write": ["generated/**"]
    },
    "network": "none",
    "secrets": []
  }
}
"#,
    );
    let plugin_source = plugin_source.to_string_lossy().to_string();
    let conformance = run(&["conformance", "check", "--path", &plugin_source, "--json"]);
    assert!(String::from_utf8_lossy(&conformance.stdout).contains("plugin:echo:permissions"));
    run(&[
        "--workspace",
        &app_path,
        "plugin",
        "install",
        &plugin_source,
        "--apply",
    ]);
    let plugins = run(&["--workspace", &app_path, "plugin", "list", "--json"]);
    assert!(String::from_utf8_lossy(&plugins.stdout).contains("rainy-echo"));
    let inspect = run(&[
        "--workspace",
        &app_path,
        "plugin",
        "inspect",
        "echo",
        "--json",
    ]);
    assert!(String::from_utf8_lossy(&inspect.stdout).contains("Echo test plugin"));
    let forwarded = run(&["--workspace", &app_path, "echo", "hello", "world"]);
    assert!(String::from_utf8_lossy(&forwarded.stdout).contains("plugin:hello world"));
}

#[test]
fn plugin_install_rejects_builtin_command_shadowing() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "demo-saas"]);
    let app = temp.path().join("demo-saas");
    let app_path = app.to_string_lossy().to_string();

    let plugin_source = temp.path().join("shadow-plugin");
    write(
        &plugin_source.join("rainy-doctor"),
        "#!/bin/sh\necho shadow\n",
    );
    write(
        &plugin_source.join("plugin.json"),
        r#"{
  "protocolVersion": "rainy.plugin.v1",
  "name": "doctor",
  "version": "0.1.0",
  "description": "Invalid shadow plugin",
  "commands": [
    {
      "name": "doctor",
      "description": "Attempts to shadow doctor"
    }
  ],
  "actions": [],
  "permissions": {
    "fs": {
      "read": ["rainy.yaml"],
      "write": ["generated/**"]
    },
    "network": "none",
    "secrets": []
  }
}
"#,
    );
    let conformance = rainy()
        .args([
            "conformance",
            "check",
            "--path",
            &plugin_source.to_string_lossy(),
            "--json",
        ])
        .output()
        .expect("run rainy");
    assert!(!conformance.status.success());
    assert!(String::from_utf8_lossy(&conformance.stderr).contains("shadows a built-in"));

    let output = rainy()
        .args([
            "--workspace",
            &app_path,
            "plugin",
            "install",
            &plugin_source.to_string_lossy(),
            "--apply",
        ])
        .output()
        .expect("run rainy");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("PLUGIN_COMMAND_SHADOWS_BUILTIN"));
    assert!(!app.join(".rainy/plugins/bin/rainy-doctor").exists());
}

#[test]
fn plugin_install_respects_policy_gate() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "demo-saas"]);
    let app = temp.path().join("demo-saas");
    let app_path = app.to_string_lossy().to_string();

    write(
        &app.join(".rainy/org-policy.yaml"),
        "denyEdit:\n  - .rainy/plugins/**\n",
    );
    let plugin_source = temp.path().join("policy-plugin");
    write(
        &plugin_source.join("rainy-policy"),
        "#!/bin/sh\necho policy\n",
    );
    write(
        &plugin_source.join("plugin.json"),
        r#"{
  "protocolVersion": "rainy.plugin.v1",
  "name": "policy",
  "version": "0.1.0",
  "description": "Policy test plugin",
  "commands": [
    {
      "name": "policy",
      "description": "Policy command"
    }
  ],
  "actions": [],
  "permissions": {
    "fs": {
      "read": ["rainy.yaml"],
      "write": ["generated/**"]
    },
    "network": "none",
    "secrets": []
  }
}
"#,
    );

    let output = rainy()
        .args([
            "--workspace",
            &app_path,
            "plugin",
            "install",
            &plugin_source.to_string_lossy(),
            "--apply",
        ])
        .output()
        .expect("run rainy");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("POLICY_DENY_EDIT"));
    assert!(!app.join(".rainy/plugins/bin/rainy-policy").exists());
}

#[test]
fn plugin_list_warns_about_duplicate_plugin_names() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "demo-saas"]);
    let app = temp.path().join("demo-saas");
    let app_path = app.to_string_lossy().to_string();

    let plugin_source = temp.path().join("primary-plugin");
    write(
        &plugin_source.join("rainy-echo"),
        "#!/bin/sh\necho primary:$*\n",
    );
    write(
        &plugin_source.join("plugin.json"),
        r#"{
  "protocolVersion": "rainy.plugin.v1",
  "name": "echo",
  "version": "0.1.0",
  "description": "Primary echo plugin",
  "commands": [
    {
      "name": "echo",
      "description": "Echo arguments"
    }
  ],
  "actions": [],
  "permissions": {
    "fs": {
      "read": ["rainy.yaml"],
      "write": ["generated/**"]
    },
    "network": "none",
    "secrets": []
  }
}
"#,
    );
    run(&[
        "--workspace",
        &app_path,
        "plugin",
        "install",
        &plugin_source.to_string_lossy(),
        "--apply",
    ]);

    let path_plugin_dir = temp.path().join("path-plugins");
    write(
        &path_plugin_dir.join("rainy-echo"),
        "#!/bin/sh\necho duplicate:$*\n",
    );
    let original_path = std::env::var("PATH").expect("PATH");
    let path = format!("{}:{original_path}", path_plugin_dir.to_string_lossy());
    let plugins = rainy()
        .args(["--workspace", &app_path, "plugin", "list", "--json"])
        .env("PATH", path)
        .output()
        .expect("run rainy");

    assert!(plugins.status.success());
    let json: serde_json::Value =
        serde_json::from_slice(&plugins.stdout).expect("parse plugin list json");
    let echo = json["plugins"]
        .as_array()
        .expect("plugins array")
        .iter()
        .find(|plugin| plugin["name"] == "rainy-echo")
        .expect("rainy-echo plugin");
    assert!(
        echo["path"]
            .as_str()
            .expect("primary path")
            .contains(".rainy/plugins/bin/rainy-echo")
    );
    assert!(
        echo["shadowedPaths"]
            .as_array()
            .expect("shadowed paths")
            .iter()
            .any(|path| path
                .as_str()
                .expect("shadowed path")
                .contains("path-plugins/rainy-echo"))
    );
}

#[test]
fn community_pack_matrix_installs_extended_golden_path_capabilities() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "demo-saas"]);
    let app = temp.path().join("demo-saas");
    let app_path = app.to_string_lossy().to_string();

    for capability in [
        "postgres",
        "redis",
        "oidc-keycloak",
        "openapi-contract",
        "devcontainer",
        "opentelemetry",
        "helm-k8s-draft",
    ] {
        run(&[
            "--workspace",
            &app_path,
            "add",
            "capability",
            capability,
            "--apply",
        ]);
    }

    let app_yml = fs::read_to_string(app.join("apps/backend/src/main/resources/application.yml"))
        .expect("application.yml");
    assert!(app_yml.contains("jdbc:postgresql://localhost:5432/demo"));
    assert!(app_yml.contains("redis:"));
    assert!(app_yml.contains("issuer-uri: http://localhost:8081/realms/demo"));
    assert!(app_yml.contains("tracing:"));

    let compose = fs::read_to_string(app.join("compose.yaml")).expect("compose");
    assert!(compose.contains("postgres:"));
    assert!(compose.contains("redis:"));
    assert!(compose.contains("keycloak:"));

    assert!(app.join("openapi/openapi.yaml").exists());
    assert!(app.join(".devcontainer/devcontainer.json").exists());
    assert!(app.join("charts/demo-saas/Chart.yaml").exists());
    assert!(
        app.join("charts/demo-saas/templates/deployment.yaml")
            .exists()
    );

    run(&["--workspace", &app_path, "doctor"]);
    run_without_external_tools(&["--workspace", &app_path, "verify", "--profile", "local"]);
}

#[test]
fn extended_builtin_actions_and_conformance_are_exercised() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "demo-saas"]);
    let app = temp.path().join("demo-saas");
    let app_path = app.to_string_lossy().to_string();

    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root");
    let community_packs = repo_root
        .join("community-packs")
        .to_string_lossy()
        .to_string();
    let conformance = run(&["conformance", "check", "--path", &community_packs, "--json"]);
    assert!(String::from_utf8_lossy(&conformance.stdout).contains("rainy.conformance.v1"));

    let pack = temp.path().join("action-pack");
    write(
        &pack.join("pack.yaml"),
        r#"apiVersion: rainy.dev/v1
kind: CapabilityPack
metadata:
  name: action-pack
  version: 0.1.0
  owner: test
  description: Exercises extended built-in actions
requires:
  rainy: ">=0.1.0"
exports:
  capabilities:
    - capabilities/action-smoke.yaml
  validators: []
  skills: []
"#,
    );
    write(
        &pack.join("capabilities/action-smoke.yaml"),
        r#"apiVersion: rainy.dev/v1
kind: Capability
id: action-smoke
name: Action Smoke
version: 0.1.0
description: Exercises extended built-in actions.
dependsOn: []
providers: []
inputs: {}
actions:
  install:
    - id: add-bom
      uses: maven.addBom
      with:
        modulePath: apps/backend
        groupId: org.springframework.cloud
        artifactId: spring-cloud-dependencies
        version: "2023.0.3"
    - id: add-script
      uses: packageJson.addScript
      with:
        file: apps/frontend/package.json
        name: lint
        script: next lint
    - id: merge-json
      uses: json.merge
      with:
        file: generated/config.json
        patch:
          feature:
            enabled: true
    - id: merge-jsonc
      uses: devcontainer.merge
      with:
        file: .devcontainer/devcontainer.json
        patch:
          name: Rainy Dev
          customizations:
            vscode:
              extensions:
                - rust-lang.rust-analyzer
    - id: merge-toml
      uses: toml.merge
      with:
        file: generated/settings.toml
        patch:
          tool:
            rainy:
              enabled: true
    - id: create-file
      uses: file.create
      with:
        path: generated/hello.txt
        content: hello
    - id: append-agents
      uses: file.append
      with:
        path: AGENTS.md
        content: "- Action smoke capability installed."
    - id: render-chart
      uses: helm.renderChart
      with:
        template: templates/chart
        target: charts/action-smoke
validations: []
doctor:
  checks:
    - id: generated-file
      uses: file.exists
      with:
        path: generated/hello.txt
agentRules: []
"#,
    );
    write(
        &pack.join("templates/chart/Chart.yaml.hbs"),
        "apiVersion: v2\nname: action-smoke\nversion: 0.1.0\n",
    );

    let pack_path = pack.to_string_lossy().to_string();
    run(&[
        "--workspace",
        &app_path,
        "pack",
        "install",
        &pack_path,
        "--apply",
    ]);
    run(&[
        "--workspace",
        &app_path,
        "add",
        "capability",
        "action-smoke",
        "--apply",
    ]);
    run(&[
        "--workspace",
        &app_path,
        "doctor",
        "--capability",
        "action-smoke",
    ]);

    let pom = fs::read_to_string(app.join("apps/backend/pom.xml")).expect("pom");
    assert!(pom.contains("spring-cloud-dependencies"));
    let package = fs::read_to_string(app.join("apps/frontend/package.json")).expect("package");
    assert!(package.contains("\"lint\": \"next lint\""));
    assert!(app.join("generated/config.json").exists());
    assert!(app.join(".devcontainer/devcontainer.json").exists());
    assert!(app.join("generated/settings.toml").exists());
    assert!(app.join("generated/hello.txt").exists());
    assert!(app.join("charts/action-smoke/Chart.yaml").exists());
}

#[test]
fn http_registry_install_and_pack_signing_work() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "demo-saas"]);
    let app = temp.path().join("demo-saas");
    let app_path = app.to_string_lossy().to_string();

    let http_root = temp.path().join("http-registry");
    write(
        &http_root.join("packs/http-pack/pack.yaml"),
        r#"apiVersion: rainy.dev/v1
kind: CapabilityPack
metadata:
  name: http-pack
  version: 0.1.0
  owner: test
  description: HTTP registry pack
requires:
  rainy: ">=0.1.0"
exports:
  capabilities:
    - capabilities/http-capability.yaml
  validators: []
  skills: []
"#,
    );
    write(
        &http_root.join("packs/http-pack/capabilities/http-capability.yaml"),
        r#"apiVersion: rainy.dev/v1
kind: Capability
id: http-capability
name: HTTP Capability
version: 0.1.0
description: Capability loaded from HTTP registry.
dependsOn: []
providers: []
inputs: {}
actions:
  install: []
validations: []
doctor:
  checks: []
agentRules: []
"#,
    );
    let pack_digest = file_sha256(&http_root.join("packs/http-pack/pack.yaml"));
    let capability_digest =
        file_sha256(&http_root.join("packs/http-pack/capabilities/http-capability.yaml"));
    let base_url = serve_static(http_root.clone(), 6);
    write(
        &http_root.join("registry.yaml"),
        &format!(
            r#"protocolVersion: rainy.registry.v1
packs:
  - name: http-pack
    version: 0.1.0
    baseUrl: {base_url}/packs/http-pack
    files:
      - pack.yaml
      - capabilities/http-capability.yaml
    digests:
      pack.yaml: {pack_digest}
      capabilities/http-capability.yaml: {capability_digest}
"#
        ),
    );

    run(&[
        "--workspace",
        &app_path,
        "pack",
        "install",
        &format!("http+{base_url}/registry.yaml"),
        "--apply",
    ]);
    let list = run(&["--workspace", &app_path, "capability", "list", "--json"]);
    assert!(String::from_utf8_lossy(&list.stdout).contains("http-capability"));

    let cached_pack = app
        .join(".rainy/packs/http")
        .read_dir()
        .expect("http cache")
        .next()
        .expect("cache entry")
        .expect("cache entry")
        .path()
        .join("http-pack");
    let cached_pack_path = cached_pack.to_string_lossy().to_string();
    fs::write(
        http_root.join("packs/http-pack/capabilities/http-capability.yaml"),
        "tampered\n",
    )
    .expect("tamper remote pack");
    let output = rainy()
        .args(["--workspace", &app_path, "pack", "update", "--apply"])
        .output()
        .expect("update tampered registry");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("HTTP_REGISTRY_CHECKSUM_INVALID"));
    assert!(
        fs::read_to_string(cached_pack.join("capabilities/http-capability.yaml"))
            .expect("cached capability")
            .contains("id: http-capability")
    );

    run(&["pack", "sign", &cached_pack_path]);
    run(&["pack", "verify", &cached_pack_path]);
    fs::write(cached_pack.join("README.md"), "tampered\n").expect("tamper pack");
    let output = rainy()
        .args(["pack", "verify", &cached_pack_path])
        .output()
        .expect("run rainy");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("PACK_SIGNATURE_INVALID"));
}

#[test]
fn schema_validation_org_policy_and_http_plugin_adapter_work() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "demo-saas"]);
    let app = temp.path().join("demo-saas");
    let app_path = app.to_string_lossy().to_string();

    let schema_list = run(&["schema", "list", "--json"]);
    assert!(String::from_utf8_lossy(&schema_list.stdout).contains("rainy-project"));
    run(&[
        "schema",
        "validate",
        "--schema",
        "rainy-project",
        "--file",
        &app.join("rainy.yaml").to_string_lossy(),
    ]);
    let bad_config = temp.path().join("bad-rainy.yaml");
    write(
        &bad_config,
        "apiVersion: rainy.dev/v1\nkind: Project\nproject: {}\n",
    );
    let bad = rainy()
        .args([
            "schema",
            "validate",
            "--schema",
            "rainy-project",
            "--file",
            &bad_config.to_string_lossy(),
        ])
        .output()
        .expect("run rainy");
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("SCHEMA_VALIDATION_FAILED"));
    let bad_empty_name = temp.path().join("bad-empty-name.yaml");
    write(
        &bad_empty_name,
        r#"apiVersion: rainy.dev/v1
kind: Project
project:
  name: ""
paths:
  backend: apps/backend
  frontend: apps/frontend
package:
  java: com.example.demo
"#,
    );
    let bad_empty = rainy()
        .args([
            "schema",
            "validate",
            "--schema",
            "rainy-project",
            "--file",
            &bad_empty_name.to_string_lossy(),
        ])
        .output()
        .expect("run rainy");
    assert!(!bad_empty.status.success());
    assert!(String::from_utf8_lossy(&bad_empty.stderr).contains("SCHEMA_VALIDATION_FAILED"));

    let policy_pack = app.join("policy-packs/policy-pack");
    write(
        &policy_pack.join("pack.yaml"),
        r#"apiVersion: rainy.dev/v1
kind: CapabilityPack
metadata:
  name: policy-pack
  version: 0.1.0
  owner: test
  description: Writes generated file
requires:
  rainy: ">=0.1.0"
exports:
  capabilities:
    - capabilities/policy-capability.yaml
  validators: []
  skills: []
"#,
    );
    write(
        &policy_pack.join("capabilities/policy-capability.yaml"),
        r#"apiVersion: rainy.dev/v1
kind: Capability
id: policy-capability
name: Policy Capability
version: 0.1.0
description: Writes generated file.
dependsOn: []
providers: []
inputs: {}
actions:
  install:
    - id: create-generated
      uses: file.create
      with:
        path: generated/policy.txt
        content: denied
validations: []
doctor:
  checks: []
agentRules: []
"#,
    );
    inject_registry_source(&app, "policy-packs");
    write(
        &app.join(".rainy/org-policy.yaml"),
        "denyEdit:\n  - generated/**\n",
    );
    let denied = rainy()
        .args([
            "--workspace",
            &app_path,
            "add",
            "capability",
            "policy-capability",
            "--apply",
        ])
        .output()
        .expect("run rainy");
    assert!(!denied.status.success());
    assert!(String::from_utf8_lossy(&denied.stderr).contains("POLICY_DENY_EDIT"));
    fs::remove_file(app.join(".rainy/org-policy.yaml")).expect("remove org policy");

    let _adapter_guard = HTTP_PLUGIN_TEST_LOCK.lock().expect("plugin adapter lock");
    let adapter_url = serve_plugin_adapter_once("generated/rpc.txt", "rpc-ok");
    let plugin_source = temp.path().join("rpc-plugin");
    write(&plugin_source.join("rainy-rpc"), "#!/bin/sh\necho rpc\n");
    write(
        &plugin_source.join("plugin.json"),
        &format!(
            r#"{{
  "protocolVersion": "rainy.plugin.v1",
  "name": "rpc",
  "version": "0.1.0",
  "description": "RPC adapter plugin",
  "commands": [
    {{
      "name": "rpc",
      "description": "RPC shell command"
    }}
  ],
  "actions": [
    {{
      "id": "rpc.write",
      "description": "Write generated file"
    }}
  ],
  "permissions": {{
    "fs": {{
      "read": ["rainy.yaml"],
      "write": ["generated/**"]
    }},
    "network": "http",
    "secrets": []
  }},
  "adapter": {{
    "type": "http",
    "url": "{adapter_url}"
  }}
}}
"#
        ),
    );
    let plugin_source = plugin_source.to_string_lossy().to_string();
    run(&[
        "--workspace",
        &app_path,
        "plugin",
        "install",
        &plugin_source,
        "--apply",
    ]);
    run(&[
        "--workspace",
        &app_path,
        "plugin",
        "call",
        "rpc",
        "rpc.write",
        "--dry-run",
    ]);
    assert!(!app.join("generated/rpc.txt").exists());

    let adapter_url = serve_plugin_adapter_once("generated/rpc.txt", "rpc-ok");
    let manifest = app.join(".rainy/plugins/manifests/rpc.json");
    let content = fs::read_to_string(&manifest).expect("manifest");
    fs::write(
        &manifest,
        content.replace(&adapter_url_placeholder(&content), &adapter_url),
    )
    .expect("update manifest adapter");
    run(&[
        "--workspace",
        &app_path,
        "plugin",
        "call",
        "rpc",
        "rpc.write",
        "--apply",
    ]);
    assert_eq!(
        fs::read_to_string(app.join("generated/rpc.txt")).expect("rpc file"),
        "rpc-ok\n"
    );
}

#[test]
fn plugin_action_cannot_write_outside_manifest_permissions() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "demo-saas"]);
    let app = temp.path().join("demo-saas");
    let app_path = app.to_string_lossy().to_string();

    let _adapter_guard = HTTP_PLUGIN_TEST_LOCK.lock().expect("plugin adapter lock");
    let adapter_url = serve_plugin_adapter_once("apps/backend/pom.xml", "owned");
    let plugin_source = temp.path().join("limited-plugin");
    write(
        &plugin_source.join("rainy-limited"),
        "#!/bin/sh\necho limited\n",
    );
    write(
        &plugin_source.join("plugin.json"),
        &format!(
            r#"{{
  "protocolVersion": "rainy.plugin.v1",
  "name": "limited",
  "version": "0.1.0",
  "description": "Limited write plugin",
  "commands": [
    {{
      "name": "limited",
      "description": "Limited shell command"
    }}
  ],
  "actions": [
    {{
      "id": "limited.write",
      "description": "Attempt unauthorized write"
    }}
  ],
  "permissions": {{
    "fs": {{
      "read": ["rainy.yaml"],
      "write": ["generated/**"]
    }},
    "network": "http",
    "secrets": []
  }},
  "adapter": {{
    "type": "http",
    "url": "{adapter_url}"
  }}
}}
"#
        ),
    );
    run(&[
        "--workspace",
        &app_path,
        "plugin",
        "install",
        &plugin_source.to_string_lossy(),
        "--apply",
    ]);

    let output = rainy()
        .args([
            "--workspace",
            &app_path,
            "plugin",
            "call",
            "limited",
            "limited.write",
            "--dry-run",
        ])
        .output()
        .expect("run rainy");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("PLUGIN_FS_WRITE_DENIED"),
        "unexpected stderr: {stderr}"
    );
    assert!(
        !fs::read_to_string(app.join("apps/backend/pom.xml"))
            .expect("pom")
            .contains("owned")
    );
}

#[test]
fn wasm_plugin_action_returns_changeset_through_policy_apply() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "demo-saas"]);
    let app = temp.path().join("demo-saas");
    let app_path = app.to_string_lossy().to_string();

    let plugin_source = temp.path().join("wasm-plugin");
    write(&plugin_source.join("rainy-wasm"), "#!/bin/sh\necho wasm\n");
    let response = serde_json::json!({
        "protocolVersion": "rainy.plugin-rpc.v1",
        "changeSet": {
            "changes": [
                {
                    "kind": "create-file",
                    "path": "generated/wasm.txt",
                    "before": null,
                    "after": "wasm-ok\n",
                    "summary": "wasm write",
                    "noop": false
                }
            ]
        }
    });
    let response = serde_json::to_string(&response).expect("wasm response json");
    let offset = 16_u64;
    let packed = (offset << 32) | response.len() as u64;
    let wat_data = response
        .as_bytes()
        .iter()
        .map(|byte| format!("\\{byte:02x}"))
        .collect::<String>();
    let wat = format!(
        r#"(module
  (memory (export "memory") 1)
  (global $heap (mut i32) (i32.const 4096))
  (data (i32.const {offset}) "{wat_data}")
  (func (export "rainy_alloc") (param $len i32) (result i32)
    (local $ptr i32)
    global.get $heap
    local.set $ptr
    global.get $heap
    local.get $len
    i32.add
    global.set $heap
    local.get $ptr)
  (func (export "rainy_action") (param i32) (param i32) (result i64)
    i64.const {packed}))
"#
    );
    let wasm = wat::parse_str(wat).expect("compile wasm fixture");
    write_bytes(&plugin_source.join("write.wasm"), &wasm);
    write(
        &plugin_source.join("plugin.json"),
        r#"{
  "protocolVersion": "rainy.plugin.v1",
  "name": "wasm",
  "version": "0.1.0",
  "description": "Wasm action plugin",
  "commands": [
    {
      "name": "wasm",
      "description": "Wasm shell command"
    }
  ],
  "actions": [
    {
      "id": "wasm.write",
      "description": "Write generated file from Wasm",
      "runtime": "wasm",
      "wasm": "write.wasm"
    }
  ],
  "permissions": {
    "fs": {
      "read": ["rainy.yaml"],
      "write": ["generated/**"]
    },
    "network": "none",
    "secrets": []
  }
}
"#,
    );
    let plugin_source = plugin_source.to_string_lossy().to_string();
    run(&[
        "--workspace",
        &app_path,
        "plugin",
        "install",
        &plugin_source,
        "--apply",
    ]);
    assert!(app.join(".rainy/plugins/wasm/wasm/write.wasm").exists());

    run(&[
        "--workspace",
        &app_path,
        "plugin",
        "call",
        "wasm",
        "wasm.write",
        "--dry-run",
    ]);
    assert!(!app.join("generated/wasm.txt").exists());

    run(&[
        "--workspace",
        &app_path,
        "plugin",
        "call",
        "wasm",
        "wasm.write",
        "--apply",
    ]);
    assert_eq!(
        fs::read_to_string(app.join("generated/wasm.txt")).expect("wasm file"),
        "wasm-ok\n"
    );
}

#[test]
fn policy_blocks_denied_paths() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "demo-saas"]);
    let app = temp.path().join("demo-saas");
    let malicious = app.join("malicious-packs/malicious");
    write(
        &malicious.join("pack.yaml"),
        r#"apiVersion: rainy.dev/v1
kind: CapabilityPack
metadata:
  name: malicious
  version: 0.1.0
  owner: test
  description: Malicious pack
requires:
  rainy: ">=0.1.0"
exports:
  capabilities:
    - capabilities/malicious.yaml
  validators: []
  skills: []
"#,
    );
    write(
        &malicious.join("capabilities/malicious.yaml"),
        r#"apiVersion: rainy.dev/v1
kind: Capability
id: malicious
name: Malicious
version: 0.1.0
description: Attempts to edit a denied file.
dependsOn: []
providers: []
inputs: {}
actions:
  install:
    - id: render-prod-secret
      uses: template.render
      with:
        template: templates
        target: apps/backend/src/main/resources
validations: []
doctor:
  checks: []
agentRules: []
"#,
    );
    write(
        &malicious.join("templates/application-prod.yml.hbs"),
        "secret: should-not-write\n",
    );

    inject_registry_source(&app, "malicious-packs");

    let app_path = app.to_string_lossy().to_string();
    let output = rainy()
        .args([
            "--workspace",
            &app_path,
            "add",
            "capability",
            "malicious",
            "--apply",
        ])
        .output()
        .expect("run rainy");

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&output.stderr).contains("POLICY_DENY_EDIT"));
    assert!(
        !app.join("apps/backend/src/main/resources/application-prod.yml")
            .exists()
    );
}

#[test]
fn capability_policy_denies_pack_declared_paths() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "demo-saas"]);
    let app = temp.path().join("demo-saas");
    let pack = app.join("pack-policy-packs/policy-pack");
    write(
        &pack.join("pack.yaml"),
        r#"apiVersion: rainy.dev/v1
kind: CapabilityPack
metadata:
  name: pack-policy
  version: 0.1.0
  owner: test
  description: Capability policy pack
requires:
  rainy: ">=0.1.0"
exports:
  capabilities:
    - capabilities/pack-policy-capability.yaml
  validators: []
  skills: []
"#,
    );
    write(
        &pack.join("capabilities/pack-policy-capability.yaml"),
        r#"apiVersion: rainy.dev/v1
kind: Capability
id: pack-policy-capability
name: Pack Policy Capability
version: 0.1.0
description: Writes a path denied by its own capability policy.
dependsOn: []
providers: []
inputs: {}
policy:
  denyEdit:
    - generated/blocked/**
actions:
  install:
    - id: create-generated
      uses: file.create
      with:
        path: generated/blocked/file.txt
        content: blocked
validations: []
doctor:
  checks: []
agentRules: []
"#,
    );
    inject_registry_source(&app, "pack-policy-packs");

    let app_path = app.to_string_lossy().to_string();
    run(&[
        "--workspace",
        &app_path,
        "add",
        "capability",
        "pack-policy-capability",
        "--dry-run",
    ]);

    let output = rainy()
        .args([
            "--workspace",
            &app_path,
            "add",
            "capability",
            "pack-policy-capability",
            "--apply",
        ])
        .output()
        .expect("run rainy");

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&output.stderr).contains("POLICY_DENY_EDIT"));
    assert!(!app.join("generated/blocked/file.txt").exists());
}

#[test]
fn policy_requires_approval_for_gated_actions() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "demo-saas"]);
    let app = temp.path().join("demo-saas");
    let gated = app.join("gated-packs/gated");
    write(
        &gated.join("pack.yaml"),
        r#"apiVersion: rainy.dev/v1
kind: CapabilityPack
metadata:
  name: gated
  version: 0.1.0
  owner: test
  description: Gated operation pack
requires:
  rainy: ">=0.1.0"
exports:
  capabilities:
    - capabilities/gated-approval.yaml
  validators: []
  skills: []
"#,
    );
    write(
        &gated.join("capabilities/gated-approval.yaml"),
        r#"apiVersion: rainy.dev/v1
kind: Capability
id: gated-approval
name: Gated Approval
version: 0.1.0
description: Attempts an approval-gated operation.
dependsOn: []
providers: []
inputs: {}
actions:
  install:
    - id: k8s.apply
      uses: command.runValidation
      with:
        command: kubectl apply --dry-run=client -f generated/deployment.yaml
validations: []
doctor:
  checks: []
agentRules: []
"#,
    );
    inject_registry_source(&app, "gated-packs");

    let app_path = app.to_string_lossy().to_string();
    run(&[
        "--workspace",
        &app_path,
        "add",
        "capability",
        "gated-approval",
        "--dry-run",
    ]);

    let output = rainy()
        .args([
            "--workspace",
            &app_path,
            "add",
            "capability",
            "gated-approval",
            "--apply",
        ])
        .output()
        .expect("run rainy");

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&output.stderr).contains("POLICY_APPROVAL_REQUIRED"));
    assert!(
        !fs::read_to_string(app.join("capability.lock"))
            .expect("lock")
            .contains("gated-approval:")
    );
}

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent");
    }
    fs::write(path, content).expect("write file");
}

fn write_bytes(path: &Path, content: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent");
    }
    fs::write(path, content).expect("write bytes");
}

fn inject_registry_source(app: &Path, source: &str) {
    let rainy_yaml = app.join("rainy.yaml");
    let content = fs::read_to_string(&rainy_yaml).expect("rainy.yaml");
    let source_block = format!(
        "capabilityRegistry:\n  sources:\n    - type: local\n      path: \"{}\"",
        app.join(source).to_string_lossy()
    );
    let injected = if content.contains("capabilityRegistry:\n  sources: []") {
        content.replace("capabilityRegistry:\n  sources: []", &source_block)
    } else {
        content.replace(
            "capabilityRegistry:\n  sources:\n",
            &format!("{source_block}\n"),
        )
    };
    assert_ne!(content, injected, "registry source marker not found");
    fs::write(&rainy_yaml, injected).expect("write rainy.yaml");
}

fn count(haystack: &str, needle: &str) -> usize {
    haystack.match_indices(needle).count()
}

fn serve_static(root: std::path::PathBuf, expected_requests: usize) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test http server");
    let address = listener.local_addr().expect("local addr");
    thread::spawn(move || {
        for _ in 0..expected_requests {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buffer = [0_u8; 4096];
            let read = stream.read(&mut buffer).expect("read request");
            let request = String::from_utf8_lossy(&buffer[..read]);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/");
            let rel_path = path.trim_start_matches('/');
            let file = root.join(rel_path);
            if file.exists() {
                let body = fs::read(&file).expect("read static file");
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                stream.write_all(response.as_bytes()).expect("write head");
                stream.write_all(&body).expect("write body");
            } else {
                let body = b"not found";
                let response = format!(
                    "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                stream.write_all(response.as_bytes()).expect("write 404");
                stream.write_all(body).expect("write 404 body");
            }
        }
    });
    format!("http://{address}")
}

fn serve_plugin_adapter_once(path: &str, content: &str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind plugin adapter");
    let address = listener.local_addr().expect("local addr");
    let path = path.to_string();
    let content = content.to_string();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept adapter");
        let mut buffer = [0_u8; 4096];
        let _ = stream.read(&mut buffer).expect("read adapter request");
        let body = format!(
            r#"{{
  "protocolVersion": "rainy.plugin-rpc.v1",
  "changeSet": {{
    "changes": [
      {{
        "kind": "create-file",
        "path": "{path}",
        "before": null,
        "after": "{content}\n",
        "summary": "rpc write",
        "noop": false
      }}
    ]
  }}
}}"#
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n{}",
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .expect("write adapter response");
        stream.flush().expect("flush adapter response");
        // Keep the peer alive while ureq applies its response read timeout on macOS.
        thread::sleep(Duration::from_secs(1));
        let mut closed = [0_u8; 1];
        let _ = stream.read(&mut closed);
    });
    format!("http://{address}")
}

fn adapter_url_placeholder(content: &str) -> String {
    let marker = "\"url\": \"";
    let start = content.find(marker).expect("adapter url") + marker.len();
    let rest = &content[start..];
    let end = rest.find('"').expect("adapter url end");
    rest[..end].to_string()
}

fn file_sha256(path: &Path) -> String {
    format!(
        "{:x}",
        Sha256::digest(fs::read(path).expect("hash fixture"))
    )
}
