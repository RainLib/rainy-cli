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
    assert!(progress.contains("[1] Preparing capability"));
    assert!(progress.contains("[2] Running capability"));
    assert!(progress.contains("[4] Completed in"));

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
    let default_preview = run(&["--workspace", &workspace, "skill", "init", "--json"]);
    let default_preview = command_data(&default_preview);
    assert_eq!(default_preview["report"]["profile"], "comet");
    assert_eq!(
        default_preview["report"]["targets"],
        serde_json::json!(["codex", "universal"])
    );
    assert!(
        default_preview["report"]["changedFiles"]
            .as_array()
            .expect("planned paths")
            .iter()
            .any(|path| path == ".agents/skills/rainy-cli")
    );

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

    let automatic = rainy()
        .args([
            "--workspace",
            &workspace,
            "--progress",
            "always",
            "skill",
            "install",
        ])
        .output()
        .expect("run automatic Skill initialization preview");
    assert!(automatic.status.success());
    let automatic_progress =
        String::from_utf8(automatic.stderr).expect("automatic initialization progress");
    assert!(
        automatic_progress.contains("No Skill profile found; starting automatic initialization")
    );
    assert!(automatic_progress.contains("Building the Skill installation preview"));
    assert!(automatic_progress.contains("Completed in"));

    let failed = rainy()
        .args([
            "--workspace",
            &workspace,
            "--progress",
            "always",
            "skill",
            "install",
            "--skill",
            "missing",
        ])
        .output()
        .expect("run failed custom Skill install");
    assert!(!failed.status.success());
    let failed = String::from_utf8(failed.stderr).expect("structured error output");
    assert_eq!(failed.matches("SKILL_CUSTOM_NOT_FOUND").count(), 1);
    assert!(failed.contains("Failed in"));
    assert!(failed.contains("Next steps"));
    assert!(!failed.contains("config error:"));

    let help = String::from_utf8(run(&["--help"]).stdout).expect("top-level help");
    assert!(help.contains("--progress <MODE>"));
    assert!(help.contains("[possible values: auto, always, never]"));
}

#[test]
fn invalid_input_and_unknown_external_commands_use_the_error_contract() {
    let unknown = rainy()
        .args(["capabilty", "list"])
        .output()
        .expect("run unknown command");
    assert_eq!(unknown.status.code(), Some(2));
    assert!(unknown.stdout.is_empty());
    let unknown_error = String::from_utf8(unknown.stderr).expect("unknown command error");
    assert!(unknown_error.contains("CLI_ARGUMENT_INVALID"));
    assert!(unknown_error.contains("unrecognized subcommand 'capabilty'"));
    assert!(unknown_error.contains("'capability'"));
    assert!(unknown_error.contains("Next steps"));
    assert!(!unknown_error.contains("PLUGIN_NATIVE_NOT_TRUSTED"));

    let missing = rainy()
        .args(["capability", "add"])
        .output()
        .expect("run missing argument command");
    assert_eq!(missing.status.code(), Some(2));
    assert!(missing.stdout.is_empty());
    let missing_error = String::from_utf8(missing.stderr).expect("missing argument error");
    assert!(missing_error.contains("CLI_ARGUMENT_INVALID"));
    assert!(missing_error.contains("<CAPABILITY_ID>"));
    assert!(missing_error.contains("Usage"));
    assert!(
        missing_error.contains("rainy capability add <CAPABILITY_ID>"),
        "unexpected missing-argument error:\n{missing_error}"
    );
    assert!(missing_error.contains("Next steps"));
    assert!(!missing_error.contains("config error:"));

    let json = rainy()
        .args(["--json", "capability", "add"])
        .output()
        .expect("run JSON missing argument command");
    assert_eq!(json.status.code(), Some(2));
    assert!(json.stdout.is_empty());
    let json_error: serde_json::Value =
        serde_json::from_slice(&json.stderr).expect("structured JSON input error");
    assert_eq!(json_error["status"], "error");
    assert_eq!(json_error["error"]["code"], "CLI_ARGUMENT_INVALID");

    let temp = TempDir::new().expect("protocol schema fixtures");
    let success_path = temp.path().join("command-output.json");
    let success = run(&["--json", "capability", "list"]);
    fs::write(&success_path, &success.stdout).expect("write command output fixture");
    let validated = run(&[
        "schema",
        "validate",
        "--schema",
        "command-output",
        "--file",
        &success_path.to_string_lossy(),
        "--json",
    ]);
    assert_eq!(command_data(&validated)["report"]["status"], "passed");

    let error_path = temp.path().join("command-error.json");
    fs::write(&error_path, &json.stderr).expect("write command error fixture");
    let validated = run(&[
        "schema",
        "validate",
        "--schema",
        "command-error",
        "--file",
        &error_path.to_string_lossy(),
        "--json",
    ]);
    assert_eq!(command_data(&validated)["report"]["status"], "passed");
}

#[test]
fn credential_urls_are_rejected_without_leaking_into_output_or_audit() {
    let temp = TempDir::new().expect("credential URL workspace");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "security-demo", "--apply"]);
    let workspace = temp.path().join("security-demo");
    let secret = "rainy-secret-do-not-log";
    let source = format!("git+https://operator:{secret}@git.example.com/platform/packs.git");
    let output = rainy()
        .args([
            "--workspace",
            &workspace.to_string_lossy(),
            "registry",
            "add",
            "private",
            &source,
            "--apply",
            "--json",
        ])
        .output()
        .expect("reject credential URL");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let error: serde_json::Value =
        serde_json::from_slice(&output.stderr).expect("structured credential error");
    assert_eq!(error["error"]["code"], "PACK_SOURCE_UNSUPPORTED_URL");
    assert_eq!(error["error"]["category"], "registry");
    assert!(!String::from_utf8_lossy(&output.stderr).contains(secret));

    let audit = fs::read_to_string(workspace.join(".rainy/audit.log")).expect("failure audit");
    assert!(!audit.contains(secret));
    assert!(!audit.contains("operator:"));
    assert!(audit.contains("PACK_SOURCE_UNSUPPORTED_URL"));
}

#[test]
fn missing_project_configuration_points_skill_only_repositories_to_skill_doctor() {
    let temp = TempDir::new().expect("skill-only repository");
    let output = rainy()
        .args([
            "--workspace",
            &temp.path().to_string_lossy(),
            "doctor",
            "--scope",
            "project",
            "--json",
        ])
        .output()
        .expect("run project doctor without configuration");

    assert_eq!(output.status.code(), Some(4));
    assert!(output.stderr.is_empty());
    let report = command_data(&output);
    assert_eq!(report["report"]["status"], "failed");
    assert!(
        report["report"]["checks"]
            .as_array()
            .expect("doctor checks")
            .iter()
            .any(|check| {
                check["id"] == "project.config" && check["message"] == "rainy.yaml was not found"
            })
    );
}

#[test]
fn structured_verify_is_shell_free_and_definition_failures_are_reports() {
    let temp = TempDir::new().expect("verify workspace");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "verify-demo", "--apply"]);
    let app = temp.path().join("verify-demo");
    let pack = app.join("verify-packs/structured-verify");
    write(
        &pack.join("pack.yaml"),
        r#"apiVersion: rainy.dev/v1
kind: CapabilityPack
metadata:
  name: structured-verify
  version: 0.5.0
  owner: test
exports:
  capabilities:
    - capabilities/structured-verify.yaml
"#,
    );
    let capability_path = pack.join("capabilities/structured-verify.yaml");
    let capability = serde_json::json!({
        "apiVersion": "rainy.dev/v1",
        "kind": "Capability",
        "id": "structured-verify",
        "name": "Structured Verify",
        "version": "0.5.0",
        "description": "Cross-platform shell-free verification fixture.",
        "actions": {"install": []},
        "validations": [{
            "id": "rainy-version",
            "run": {
                "program": env!("CARGO_BIN_EXE_rainy"),
                "args": ["--version"]
            },
            "workingDirectory": ".",
            "timeoutSeconds": 30
        }]
    });
    write(
        &capability_path,
        &serde_yaml::to_string(&capability).expect("serialize capability"),
    );
    inject_registry_source(&app, "verify-packs");
    let app_path = app.to_string_lossy().to_string();
    run(&[
        "--workspace",
        &app_path,
        "capability",
        "add",
        "structured-verify",
        "--apply",
    ]);

    let output = run(&[
        "--workspace",
        &app_path,
        "verify",
        "--profile",
        "local",
        "--json",
    ]);
    let data = command_data(&output);
    let validation = data["report"]["steps"]
        .as_array()
        .expect("verify steps")
        .iter()
        .find(|step| step["id"] == "structured-verify:rainy-version")
        .expect("structured validation result");
    assert_eq!(validation["status"], "passed");
    assert_eq!(validation["timedOut"], serde_json::Value::Null);

    let unsafe_capability = serde_json::json!({
        "apiVersion": "rainy.dev/v1",
        "kind": "Capability",
        "id": "structured-verify",
        "name": "Structured Verify",
        "version": "0.5.0",
        "actions": {"install": []},
        "validations": [{
            "id": "legacy-shell",
            "command": "rainy --version && echo unsafe"
        }]
    });
    write(
        &capability_path,
        &serde_yaml::to_string(&unsafe_capability).expect("serialize unsafe capability"),
    );
    let failed = rainy()
        .args([
            "--workspace",
            &app_path,
            "verify",
            "--profile",
            "local",
            "--json",
        ])
        .output()
        .expect("run unsafe legacy validation");
    assert_eq!(failed.status.code(), Some(4));
    assert!(failed.stderr.is_empty());
    let data = command_data(&failed);
    assert_eq!(data["report"]["status"], "failed");
    assert!(
        data["report"]["steps"]
            .as_array()
            .expect("failed verify steps")
            .iter()
            .any(|step| step["message"]
                .as_str()
                .is_some_and(|message| message.contains("VERIFY_LEGACY_SHELL_UNSUPPORTED")))
    );
}

#[test]
fn help_describes_every_command_and_leaf_with_business_placeholders_and_examples() {
    for args in [Vec::new(), vec!["capability"], vec!["skill"]] {
        let output = rainy().args(&args).output().expect("run help fallback");
        assert!(output.status.success(), "help fallback returned an error");
        assert!(output.stderr.is_empty(), "help fallback wrote to stderr");
        let help = String::from_utf8(output.stdout).expect("help fallback output");
        assert!(help.contains("Usage:"), "help fallback omitted usage");
        assert!(
            help.contains("Commands:"),
            "help fallback omitted commands for {args:?}: {help}"
        );
    }

    let help = String::from_utf8(run(&["--help"]).stdout).expect("top-level help");
    assert!(help.contains("Arguments shown as <VALUE> are required values"));
    assert!(!help.contains("\n  init "));
    assert!(!help.contains("\n  add "));
    assert!(help.contains("self         Check, install, or skip Rainy CLI updates"));
    assert!(help.contains("completion   Generate shell completion scripts"));
    assert!(help.contains("--workspace <PROJECT_DIR>"));
    assert!(help.contains("QUICK START:"));

    let groups: &[&[&str]] = &[
        &["init"],
        &["add"],
        &["capability"],
        &["pack"],
        &["registry"],
        &["defaults"],
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
        &["capability", "add"],
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
        &["registry", "list"],
        &["registry", "add"],
        &["registry", "sync"],
        &["registry", "remove"],
        &["registry", "doctor"],
        &["defaults", "status"],
        &["defaults", "install"],
        &["defaults", "update"],
        &["defaults", "doctor"],
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
        &["completion"],
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
        String::from_utf8(run(&["capability", "add", "--help"]).stdout).expect("add help");
    assert!(capability_help.contains("<CAPABILITY_ID>"));
    assert!(capability_help.contains("--output-plan <PLAN_FILE>"));
    assert!(capability_help.contains("Options:"));
    assert!(capability_help.contains("Global Options:"));

    let self_help =
        String::from_utf8(run(&["self", "update", "--help"]).stdout).expect("self help");
    assert!(self_help.contains("--repo <OWNER/REPO>"));
    assert!(self_help.contains("--version <VERSION>"));

    let temp = TempDir::new().expect("completion workspace");
    fs::write(temp.path().join("rainy.yaml"), "apiVersion: rainy.dev/v1\n")
        .expect("completion workspace marker");
    let root = temp.path().to_string_lossy().to_string();
    let completion = run(&[
        "--workspace",
        &root,
        "--progress",
        "always",
        "completion",
        "zsh",
    ]);
    assert!(completion.status.success());
    assert!(completion.stderr.is_empty());
    assert!(String::from_utf8_lossy(&completion.stdout).starts_with("#compdef rainy\n"));
    assert!(!temp.path().join(".rainy/audit.log").exists());

    let completion = run(&["--json", "completion", "fish"]);
    let envelope = command_envelope(&completion);
    assert_eq!(envelope["type"], "completion");
    let completion = envelope["data"].clone();
    assert_eq!(completion["shell"], "fish");
    assert!(
        completion["script"]
            .as_str()
            .expect("completion script")
            .contains("complete -c rainy")
    );
}

#[test]
fn human_output_uses_stable_sections_and_structured_error_details() {
    let capabilities =
        String::from_utf8(run(&["capability", "list"]).stdout).expect("capability output");
    assert!(capabilities.starts_with("Capabilities\n\nSummary\n"));
    assert!(capabilities.contains("\nItems\n"));

    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    let preview =
        String::from_utf8(run(&["--workspace", &root, "new", "output-demo", "--dry-run"]).stdout)
            .expect("init preview");
    assert!(preview.starts_with("Project initialization\n\nSummary\n"));
    assert!(preview.contains("\nNext step\n"));
    assert!(preview.contains("\nPlanned locations\n"));

    let invalid_policy = temp.path().join("invalid-policy.yaml");
    fs::write(&invalid_policy, "unknownField: true\n").expect("invalid policy fixture");
    let failed = rainy()
        .args([
            "schema",
            "validate",
            "--schema",
            "org-policy",
            "--file",
            &invalid_policy.to_string_lossy(),
        ])
        .output()
        .expect("run invalid schema validation");
    assert!(!failed.status.success());
    assert_eq!(failed.status.code(), Some(4));
    assert!(failed.stderr.is_empty());
    let report = String::from_utf8(failed.stdout).expect("human validation report");
    assert!(report.contains("Schema validation"));
    assert!(report.contains("Status  Failed"));
    assert!(report.contains("Additional properties are not allowed"));
}

#[test]
fn skill_help_explains_the_workflow_and_each_subcommand() {
    let help = String::from_utf8(run(&["skill", "--help"]).stdout).expect("skill help");
    assert!(help.contains("Manage a project-scoped AI Skill profile"));
    assert!(help.contains("Interactive installs ask for final confirmation and apply immediately"));
    assert!(help.contains("Non-interactive and JSON callers still require --apply or --yes"));
    assert!(help.contains("rainy skill install --skill release-review --apply"));
    assert!(help.contains("Run 'rainy skill <COMMAND> --help'"));

    for command in [
        "init",
        "install",
        "create",
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
    assert!(init_help.contains("scripts default to"));
    assert!(init_help.contains("[default: zh]"));
    assert!(init_help.contains("multi-select"));

    let install_help =
        String::from_utf8(run(&["skill", "install", "--help"]).stdout).expect("install help");
    assert!(install_help.contains("--no-custom-skills"));
    assert!(install_help.contains("pass --dry-run to preview"));
    assert!(install_help.contains("Remove all installed project-owned Skills"));

    let registry_sync_help =
        String::from_utf8(run(&["registry", "sync", "--help"]).stdout).expect("registry sync help");
    assert!(registry_sync_help.contains("--skill <SKILL_ID>"));
    assert!(registry_sync_help.contains("--all-skills"));
    assert!(registry_sync_help.contains("Interactively select target hosts and exported Skills"));

    let pack_install_help =
        String::from_utf8(run(&["pack", "install", "--help"]).stdout).expect("pack install help");
    assert!(pack_install_help.contains("--skill <SKILL_ID>"));
    assert!(pack_install_help.contains("--all-skills"));
}

#[test]
fn skill_install_auto_initializes_and_manages_selected_project_skills() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "custom-skill-app", "--apply"]);
    let app = temp.path().join("custom-skill-app");
    let app_path = app.to_string_lossy().to_string();

    let preview = run(&[
        "--workspace",
        &app_path,
        "skill",
        "create",
        "release-review",
        "--description",
        "Review enterprise releases before delivery",
        "--json",
    ]);
    let preview = command_envelope(&preview);
    assert_eq!(preview["status"], "preview");
    assert!(!app.join("rainy-skills/release-review").exists());

    run(&[
        "--workspace",
        &app_path,
        "skill",
        "create",
        "release-review",
        "--description",
        "Review enterprise releases before delivery",
        "--apply",
        "--json",
    ]);
    for path in [
        "rainy-skills/release-review/SKILL.md",
        "rainy-skills/release-review/references/README.md",
        "rainy-skills/release-review/scripts/README.md",
    ] {
        assert!(app.join(path).is_file(), "missing {path}");
    }
    write(
        &app.join("rainy-skills/company-java/SKILL.md"),
        "---\nname: company-java\ndescription: Apply company Java service rules.\n---\n\n# Company Java\n",
    );
    write(
        &app.join("rainy-skills/company-java/scripts/check.sh"),
        "#!/bin/sh\nexit 0\n",
    );

    let automatic_preview = run(&[
        "--workspace",
        &app_path,
        "skill",
        "install",
        "--profile",
        "rainy",
        "--target",
        "codex",
        "--skill",
        "release-review",
        "--json",
    ]);
    let automatic_preview = command_data(&automatic_preview);
    assert_eq!(automatic_preview["report"]["operation"], "install");
    assert_eq!(automatic_preview["report"]["profile"], "rainy");
    assert_eq!(
        automatic_preview["report"]["customSkills"],
        serde_json::json!(["release-review"])
    );
    assert!(!app.join("rainy-skills.yaml").exists());

    run(&[
        "--workspace",
        &app_path,
        "skill",
        "install",
        "--profile",
        "rainy",
        "--target",
        "codex",
        "--skill",
        "release-review",
        "--apply",
        "--json",
    ]);
    assert!(app.join("rainy-skills.yaml").is_file());
    assert!(app.join("skills.lock").is_file());
    assert!(app.join(".agents/skills/release-review/SKILL.md").is_file());
    assert!(!app.join(".agents/skills/company-java").exists());
    let profile = fs::read_to_string(app.join("rainy-skills.yaml")).expect("Skill profile");
    assert!(profile.contains("customSkills:\n- release-review"));
    let lock = fs::read_to_string(app.join("skills.lock")).expect("Skill lock");
    assert!(lock.contains("name: release-review"));
    let doctor = run(&["--workspace", &app_path, "skill", "doctor", "--json"]);
    let doctor = command_data(&doctor);
    let check_ids = doctor["report"]["checks"]
        .as_array()
        .expect("doctor checks")
        .iter()
        .map(|check| check["id"].as_str().expect("check ID"))
        .collect::<Vec<_>>();
    assert_eq!(
        check_ids
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        check_ids.len(),
        "doctor emitted duplicate checks for a shared target directory"
    );

    write(
        &app.join(".agents/skills/release-review/local-change.md"),
        "local change\n",
    );
    let rejected = rainy()
        .args([
            "--workspace",
            &app_path,
            "skill",
            "install",
            "--skill",
            "company-java",
            "--apply",
            "--json",
        ])
        .output()
        .expect("reject deselecting modified custom Skill");
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("SKILL_MANAGED_FILES_MODIFIED"));

    run(&[
        "--workspace",
        &app_path,
        "skill",
        "install",
        "--skill",
        "company-java",
        "--force",
        "--apply",
        "--json",
    ]);
    assert!(!app.join(".agents/skills/release-review").exists());
    assert!(
        app.join(".agents/skills/company-java/scripts/check.sh")
            .is_file()
    );
    assert!(
        app.join("rainy-skills/release-review/SKILL.md").is_file(),
        "project-owned Skill source was removed"
    );

    run(&[
        "--workspace",
        &app_path,
        "skill",
        "install",
        "--no-custom-skills",
        "--apply",
        "--json",
    ]);
    assert!(!app.join(".agents/skills/company-java").exists());
    assert!(app.join("rainy-skills/company-java/SKILL.md").is_file());
    let profile = fs::read_to_string(app.join("rainy-skills.yaml")).expect("Skill profile");
    assert!(profile.contains("customSkills: []"));

    run(&[
        "--workspace",
        &app_path,
        "skill",
        "uninstall",
        "--apply",
        "--json",
    ]);
    assert!(!app.join(".agents/skills/company-java").exists());
    assert!(app.join("rainy-skills/company-java/SKILL.md").is_file());
}

#[test]
fn skill_commands_work_without_a_rainy_project_config() {
    let temp = TempDir::new().expect("tempdir");
    let workspace = temp.path().to_string_lossy().to_string();

    run(&[
        "--workspace",
        &workspace,
        "skill",
        "create",
        "release-review",
        "--description",
        "Review releases in a standard repository",
        "--apply",
        "--json",
    ]);
    assert!(!temp.path().join("rainy.yaml").exists());
    assert!(!temp.path().join("capability.lock").exists());

    let preview = run(&[
        "--workspace",
        &workspace,
        "skill",
        "install",
        "--profile",
        "rainy",
        "--target",
        "codex",
        "--skill",
        "release-review",
        "--json",
    ]);
    let preview = command_data(&preview);
    assert_eq!(preview["report"]["operation"], "install");
    assert!(
        preview["report"]["changedFiles"]
            .as_array()
            .expect("planned files")
            .iter()
            .any(|path| path == "AGENTS.md")
    );
    assert!(
        !preview["report"]["changedFiles"]
            .as_array()
            .expect("planned files")
            .iter()
            .any(|path| path == ".enterprise-agent/context.md")
    );

    run(&[
        "--workspace",
        &workspace,
        "skill",
        "install",
        "--profile",
        "rainy",
        "--target",
        "codex",
        "--skill",
        "release-review",
        "--apply",
        "--json",
    ]);
    for path in [
        "rainy-skills.yaml",
        "skills.lock",
        "AGENTS.md",
        ".agents/skills/rainy-cli/SKILL.md",
        ".agents/skills/release-review/SKILL.md",
    ] {
        assert!(temp.path().join(path).is_file(), "missing {path}");
    }
    assert!(!temp.path().join("rainy.yaml").exists());
    assert!(!temp.path().join("capability.lock").exists());
    assert!(!temp.path().join(".enterprise-agent").exists());
    let agents = fs::read_to_string(temp.path().join("AGENTS.md")).expect("standalone AGENTS");
    assert!(agents.contains("Project Skills: `release-review`"));

    run(&["--workspace", &workspace, "skill", "status", "--json"]);
    run(&["--workspace", &workspace, "skill", "doctor", "--json"]);
    let sync = run(&["--workspace", &workspace, "skill", "sync", "--json"]);
    let sync = command_data(&sync);
    assert_eq!(
        sync["report"]["changedFiles"],
        serde_json::json!(["AGENTS.md"])
    );
}

#[test]
fn agent_commands_work_without_a_rainy_project_config() {
    let temp = TempDir::new().expect("tempdir");
    let workspace = temp.path().to_string_lossy().to_string();

    let preview = run(&["--workspace", &workspace, "agent", "init", "--json"]);
    let preview = command_envelope(&preview);
    assert_eq!(preview["status"], "preview");
    assert_eq!(preview["data"]["message"], "Would refresh AGENTS.md");
    assert!(!temp.path().join("AGENTS.md").exists());

    run(&[
        "--workspace",
        &workspace,
        "agent",
        "init",
        "--apply",
        "--json",
    ]);
    let agents = fs::read_to_string(temp.path().join("AGENTS.md")).expect("AGENTS.md");
    assert!(agents.contains("rainy:context:start"));
    assert!(agents.contains("rainy skill install"));
    assert!(!temp.path().join(".enterprise-agent").exists());

    let context = run(&["--workspace", &workspace, "agent", "context", "--json"]);
    let context = command_data(&context);
    assert!(
        context["context"]
            .as_str()
            .expect("context")
            .contains("Project Rules")
    );
}

#[test]
fn managed_source_tracks_versions_updates_and_composes_new_projects() {
    let temp = TempDir::new().expect("tempdir");
    let source = temp.path().join("company-source");
    let rainy_home = temp.path().join("rainy-home");
    write_source_fixture(&source, "1.0.0", "module version 1");
    let workspace = temp.path().to_string_lossy().to_string();
    let source_path = source.to_string_lossy().to_string();
    let rainy_home_path = rainy_home.to_string_lossy().to_string();
    let envs = [("RAINY_HOME", rainy_home_path.as_str())];

    let inspect = run_with_env(
        &[
            "--workspace",
            &workspace,
            "source",
            "inspect",
            &source_path,
            "--json",
        ],
        &envs,
    );
    let inspect = command_data(&inspect);
    assert_eq!(inspect["report"]["operation"], "inspect");
    assert_eq!(inspect["report"]["sources"][0]["currentVersion"], "1.0.0");

    let preview = run_with_env(
        &[
            "--workspace",
            &workspace,
            "source",
            "add",
            "company",
            &source_path,
            "--json",
        ],
        &envs,
    );
    assert_eq!(command_envelope(&preview)["status"], "preview");
    assert!(!rainy_home.join("sources.yaml").exists());

    run_with_env(
        &[
            "--workspace",
            &workspace,
            "source",
            "add",
            "company",
            &source_path,
            "--apply",
            "--json",
        ],
        &envs,
    );
    assert!(rainy_home.join("sources.yaml").is_file());
    assert!(rainy_home.join("sources.lock").is_file());

    let resolved = run_with_env(
        &[
            "--workspace",
            &workspace,
            "source",
            "resolve",
            "company",
            "backend-a",
            "--json",
        ],
        &envs,
    );
    let resolved = command_data(&resolved);
    assert_eq!(
        resolved["report"]["sources"][0]["contents"][0]["type"],
        "workspace-module"
    );
    let resolved_path = resolved["report"]["sources"][0]["contents"][0]["resolvedPath"]
        .as_str()
        .expect("resolved content path");
    assert!(Path::new(resolved_path).join("module.txt.hbs").is_file());

    let template_catalog = run_with_env(
        &[
            "--workspace",
            &workspace,
            "source",
            "resolve",
            "company",
            "enterprise-project-templates",
            "--json",
        ],
        &envs,
    );
    let template_catalog = command_data(&template_catalog);
    assert_eq!(
        template_catalog["report"]["sources"][0]["contents"][0]["type"],
        "project-template-catalog"
    );
    let catalog_root = template_catalog["report"]["sources"][0]["contents"][0]["resolvedPath"]
        .as_str()
        .expect("resolved project template catalog path");
    assert!(
        Path::new(catalog_root)
            .join("project-templates.yaml")
            .is_file()
    );

    let repository_preview = run_with_env(
        &[
            "--workspace",
            &workspace,
            "new",
            "repository-preview",
            "--source",
            "company",
            "--git-url",
            "git@git.example.com:apps/repository-preview.git",
            "--json",
        ],
        &envs,
    );
    let repository_preview = command_envelope(&repository_preview);
    assert_eq!(repository_preview["type"], "source-project");
    assert_eq!(
        repository_preview["data"]["remote_url"],
        "git@git.example.com:apps/repository-preview.git"
    );

    let current = run_with_env(
        &[
            "--workspace",
            &workspace,
            "source",
            "check",
            "company",
            "--json",
        ],
        &envs,
    );
    let current = command_data(&current);
    assert_eq!(current["report"]["sources"][0]["state"], "current");
    assert_eq!(current["report"]["sources"][0]["updateAvailable"], false);

    let created = run_with_env(
        &[
            "--workspace",
            &workspace,
            "new",
            "source-project",
            "--source",
            "company",
            "--template",
            "service-base",
            "--module",
            "backend-a",
            "--package",
            "com.example.source",
            "--apply",
            "--json",
        ],
        &envs,
    );
    assert_eq!(command_envelope(&created)["type"], "source-project");
    let project = temp.path().join("source-project");
    assert!(project.join("rainy.yaml").is_file());
    assert!(project.join("services/backend-a/module.txt").is_file());
    assert!(project.join(".rainy/project-source.lock").is_file());
    assert!(
        fs::read_to_string(project.join("rainy.yaml"))
            .expect("project config")
            .contains("com.example.source")
    );

    let portable_home = temp.path().join("portable-rainy-home");
    let portable_home_path = portable_home.to_string_lossy().to_string();
    let portable_env = [("RAINY_HOME", portable_home_path.as_str())];
    let portable = run_with_env(
        &[
            "--workspace",
            &project.to_string_lossy(),
            "source",
            "check",
            "--project",
            "--json",
        ],
        &portable_env,
    );
    let portable = command_data(&portable);
    assert_eq!(portable["report"]["sources"][0]["state"], "current");
    run_with_env(
        &[
            "--workspace",
            &project.to_string_lossy(),
            "source",
            "update",
            "--project",
            "--apply",
            "--json",
        ],
        &portable_env,
    );
    assert!(portable_home.join("sources.yaml").is_file());
    assert!(portable_home.join("sources.lock").is_file());

    let nested = rainy()
        .current_dir(project.join("services/backend-a"))
        .env("RAINY_HOME", &rainy_home)
        .args(["source", "check", "--project", "--json"])
        .output()
        .expect("check project Source from a nested directory");
    assert!(nested.status.success());
    assert_eq!(
        command_data(&nested)["report"]["sources"][0]["state"],
        "current"
    );

    write_source_fixture(&source, "1.1.0", "module version 2");
    let changed = run_with_env(
        &[
            "--workspace",
            &workspace,
            "source",
            "check",
            "company",
            "--json",
        ],
        &envs,
    );
    let changed = command_data(&changed);
    assert_eq!(changed["report"]["sources"][0]["state"], "update-available");
    assert_eq!(changed["report"]["sources"][0]["latestVersion"], "1.1.0");

    let project_changed = run_with_env(
        &[
            "--workspace",
            &project.to_string_lossy(),
            "source",
            "check",
            "--project",
            "--json",
        ],
        &envs,
    );
    let project_changed = command_data(&project_changed);
    assert_eq!(
        project_changed["report"]["sources"][0]["state"],
        "update-available"
    );
    assert_eq!(
        project_changed["report"]["sources"][0]["currentVersion"],
        "1.0.0"
    );

    let updated = run_with_env(
        &[
            "--workspace",
            &workspace,
            "source",
            "update",
            "company",
            "--apply",
            "--json",
        ],
        &envs,
    );
    let updated = command_data(&updated);
    assert_eq!(updated["report"]["sources"][0]["currentVersion"], "1.1.0");
    assert_eq!(updated["report"]["sources"][0]["state"], "updated");

    let project_stale = run_with_env(
        &[
            "--workspace",
            &project.to_string_lossy(),
            "source",
            "check",
            "--project",
            "--json",
        ],
        &envs,
    );
    let project_stale = command_data(&project_stale);
    assert_eq!(
        project_stale["report"]["sources"][0]["state"],
        "project-update-available"
    );
    assert_eq!(
        project_stale["report"]["sources"][0]["currentVersion"],
        "1.0.0"
    );
    assert_eq!(
        project_stale["report"]["sources"][0]["latestVersion"],
        "1.1.0"
    );

    write(
        &rainy_home.join("sources.yaml"),
        r#"apiVersion: rainy.dev/v1
kind: RainySourceCatalog
sources:
  company:
    source: http://127.0.0.1:1/unreachable.zip
    channel: stable
"#,
    );
    let offline = run_with_env(
        &[
            "--workspace",
            &workspace,
            "source",
            "check",
            "company",
            "--json",
        ],
        &envs,
    );
    let offline = command_envelope(&offline);
    assert_eq!(offline["status"], "warning");
    assert_eq!(
        offline["data"]["report"]["sources"][0]["state"],
        "unreachable"
    );
    assert!(rainy_home.join("sources.lock").is_file());

    let provenance = run_with_env(
        &[
            "--workspace",
            &project.to_string_lossy(),
            "source",
            "check",
            "--project",
            "--json",
        ],
        &envs,
    );
    let provenance = command_data(&provenance);
    assert_eq!(
        provenance["report"]["sources"][0]["state"],
        "project-update-available"
    );
    assert_eq!(provenance["report"]["sources"][0]["latestVersion"], "1.1.0");
}

#[test]
fn source_index_installs_a_verified_zip_release() {
    let temp = TempDir::new().expect("tempdir");
    let source = temp.path().join("source-content");
    let server = temp.path().join("server");
    let rainy_home = temp.path().join("rainy-home");
    write_source_fixture(&source, "1.2.0", "zip module");
    fs::create_dir_all(&server).expect("server root");
    let archive = server.join("company-source-1.2.0.zip");
    create_zip_from_directory(&source, &archive, "company-source");
    let digest = file_sha256(&archive);
    let base_url = serve_static(server.clone(), 3);
    write(
        &server.join("rainy-source-index.yaml"),
        &format!(
            r#"apiVersion: rainy.dev/v1
kind: RainySourceIndex
metadata:
  name: company-source
releases:
  - version: 1.2.0
    url: company-source-1.2.0.zip
    sha256: {digest}
    channel: stable
x-company-mirror: true
"#,
        ),
    );
    let workspace = temp.path().to_string_lossy().to_string();
    let rainy_home_path = rainy_home.to_string_lossy().to_string();
    let index_url = format!("{base_url}/rainy-source-index.yaml");
    let envs = [("RAINY_HOME", rainy_home_path.as_str())];

    let installed = run_with_env(
        &[
            "--workspace",
            &workspace,
            "source",
            "add",
            "company-release",
            &index_url,
            "--channel",
            "stable",
            "--apply",
            "--json",
        ],
        &envs,
    );
    let installed = command_data(&installed);
    assert_eq!(installed["report"]["sources"][0]["sourceType"], "index");
    assert_eq!(installed["report"]["sources"][0]["currentVersion"], "1.2.0");

    let checked = run_with_env(
        &[
            "--workspace",
            &workspace,
            "source",
            "check",
            "company-release",
            "--json",
        ],
        &envs,
    );
    let checked = command_data(&checked);
    assert_eq!(checked["report"]["sources"][0]["state"], "current");
    assert_eq!(checked["report"]["sources"][0]["latestVersion"], "1.2.0");
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

fn command_envelope(output: &Output) -> serde_json::Value {
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("Rainy command JSON envelope");
    assert_eq!(value["protocolVersion"], "rainy.command.v1");
    assert!(value["type"].is_string(), "missing command result type");
    assert!(value["status"].is_string(), "missing command result status");
    assert!(value["data"].is_object(), "missing command result data");
    value
}

fn command_data(output: &Output) -> serde_json::Value {
    command_envelope(output)["data"].clone()
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
    let report = command_data(&output);
    assert_eq!(report["report"]["latestVersion"], "9.9.9");
    assert!(
        report["report"]["installCommand"]
            .as_str()
            .expect("install command")
            .contains(&release_base)
    );
}

#[test]
fn self_update_and_skip_preview_by_default_and_yes_applies() {
    let temp = TempDir::new().expect("self command state");
    let rainy_home = temp.path().join("rainy-home");

    let update = rainy()
        .args(["self", "update", "--version", "v9.9.9", "--json"])
        .env("RAINY_HOME", &rainy_home)
        .output()
        .expect("preview explicit update");
    assert!(update.status.success());
    let update = command_envelope(&update);
    assert_eq!(update["status"], "preview");
    assert_eq!(update["data"]["report"]["operation"], "update");
    assert_eq!(update["data"]["report"]["status"], "dry-run");
    assert_eq!(
        update["data"]["report"]["applyCommand"],
        "rainy self update --version v9.9.9 --apply"
    );
    let report_path = temp.path().join("update-report.json");
    fs::write(
        &report_path,
        serde_json::to_vec_pretty(&update["data"]["report"]).expect("serialize update report"),
    )
    .expect("write update report");
    let validated = run(&[
        "schema",
        "validate",
        "--schema",
        "update-report",
        "--file",
        &report_path.to_string_lossy(),
        "--json",
    ]);
    assert_eq!(command_data(&validated)["report"]["status"], "passed");
    assert!(!rainy_home.join("update-check.json").exists());

    let preview = rainy()
        .args(["self", "skip", "v9.9.9", "--json"])
        .env("RAINY_HOME", &rainy_home)
        .output()
        .expect("preview explicit skip");
    assert!(preview.status.success());
    let preview = command_envelope(&preview);
    assert_eq!(preview["status"], "preview");
    assert_eq!(preview["data"]["report"]["skipped"], false);
    assert!(!rainy_home.join("update-check.json").exists());

    let applied = rainy()
        .args(["self", "skip", "v9.9.9", "--yes", "--json"])
        .env("RAINY_HOME", &rainy_home)
        .output()
        .expect("apply explicit skip");
    assert!(applied.status.success());
    let applied = command_envelope(&applied);
    assert_eq!(applied["status"], "applied");
    assert_eq!(applied["data"]["report"]["status"], "applied");
    assert_eq!(applied["data"]["report"]["skipped"], true);
    let state = fs::read_to_string(rainy_home.join("update-check.json")).expect("update state");
    assert!(state.contains("9.9.9"));
}

#[test]
fn read_only_and_preview_commands_do_not_create_audit_or_managed_outputs() {
    let temp = TempDir::new().expect("preview workspace");
    let root = temp.path().to_string_lossy().to_string();
    let schemas = run(&["--workspace", &root, "schema", "list", "--json"]);
    assert_eq!(command_envelope(&schemas)["status"], "ok");
    assert!(!temp.path().join(".rainy").exists());

    run(&["--workspace", &root, "new", "preview-demo", "--apply"]);
    let workspace = temp.path().join("preview-demo");
    let workspace_arg = workspace.to_string_lossy().to_string();
    let agents_before = fs::read_to_string(workspace.join("AGENTS.md")).expect("initial AGENTS");

    let agent = run(&["--workspace", &workspace_arg, "agent", "init", "--json"]);
    assert_eq!(command_envelope(&agent)["status"], "preview");
    assert_eq!(
        fs::read_to_string(workspace.join("AGENTS.md")).expect("preview AGENTS"),
        agents_before
    );
    assert!(!workspace.join(".enterprise-agent").exists());

    let evidence = run(&[
        "--workspace",
        &workspace_arg,
        "evidence",
        "generate",
        "--json",
    ]);
    assert_eq!(command_envelope(&evidence)["status"], "preview");
    assert!(!workspace.join("evidence/report.md").exists());
    assert!(!workspace.join("evidence/report.json").exists());

    let sync = run(&["--workspace", &workspace_arg, "skill", "sync", "--json"]);
    assert_eq!(command_envelope(&sync)["status"], "preview");
    assert_eq!(
        fs::read_to_string(workspace.join("AGENTS.md")).expect("sync preview AGENTS"),
        agents_before
    );
    assert!(!workspace.join(".rainy/audit.log").exists());
}

#[test]
fn rainy_skill_profile_has_a_safe_project_lifecycle() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "skill-app", "--apply"]);
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
    let preview_envelope = command_envelope(&preview);
    assert_eq!(preview_envelope["type"], "skill");
    let preview_json = preview_envelope["data"].clone();
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
            "codex,universal",
            "--no-custom-skills",
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
    assert!(app.join(".agents/skills/rainy-cli/SKILL.md").is_file());
    assert!(!app.join(".agents/skills/rainy-comet").exists());

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
    let doctor_json = command_data(&doctor);
    assert_eq!(doctor_json["report"]["status"], "passed");

    let lock_path = app.join("skills.lock");
    let valid_lock = fs::read_to_string(&lock_path).expect("valid skills lock");
    let unsafe_lock = valid_lock.replacen(
        "path: .agents/skills/rainy-cli",
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
    assert!(rejected.stderr.is_empty());
    assert!(String::from_utf8_lossy(&rejected.stdout).contains("SKILL_LOCK_PATH_INVALID"));
    fs::write(&lock_path, valid_lock).expect("restore skills lock");

    let agents = app.join("AGENTS.md");
    let existing = fs::read_to_string(&agents).expect("AGENTS.md");
    fs::write(&agents, format!("{existing}\n<!-- user-content -->\n")).expect("extend AGENTS.md");
    run(&[
        "--workspace",
        &app_path,
        "skill",
        "sync",
        "--apply",
        "--json",
    ]);
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
    assert!(!app.join(".agents/skills/rainy-cli").exists());
}

#[test]
fn rainy_skill_profile_installs_supported_hosts_and_universal_target() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "skill-hosts-app", "--apply"]);
    let app = temp.path().join("skill-hosts-app");
    let app_path = app.to_string_lossy().to_string();

    run(&[
        "--workspace",
        &app_path,
        "skill",
        "init",
        "--profile",
        "rainy",
        "--target",
        "claude,cursor,codex",
        "--apply",
        "--json",
    ]);

    for path in [
        ".agents/skills/rainy-cli/SKILL.md",
        ".claude/skills/rainy-cli/SKILL.md",
        ".cursor/skills/rainy-cli/SKILL.md",
    ] {
        assert!(app.join(path).is_file(), "missing {path}");
    }
    let profile = fs::read_to_string(app.join("rainy-skills.yaml")).expect("skill profile");
    for target in ["claude", "codex", "cursor", "universal"] {
        assert!(
            profile.contains(&format!("- {target}")),
            "missing target {target}"
        );
    }

    run(&[
        "--workspace",
        &app_path,
        "skill",
        "uninstall",
        "--apply",
        "--json",
    ]);
    assert!(!app.join(".agents/skills/rainy-cli").exists());
    assert!(!app.join(".claude/skills/rainy-cli").exists());
    assert!(!app.join(".cursor/skills/rainy-cli").exists());
}

#[cfg(unix)]
#[test]
fn comet_skill_profile_uses_pinned_upstream_and_detects_drift() {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "comet-app", "--apply"]);
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
    for root in .codex .agent; do
      mkdir -p "$workspace/$root/skills/$name"
      printf '%s\n' '---' "name: $name" "description: test $name" '---' '' "# $name" > "$workspace/$root/skills/$name/SKILL.md"
    done
    ;;
  uninstall)
    rm -rf "$workspace/.agents/skills/comet" "$workspace/.agents/skills/comet-open" "$workspace/.codex/skills/openspec-propose" "$workspace/.agent/skills/openspec-propose"
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
    let fake_skills = temp.path().join("fake-skills");
    fs::write(
        &fake_skills,
        r##"#!/bin/sh
set -eu
for name in using-superpowers brainstorming custom-superpower; do
  rm -rf "$PWD/.agents/skills/$name"
  mkdir -p "$PWD/.agents/skills/$name"
  printf '%s\n' '---' "name: $name" "description: test $name" '---' '' "# $name" > "$PWD/.agents/skills/$name/SKILL.md"
done
printf '%s\n' '{"version":1,"skills":{"using-superpowers":{"source":"https://github.com/obra/superpowers/tree/v5.1.0/skills"},"brainstorming":{"source":"https://github.com/obra/superpowers/tree/v5.1.0/skills"},"custom-superpower":{"source":"https://github.com/obra/superpowers/tree/v5.1.0/skills"},"unrelated":{"source":"https://github.com/example/other"}}}' > "$PWD/skills-lock.json"
printf '%s\n' '{"status":"ok"}'
"##,
    )
    .expect("fake skills");
    let mut permissions = fs::metadata(&fake_skills).expect("metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_skills, permissions).expect("permissions");
    let fake_skills_path = fake_skills.to_string_lossy().to_string();
    fs::create_dir_all(app.join(".codex/rules")).expect("codex rules directory");
    fs::write(app.join(".codex/rules/user.rules"), "preserve\n").expect("codex user rules");
    fs::create_dir_all(app.join(".agent/workflows")).expect("agent workflows directory");
    fs::write(app.join(".agent/workflows/user.md"), "preserve\n").expect("agent user workflow");

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
    assert!(preview_text.contains("Preview only; no files changed"));
    assert!(preview_text.contains("Next step"));
    assert!(preview_text.contains("rainy skill init --profile comet"));
    assert!(preview_text.contains("Enabled Skills"));
    assert!(!preview_text.contains("npx --yes --package @rpamis/comet@0.4.0-beta.6"));

    let verbose_preview = run(&[
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
        "--verbose",
    ]);
    let verbose_preview =
        String::from_utf8(verbose_preview.stdout).expect("verbose Comet preview output");
    assert!(verbose_preview.contains("Upstream command"));
    assert!(verbose_preview.contains("npx --yes --package @rpamis/comet@0.4.0-beta.6"));

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
        &[
            ("RAINY_COMET_BIN", &fake_path),
            ("RAINY_SKILLS_BIN", &fake_skills_path),
        ],
    );
    for path in [
        ".agents/skills/rainy-cli/SKILL.md",
        ".agents/skills/rainy-comet/SKILL.md",
        ".agents/skills/comet/SKILL.md",
        ".agents/skills/comet-open/SKILL.md",
        ".agents/skills/openspec-propose/SKILL.md",
        ".agents/skills/using-superpowers/SKILL.md",
        ".agents/skills/brainstorming/SKILL.md",
        ".agents/skills/custom-superpower/SKILL.md",
        ".comet/config.yaml",
    ] {
        assert!(app.join(path).is_file(), "missing {path}");
    }
    assert!(!app.join(".codex/skills").exists());
    assert!(!app.join(".agent/skills").exists());
    assert!(app.join(".codex/rules/user.rules").is_file());
    assert!(app.join(".agent/workflows/user.md").is_file());
    let comet_config = fs::read_to_string(app.join(".comet/config.yaml")).expect("Comet config");
    assert!(comet_config.contains("auto_transition: false"));
    let lock = fs::read_to_string(app.join("skills.lock")).expect("skills lock");
    assert!(lock.contains("version: 0.4.0-beta.6"));
    assert!(lock.contains("name: openspec"));
    assert!(lock.contains(".agents/skills/comet"));
    assert!(lock.contains("name: superpowers"));
    assert!(lock.contains("managedBy: rainy"));
    assert!(lock.contains(".agents/skills/custom-superpower"));
    let profile = fs::read_to_string(app.join("rainy-skills.yaml")).expect("skills profile");
    assert!(profile.contains("skills: skills@1.5.20"));
    assert!(profile.contains("superpowers: obra/superpowers@5.1.0"));

    let doctor = run_with_env(
        &["--workspace", &app_path, "skill", "doctor", "--json"],
        &[
            ("RAINY_COMET_BIN", &fake_path),
            ("RAINY_SKILLS_BIN", &fake_skills_path),
        ],
    );
    let doctor = String::from_utf8_lossy(&doctor.stdout);
    assert!(doctor.contains("\"status\": \"passed\""));
    assert!(!doctor.contains("\"status\": \"warn\""));
    assert!(doctor.contains("superpowers skills are installed"));

    fs::write(
        app.join(".agents/skills/rainy-comet/local-edit.txt"),
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
        .env("RAINY_SKILLS_BIN", &fake_skills_path)
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
        &[
            ("RAINY_COMET_BIN", &fake_path),
            ("RAINY_SKILLS_BIN", &fake_skills_path),
        ],
    );
    let updated_lock = fs::read_to_string(app.join("skills.lock")).expect("updated lock");
    assert!(updated_lock.contains("version: 0.4.0-beta.7"));

    fs::write(
        app.join(".agents/skills/using-superpowers/local-edit.txt"),
        "modified\n",
    )
    .expect("modify upstream skill");
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
        .env("RAINY_SKILLS_BIN", &fake_skills_path)
        .output()
        .expect("run upstream-drifted update");
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("SKILL_UPSTREAM_FILES_MODIFIED"));

    run_with_env(
        &[
            "--workspace",
            &app_path,
            "skill",
            "uninstall",
            "--apply",
            "--force",
            "--json",
        ],
        &[
            ("RAINY_COMET_BIN", &fake_path),
            ("RAINY_SKILLS_BIN", &fake_skills_path),
        ],
    );
    assert!(!app.join(".agents/skills/rainy-comet").exists());
    assert!(!app.join(".agents/skills/comet").exists());
    assert!(!app.join(".agents/skills/using-superpowers").exists());
    assert!(!app.join(".agents/skills/brainstorming").exists());
    assert!(!app.join(".agents/skills/custom-superpower").exists());
    let upstream_lock = fs::read_to_string(app.join("skills-lock.json")).expect("upstream lock");
    assert!(upstream_lock.contains("unrelated"));
    assert!(!upstream_lock.contains("using-superpowers"));
    assert!(!app.join("rainy-skills.yaml").exists());
}

#[cfg(unix)]
#[test]
fn comet_skill_init_failure_is_retryable_without_force() {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "retry-app", "--apply"]);
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
    let fake_skills = temp.path().join("fake-skills-retry");
    fs::write(
        &fake_skills,
        r##"#!/bin/sh
set -eu
mkdir -p "$PWD/.agents/skills/using-superpowers"
printf '%s\n' '---' 'name: using-superpowers' 'description: test' '---' > "$PWD/.agents/skills/using-superpowers/SKILL.md"
printf '%s\n' '{"version":1,"skills":{"using-superpowers":{"source":"https://github.com/obra/superpowers/tree/v5.1.0/skills"}}}' > "$PWD/skills-lock.json"
printf '%s\n' '{"status":"ok"}'
"##,
    )
    .expect("fake skills");
    let mut permissions = fs::metadata(&fake_skills).expect("metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_skills, permissions).expect("permissions");
    let fake_skills_path = fake_skills.to_string_lossy().to_string();

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
        .env("RAINY_SKILLS_BIN", &fake_skills_path)
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
        &[
            ("RAINY_COMET_BIN", &fake_path),
            ("RAINY_SKILLS_BIN", &fake_skills_path),
        ],
    );
    assert!(app.join("rainy-skills.yaml").is_file());
    assert!(app.join("skills.lock").is_file());
}

#[cfg(unix)]
#[test]
fn comet_skill_init_rejects_superpowers_installer_failure() {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&[
        "--workspace",
        &root,
        "new",
        "superpowers-failure-app",
        "--apply",
    ]);
    let app = temp.path().join("superpowers-failure-app");
    let app_path = app.to_string_lossy().to_string();
    let comet_marker = temp.path().join("comet-ran");
    let fake_comet = temp.path().join("fake-comet-not-expected");
    fs::write(
        &fake_comet,
        format!("#!/bin/sh\ntouch '{}'\n", comet_marker.display()),
    )
    .expect("fake comet");
    let fake_skills = temp.path().join("fake-skills-failure");
    fs::write(
        &fake_skills,
        "#!/bin/sh\necho 'network unavailable' >&2\nexit 17\n",
    )
    .expect("fake skills failure");
    for path in [&fake_comet, &fake_skills] {
        let mut permissions = fs::metadata(path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).expect("permissions");
    }

    let failed = rainy()
        .args([
            "--workspace",
            &app_path,
            "skill",
            "init",
            "--apply",
            "--json",
        ])
        .env("RAINY_COMET_BIN", &fake_comet)
        .env("RAINY_SKILLS_BIN", &fake_skills)
        .output()
        .expect("run failed Superpowers install");
    assert!(!failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("SKILL_SUPERPOWERS_FAILED"));
    assert!(!comet_marker.exists());
    assert!(!app.join("rainy-skills.yaml").exists());
    assert!(!app.join("skills.lock").exists());
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
    run(&["--workspace", &root, "new", "demo-saas", "--apply"]);
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
        "--apply",
    ]);
    let app = temp.path().join("demo-saas");
    let app_path = app.to_string_lossy().to_string();
    let generated_ci = fs::read_to_string(app.join(".github/workflows/ci.yml")).expect("ci yml");
    assert!(generated_ci.contains("actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0"));
    assert!(!generated_ci.contains("actions/setup-java@"));
    assert!(generated_ci.contains("Install Java and Maven"));
    assert!(generated_ci.contains("pnpm install --frozen-lockfile"));
    assert!(app.join("apps/frontend/pnpm-lock.yaml").exists());
    assert!(generated_ci.contains("Install Rainy CLI"));
    assert!(generated_ci.contains("~/.rainy/bin/rainy verify --profile ci --json"));

    run(&[
        "--workspace",
        &app_path,
        "capability",
        "add",
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
        "capability",
        "add",
        "minio-file-storage",
        "--apply",
    ]);
    run(&["--workspace", &app_path, "doctor"]);
    let audit_log = app.join(".rainy/audit.log");
    assert!(audit_log.exists());
    let audit = fs::read_to_string(&audit_log).expect("audit log");
    assert!(audit.contains("\"protocolVersion\":\"rainy.audit.v1\""));
    assert!(audit.contains("\"command\":\"capability add\""));
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
    run_without_external_tools(&["--workspace", &app_path, "evidence", "generate", "--apply"]);

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
fn new_defaults_to_preview_and_does_not_create_project() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    let output = run(&[
        "--workspace",
        &root,
        "new",
        "demo-saas",
        "--golden-path",
        "spring-nextjs-saas",
        "--json",
    ]);

    assert!(!temp.path().join("demo-saas").exists());
    let envelope = command_envelope(&output);
    assert_eq!(envelope["type"], "init");
    assert_eq!(envelope["status"], "preview");
    let json = envelope["data"].clone();
    assert!(
        json["files"]
            .as_array()
            .expect("files array")
            .iter()
            .any(|file| file == "rainy.yaml")
    );

    write(
        &temp.path().join("project-templates.yaml"),
        "not: a valid catalog\n",
    );
    let non_interactive = run(&["--workspace", &root, "--json", "new", "automation-default"]);
    let envelope = command_envelope(&non_interactive);
    assert_eq!(envelope["type"], "init");
    assert_eq!(envelope["status"], "preview");
    assert!(!temp.path().join("automation-default").exists());
}

#[test]
fn new_from_enterprise_git_template_removes_source_git_and_prints_repository_setup() {
    let temp = TempDir::new().expect("tempdir");
    let source = temp.path().join("enterprise-template");
    write(
        &source.join("rainy.yaml.hbs"),
        r#"apiVersion: rainy.dev/v1
kind: Project
project:
  name: "{{ project.name }}"
  type: service
  owner: platform-engineering
paths:
  backend: apps/backend
  frontend: apps/frontend
  generated: generated
  evidence: evidence
package:
  java: "{{ package.java }}"
  npmScope: "@company"
capabilityRegistry:
  sources: []
policy:
  allowNativePlugins: false
  allowEdit:
    - capability.lock
    - generated/**
    - evidence/**
  denyEdit:
    - "**/.env*"
    - "**/secrets/**"
  requireApproval:
    - deploy.production
verify:
  profiles:
    local: [doctor]
    ci: [doctor, security-basic]
"#,
    );
    write(
        &source.join("capability.lock.hbs"),
        r#"lockfileVersion: 1
project:
  name: "{{ project.name }}"
rainy:
  version: 0.4.0
capabilities: {}
skills: []
"#,
    );
    write(
        &source.join("src/{{ project.name }}/package.txt.hbs"),
        "{{ package.java }}\n{{ packagePath }}\n",
    );
    write(&source.join("literal.txt"), "{{ not-a-rainy-variable }}\n");
    for args in [
        vec!["init", "--quiet"],
        vec!["config", "user.email", "rainy-test@example.com"],
        vec!["config", "user.name", "Rainy Test"],
        vec!["add", "."],
        vec!["commit", "--quiet", "-m", "enterprise template"],
    ] {
        let status = Command::new("git")
            .arg("-C")
            .arg(&source)
            .args(args)
            .status()
            .expect("run template git");
        assert!(status.success());
    }
    let status = Command::new("git")
        .arg("-C")
        .arg(&source)
        .args(["branch", "-M", "main"])
        .status()
        .expect("name template branch");
    assert!(status.success());
    let commit = Command::new("git")
        .arg("-C")
        .arg(&source)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("resolve template commit");
    let commit = String::from_utf8(commit.stdout)
        .expect("template commit UTF-8")
        .trim()
        .to_string();
    let catalog = temp.path().join("project-templates.yaml");
    write(
        &catalog,
        &format!(
            r#"apiVersion: rainy.dev/v1
kind: ProjectTemplateCatalog
templates:
  enterprise-java-service:
    description: Enterprise Java service
    source:
      type: git
      url: "file://{}"
      ref: main
    repository:
      defaultBranch: main
      remoteUrl: "git@git.example.com:apps/{{{{ project.name }}}}.git"
"#,
            source.display()
        ),
    );
    let root = temp.path().to_string_lossy().to_string();
    let catalog_path = catalog.to_string_lossy().to_string();
    let preview = run(&[
        "--workspace",
        &root,
        "new",
        "orders",
        "--template",
        "enterprise-java-service",
        "--template-config",
        &catalog_path,
        "--package",
        "com.company.orders",
        "--json",
    ]);
    let envelope = command_envelope(&preview);
    assert_eq!(envelope["type"], "project-template");
    assert_eq!(envelope["status"], "preview");
    let preview = envelope["data"].clone();
    assert_eq!(preview["requested_ref"], "main");
    assert_eq!(preview["source_git_removed"], false);
    assert_eq!(preview["remote_url"], "git@git.example.com:apps/orders.git");
    assert!(!temp.path().join("orders").exists());

    let output = rainy()
        .args([
            "--workspace",
            &root,
            "new",
            "orders",
            "--template",
            "enterprise-java-service",
            "--package",
            "com.company.orders",
            "--git-url",
            "git@git.example.com:teams/orders.git",
            "--apply",
        ])
        .env("RAINY_TEMPLATE_CONFIG", &catalog)
        .output()
        .expect("create project from template");
    assert!(
        output.status.success(),
        "template creation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("template output UTF-8");
    assert!(stdout.contains("Source Git metadata  Removed"));
    assert!(stdout.contains("git init -b main"));
    assert!(stdout.contains("git remote add origin git@git.example.com:teams/orders.git"));

    let project = temp.path().join("orders");
    assert!(!project.join(".git").exists());
    assert!(!project.join("rainy.yaml.hbs").exists());
    assert!(project.join("rainy.yaml").exists());
    assert!(project.join("capability.lock").exists());
    assert!(project.join(".rainy/project-template.lock").exists());
    assert_eq!(
        fs::read_to_string(project.join("src/orders/package.txt")).expect("rendered package"),
        "com.company.orders\ncom/company/orders\n"
    );
    assert_eq!(
        fs::read_to_string(project.join("literal.txt")).expect("literal template file"),
        "{{ not-a-rainy-variable }}\n"
    );
    let project_config = fs::read_to_string(project.join("rainy.yaml")).expect("rainy config");
    assert!(project_config.contains("name: \"orders\""));
    assert!(project_config.contains("java: \"com.company.orders\""));

    let status = run(&[
        "--workspace",
        &project.to_string_lossy(),
        "template",
        "status",
        "--json",
    ]);
    let status = command_envelope(&status);
    assert_eq!(status["type"], "template");
    assert_eq!(
        status["data"]["report"]["template"],
        "enterprise-java-service"
    );
    assert_eq!(status["data"]["report"]["resolvedRef"], commit);
    assert_eq!(
        status["data"]["report"]["updateAvailable"],
        serde_json::Value::Null
    );

    let check = run(&[
        "--workspace",
        &project.to_string_lossy(),
        "template",
        "check",
        "--json",
    ]);
    let check = command_envelope(&check);
    assert_eq!(check["status"], "ok");
    assert_eq!(check["data"]["report"]["updateAvailable"], false);
    assert_eq!(check["data"]["report"]["latestRef"], commit);

    write(
        &source.join("upstream-release.txt"),
        "new upstream release\n",
    );
    let status = Command::new("git")
        .arg("-C")
        .arg(&source)
        .args(["add", "."])
        .status()
        .expect("stage updated template");
    assert!(status.success());
    let status = Command::new("git")
        .arg("-C")
        .arg(&source)
        .args(["commit", "--quiet", "-m", "update enterprise template"])
        .status()
        .expect("commit updated template");
    assert!(status.success());
    let changed = run(&[
        "--workspace",
        &project.to_string_lossy(),
        "template",
        "check",
        "--json",
    ]);
    let changed = command_envelope(&changed);
    assert_eq!(changed["status"], "warning");
    assert_eq!(changed["data"]["report"]["status"], "update-available");
    assert_eq!(changed["data"]["report"]["updateAvailable"], true);

    let lock_path = project.join(".rainy/project-template.lock");
    let lock = fs::read_to_string(&lock_path).expect("template provenance lock");
    write(
        &lock_path,
        &lock.replace(
            &format!("file://{}", source.display()),
            "http://127.0.0.1:1/unreachable.git",
        ),
    );
    let unreachable = run(&[
        "--workspace",
        &project.to_string_lossy(),
        "template",
        "check",
        "--json",
    ]);
    let unreachable = command_envelope(&unreachable);
    assert_eq!(unreachable["status"], "warning");
    assert_eq!(unreachable["data"]["report"]["status"], "warning");
    assert_eq!(
        unreachable["data"]["report"]["updateAvailable"],
        serde_json::Value::Null
    );
}

#[test]
fn enterprise_template_selects_named_remote_and_applies_local_overlay() {
    let temp = TempDir::new().expect("tempdir");
    let initialize_remote = |path: &Path, marker: &str| {
        write(&path.join("download-method.txt"), marker);
        write(&path.join("upstream.txt"), "upstream template\n");
        write(
            &path.join("starter/src/main/resources/application.yml"),
            "---\nspring:\n  application:\n    name: test\n---\nserver:\n  port: 8080\n",
        );
        for args in [
            vec!["init", "--quiet"],
            vec!["config", "user.email", "rainy-test@example.com"],
            vec!["config", "user.name", "Rainy Test"],
            vec!["add", "."],
            vec!["commit", "--quiet", "-m", "template remote"],
            vec!["branch", "-M", "main"],
        ] {
            let status = Command::new("git")
                .arg("-C")
                .arg(path)
                .args(args)
                .status()
                .expect("initialize template remote");
            assert!(status.success());
        }
    };
    let ssh_source = temp.path().join("ssh-template");
    let http_source = temp.path().join("http-template");
    initialize_remote(&ssh_source, "ssh\n");
    initialize_remote(&http_source, "http\n");

    let overlay = temp.path().join("overlays/pkulaw");
    write(
        &overlay.join("rainy.yaml.hbs"),
        r#"apiVersion: rainy.dev/v1
kind: Project
project:
  name: "{{ project.name }}"
  type: service
  owner: platform-engineering
paths:
  backend: starter
  frontend: interfaces
  generated: generated
  evidence: evidence
package:
  java: "{{ package.java }}"
  npmScope: "@company"
capabilityRegistry:
  sources: []
policy:
  allowNativePlugins: false
  allowEdit: [rainy.yaml, capability.lock, generated/**, evidence/**]
  denyEdit: ["**/.env*", "**/secrets/**"]
  requireApproval: [deploy.production]
verify:
  profiles:
    local: [doctor]
    ci: [doctor, security-basic]
"#,
    );
    write(
        &overlay.join("capability.lock.hbs"),
        r#"lockfileVersion: 1
project:
  name: "{{ project.name }}"
rainy:
  version: 0.5.0
capabilities: {}
skills: []
"#,
    );
    write(
        &overlay.join("enterprise-overlay.txt.hbs"),
        "managed {{ project.name }}\n",
    );
    let catalog = temp.path().join("project-templates.yaml");
    write(
        &catalog,
        &format!(
            r#"apiVersion: rainy.dev/v1
kind: ProjectTemplateCatalog
templates:
  pkulaw-backend-mvc:
    description: Pkulaw backend service
    source:
      type: git
      ref: main
      defaultRemote: ssh
      remotes:
        ssh:
          description: SSH authentication
          url: "file://{}"
        http:
          description: HTTP authentication
          url: "file://{}"
    overlay: overlays/pkulaw
    textReplacements:
      - path: upstream.txt
        find: upstream template
        replace: "managed {{{{ project.name }}}}"
"#,
            ssh_source.display(),
            http_source.display()
        ),
    );

    let root = temp.path().join("projects");
    let root_string = root.to_string_lossy().to_string();
    let catalog_string = catalog.to_string_lossy().to_string();
    let output = run(&[
        "--workspace",
        &root_string,
        "--json",
        "new",
        "orders",
        "--template",
        "pkulaw-backend-mvc",
        "--template-config",
        &catalog_string,
        "--template-remote",
        "http",
        "--package",
        "com.pkulaw",
        "--apply",
    ]);
    let data = command_data(&output);
    assert_eq!(data["source_remote"], "http");
    assert_eq!(data["source"], format!("file://{}", http_source.display()));
    assert_eq!(data["source_git_removed"], true);

    let project = root.join("orders");
    assert!(!project.join(".git").exists());
    assert_eq!(
        fs::read_to_string(project.join("download-method.txt")).expect("selected remote marker"),
        "http\n"
    );
    assert_eq!(
        fs::read_to_string(project.join("enterprise-overlay.txt")).expect("overlay marker"),
        "managed orders\n"
    );
    assert_eq!(
        fs::read_to_string(project.join("upstream.txt")).expect("replacement target"),
        "managed orders\n"
    );
    assert!(project.join("rainy.yaml").is_file());
    assert!(project.join("capability.lock").is_file());
    let doctor = run(&[
        "--workspace",
        &project.to_string_lossy(),
        "--json",
        "doctor",
        "--scope",
        "auto",
    ]);
    assert_eq!(
        command_envelope(&doctor)["data"]["report"]["status"],
        "passed"
    );
}

#[test]
fn standalone_binary_downloads_defaults_and_keeps_schemas_embedded() {
    let temp = TempDir::new().expect("tempdir");
    let rainy_home = temp.path().join("rainy-home");
    let schema_cache = temp.path().join("schema-cache");
    let distribution = temp.path().join("defaults-repository");
    write(
        &distribution.join("rainy-defaults.yaml"),
        r#"apiVersion: rainy.dev/v1
kind: RainyDefaults
metadata:
  name: rainy-official
  version: 0.5.0
requires:
  rainy: ">=0.5.0, <0.6.0"
paths:
  packs: community-packs
  skills: integrations/skills
  templates: defaults/templates
"#,
    );
    copy_directory(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../community-packs"),
        &distribution.join("community-packs"),
    );
    copy_directory(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../integrations/skills"),
        &distribution.join("integrations/skills"),
    );
    copy_directory(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../defaults/templates"),
        &distribution.join("defaults/templates"),
    );
    for args in [
        vec!["init", "--quiet"],
        vec!["config", "user.email", "rainy-test@example.com"],
        vec!["config", "user.name", "Rainy Test"],
        vec!["add", "."],
        vec!["commit", "--quiet", "-m", "default distribution"],
    ] {
        let status = Command::new("git")
            .arg("-C")
            .arg(&distribution)
            .args(args)
            .status()
            .expect("run git");
        assert!(status.success());
    }
    let commit = Command::new("git")
        .arg("-C")
        .arg(&distribution)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("resolve defaults commit");
    let commit = String::from_utf8(commit.stdout)
        .expect("commit UTF-8")
        .trim()
        .to_string();
    let source = format!("file://{}", distribution.display());

    let status = rainy()
        .args(["defaults", "status", "--json"])
        .current_dir(temp.path())
        .env("RAINY_HOME", &rainy_home)
        .env("RAINY_FORCE_REMOTE_DEFAULTS", "1")
        .env("RAINY_DEFAULTS_SOURCE", &source)
        .env("RAINY_DEFAULTS_REF", &commit)
        .output()
        .expect("check defaults status");
    assert_eq!(command_data(&status)["report"]["status"], "missing");
    let offline = rainy()
        .args(["capability", "list", "--json"])
        .current_dir(temp.path())
        .env("RAINY_HOME", &rainy_home)
        .env("RAINY_FORCE_REMOTE_DEFAULTS", "1")
        .env("RAINY_DEFAULTS_SOURCE", &source)
        .env("RAINY_DEFAULTS_REF", &commit)
        .env("RAINY_OFFLINE", "1")
        .output()
        .expect("run offline defaults check");
    assert!(!offline.status.success());
    assert!(String::from_utf8_lossy(&offline.stderr).contains("DEFAULTS_OFFLINE_MISSING"));

    let install = rainy()
        .args(["defaults", "install", "--apply", "--json"])
        .current_dir(temp.path())
        .env("RAINY_HOME", &rainy_home)
        .env("RAINY_FORCE_REMOTE_DEFAULTS", "1")
        .env("RAINY_DEFAULTS_SOURCE", &source)
        .env("RAINY_DEFAULTS_REF", &commit)
        .output()
        .expect("install defaults");
    assert!(
        install.status.success(),
        "{}",
        String::from_utf8_lossy(&install.stderr)
    );
    for command in ["doctor", "update"] {
        let mut args = vec!["defaults", command, "--json"];
        if command == "update" {
            args.insert(2, "--apply");
        }
        let output = rainy()
            .args(args)
            .current_dir(temp.path())
            .env("RAINY_HOME", &rainy_home)
            .env("RAINY_FORCE_REMOTE_DEFAULTS", "1")
            .output()
            .expect("validate managed defaults");
        assert!(
            output.status.success(),
            "defaults {command} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    for args in [
        &["capability", "list", "--json"][..],
        &["schema", "list", "--json"][..],
    ] {
        let output = rainy()
            .args(args)
            .current_dir(temp.path())
            .env("RAINY_HOME", &rainy_home)
            .env("RAINY_FORCE_REMOTE_DEFAULTS", "1")
            .env("RAINY_FORCE_EMBEDDED_ASSETS", "1")
            .env("RAINY_ASSET_CACHE", &schema_cache)
            .output()
            .expect("run standalone command");
        assert!(
            output.status.success(),
            "standalone command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let output = rainy()
        .args(["new", "standalone-app", "--apply"])
        .current_dir(temp.path())
        .env("RAINY_FORCE_EMBEDDED_ASSETS", "1")
        .env("RAINY_ASSET_CACHE", &schema_cache)
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
    assert!(!config.contains(&rainy_home.to_string_lossy().to_string()));
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
        .env("RAINY_ASSET_CACHE", &schema_cache)
        .env("RAINY_HOME", &rainy_home)
        .env("RAINY_FORCE_REMOTE_DEFAULTS", "1")
        .output()
        .expect("install managed Rainy skill");
    assert!(
        output.status.success(),
        "managed skill install failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        temp.path()
            .join("standalone-app/.agents/skills/rainy-cli/SKILL.md")
            .is_file()
    );
    assert!(
        schema_cache
            .join(format!("rainy-cli-schemas-{}", env!("CARGO_PKG_VERSION")))
            .join(".complete")
            .is_file()
    );
    assert!(rainy_home.join("defaults.lock").is_file());
    let lock = fs::read_to_string(rainy_home.join("defaults.lock")).expect("defaults lock");
    assert!(lock.contains(&commit));
    assert!(lock.contains("packageVersion: 0.5.0"));
}

#[test]
fn local_defaults_are_snapshotted_into_rainy_home() {
    let temp = TempDir::new().expect("tempdir");
    let rainy_home = temp.path().join("rainy-home");
    let source = temp.path().join("local-defaults");
    write(
        &source.join("rainy-defaults.yaml"),
        r#"apiVersion: rainy.dev/v1
kind: RainyDefaults
metadata:
  name: rainy-local
  version: 0.5.0
requires:
  rainy: ">=0.5.0, <0.6.0"
paths:
  packs: packs
  skills: skills
  templates: templates
"#,
    );
    for directory in ["skills", "templates"] {
        fs::create_dir_all(source.join(directory)).expect("create defaults directory");
    }
    copy_directory(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../community-packs"),
        &source.join("packs"),
    );

    let install = rainy()
        .args([
            "defaults",
            "update",
            "--source",
            &source.to_string_lossy(),
            "--ref",
            "local-worktree",
            "--apply",
            "--json",
        ])
        .env("RAINY_HOME", &rainy_home)
        .env("RAINY_FORCE_REMOTE_DEFAULTS", "1")
        .output()
        .expect("install local defaults");
    assert!(
        install.status.success(),
        "local defaults install failed: {}",
        String::from_utf8_lossy(&install.stderr)
    );
    let report = command_data(&install)["report"].clone();
    assert_eq!(report["packageVersion"], "0.5.0");
    let cache = Path::new(report["cachePath"].as_str().expect("cache path"));
    assert!(cache.starts_with(rainy_home.join("defaults")));
    assert_ne!(cache, source);
    assert!(cache.join("packs/redis/pack.yaml").is_file());

    fs::remove_dir_all(&source).expect("remove local source");
    for args in [
        &["defaults", "doctor", "--json"][..],
        &["capability", "list", "--json"][..],
    ] {
        let output = rainy()
            .args(args)
            .env("RAINY_HOME", &rainy_home)
            .env("RAINY_FORCE_REMOTE_DEFAULTS", "1")
            .output()
            .expect("use local defaults snapshot");
        assert!(
            output.status.success(),
            "cached defaults command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn doctor_fails_when_installed_capability_artifact_is_missing() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "demo-saas", "--apply"]);
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
    assert_eq!(output.status.code(), Some(4));
    assert!(output.stderr.is_empty());
    let report = command_data(&output);
    assert_eq!(report["report"]["status"], "failed");
    assert!(
        report["report"]["checks"]
            .to_string()
            .contains("apps/frontend/src/components/file-upload")
    );
}

#[test]
fn verify_ci_profile_rejects_unknown_steps() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "demo-saas", "--apply"]);
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
    assert_eq!(output.status.code(), Some(4));
    assert!(output.stderr.is_empty());
    let report = command_data(&output);
    assert_eq!(report["report"]["status"], "failed");
    assert!(
        report["report"]["steps"]
            .to_string()
            .contains("unknown-production-step")
    );
    assert!(
        report["report"]["steps"]
            .to_string()
            .contains("unknown verify step is not allowed in strict profile")
    );
}

#[test]
fn plan_file_apply_remove_upgrade_and_skill_sync() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "demo-saas", "--apply"]);
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
    let legacy_plan_path = app.join("plans/minio-plan-legacy.json");
    let legacy_plan = legacy_plan_path.to_string_lossy().to_string();
    run(&[
        "--workspace",
        &app_path,
        "add",
        "capability",
        "minio-file-storage",
        "--dry-run",
        "--output-plan",
        &legacy_plan,
    ]);
    let canonical: serde_json::Value =
        serde_json::from_slice(&fs::read(&plan_path).expect("canonical plan"))
            .expect("canonical plan JSON");
    let legacy: serde_json::Value =
        serde_json::from_slice(&fs::read(&legacy_plan_path).expect("legacy plan"))
            .expect("legacy plan JSON");
    assert_eq!(canonical, legacy, "legacy add command changed behavior");
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

    run(&["--workspace", &app_path, "skill", "sync", "--apply"]);
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
    run(&["--workspace", &root, "new", "demo-saas", "--apply"]);
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
    run(&["--workspace", &root, "new", "demo-saas", "--apply"]);
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
    run(&["--workspace", &root, "new", "demo-saas", "--apply"]);
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
    run(&["--workspace", &root, "new", "demo-saas", "--apply"]);
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
    run(&["--workspace", &root, "new", "demo-saas", "--apply"]);
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
    run(&["--workspace", &root, "new", "demo-saas", "--apply"]);
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
    run(&["--workspace", &root, "new", "demo-saas", "--apply"]);
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
    run(&["--workspace", &root, "new", "demo-saas", "--apply"]);
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
    assert!(conformance.stderr.is_empty());
    assert!(String::from_utf8_lossy(&conformance.stdout).contains("shadows a built-in"));

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
    run(&["--workspace", &root, "new", "demo-saas", "--apply"]);
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
    run(&["--workspace", &root, "new", "demo-saas", "--apply"]);
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
    let json = command_data(&plugins);
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
    run(&["--workspace", &root, "new", "demo-saas", "--apply"]);
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
    run(&["--workspace", &root, "new", "demo-saas", "--apply"]);
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
    run(&["--workspace", &root, "new", "demo-saas", "--apply"]);
    let app = temp.path().join("demo-saas");
    let app_path = app.to_string_lossy().to_string();
    let rainy_home = temp.path().join("rainy-home");

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

    let source = format!("http+{base_url}/registry.yaml");
    let install = rainy()
        .args([
            "--workspace",
            &app_path,
            "pack",
            "install",
            &source,
            "--apply",
        ])
        .env("RAINY_HOME", &rainy_home)
        .output()
        .expect("install HTTP registry");
    assert!(
        install.status.success(),
        "{}",
        String::from_utf8_lossy(&install.stderr)
    );
    let list = rainy()
        .args(["--workspace", &app_path, "capability", "list", "--json"])
        .env("RAINY_HOME", &rainy_home)
        .output()
        .expect("list HTTP capability");
    assert!(String::from_utf8_lossy(&list.stdout).contains("http-capability"));

    assert!(!app.join(".rainy/packs").exists());
    let registry_cache = first_directory(&rainy_home.join("registries"));
    let cached_pack = first_directory(&registry_cache).join("http-pack");
    let cached_pack_path = cached_pack.to_string_lossy().to_string();
    fs::write(
        http_root.join("packs/http-pack/capabilities/http-capability.yaml"),
        "tampered\n",
    )
    .expect("tamper remote pack");
    let output = rainy()
        .args(["--workspace", &app_path, "pack", "update", "--apply"])
        .env("RAINY_HOME", &rainy_home)
        .output()
        .expect("update tampered registry");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("HTTP_REGISTRY_CHECKSUM_INVALID"));
    assert!(
        fs::read_to_string(cached_pack.join("capabilities/http-capability.yaml"))
            .expect("cached capability")
            .contains("id: http-capability")
    );

    run(&["pack", "sign", &cached_pack_path, "--apply"]);
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
fn registry_uses_global_cache_and_installs_only_selected_enterprise_skills() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "registry-demo", "--apply"]);
    let app = temp.path().join("registry-demo");
    let app_path = app.to_string_lossy().to_string();
    let rainy_home = temp.path().join("rainy-home");
    let source = temp.path().join("enterprise-registry");

    write(
        &source.join("platform/pack.yaml"),
        r#"apiVersion: rainy.dev/v1
kind: CapabilityPack
metadata:
  name: platform
  version: 1.0.0
  owner: enterprise
exports:
  capabilities: []
  validators: []
  skills:
    - skills/company-platform
    - skills/company-security
  plugins: []
"#,
    );
    write(
        &source.join("platform/skills/company-platform/SKILL.md"),
        "---\nname: company-platform\ndescription: Enterprise platform workflow.\n---\n\nUse the approved platform workflow.\n",
    );
    write(
        &source.join("platform/skills/company-security/SKILL.md"),
        "---\nname: company-security\ndescription: Enterprise security workflow.\n---\n\nUse the approved security workflow.\n",
    );
    write(
        &source.join("security/pack.yaml"),
        r#"apiVersion: rainy.dev/v1
kind: CapabilityPack
metadata:
  name: security
  version: 1.0.0
  owner: enterprise
exports:
  capabilities: []
  validators: []
  skills: []
  plugins: []
"#,
    );

    let source_path = source.to_string_lossy().to_string();
    let add = rainy()
        .args([
            "--workspace",
            &app_path,
            "registry",
            "add",
            "company",
            &source_path,
            "--apply",
        ])
        .env("RAINY_HOME", &rainy_home)
        .output()
        .expect("add registry");
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );

    let unknown = rainy()
        .args([
            "--workspace",
            &app_path,
            "registry",
            "sync",
            "company",
            "--module",
            "platform",
            "--install-skills",
            "--target",
            "cursor",
            "--skill",
            "missing-skill",
            "--apply",
        ])
        .env("RAINY_HOME", &rainy_home)
        .output()
        .expect("reject unknown registry Skill");
    assert!(!unknown.status.success());
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("REGISTRY_SKILL_NOT_FOUND"));
    assert!(!app.join(".cursor/skills").exists());

    let sync = rainy()
        .args([
            "--workspace",
            &app_path,
            "registry",
            "sync",
            "company",
            "--module",
            "platform",
            "--install-skills",
            "--target",
            "cursor",
            "--skill",
            "company-platform",
            "--apply",
        ])
        .env("RAINY_HOME", &rainy_home)
        .output()
        .expect("sync registry");
    assert!(
        sync.status.success(),
        "{}",
        String::from_utf8_lossy(&sync.stderr)
    );
    assert!(!app.join(".rainy/packs").exists());
    assert!(
        app.join(".cursor/skills/company-platform/SKILL.md")
            .is_file()
    );
    assert!(!app.join(".cursor/skills/company-security").exists());
    assert!(!app.join(".agents/skills/company-platform").exists());
    let sync_output = String::from_utf8(sync.stdout).expect("registry sync output");
    assert!(sync_output.contains("skills   company-platform (cursor)"));

    let cache_name = rainy_home.join("registries/company");
    let cache = first_directory(&cache_name);
    assert!(cache.join("platform/pack.yaml").is_file());
    assert!(!cache.join("security").exists());
    let lock = fs::read_to_string(app.join(".rainy/registry.lock")).expect("registry lock");
    assert!(lock.contains(&cache.to_string_lossy().to_string()));
    assert!(lock.contains("installedSkills:"));

    write(
        &app.join(".cursor/skills/company-platform/SKILL.md"),
        "locally modified\n",
    );
    let update = rainy()
        .args(["--workspace", &app_path, "pack", "update", "--apply"])
        .env("RAINY_HOME", &rainy_home)
        .output()
        .expect("update registry");
    assert!(!update.status.success());
    assert!(String::from_utf8_lossy(&update.stderr).contains("REGISTRY_SKILL_CONFLICT"));

    let switch = rainy()
        .args([
            "--workspace",
            &app_path,
            "registry",
            "sync",
            "company",
            "--module",
            "platform",
            "--install-skills",
            "--target",
            "cursor",
            "--skill",
            "company-security",
            "--apply",
        ])
        .env("RAINY_HOME", &rainy_home)
        .output()
        .expect("switch selected registry Skill");
    assert!(!switch.status.success());
    assert!(String::from_utf8_lossy(&switch.stderr).contains("before deselecting it"));

    let switch = rainy()
        .args([
            "--workspace",
            &app_path,
            "registry",
            "sync",
            "company",
            "--module",
            "platform",
            "--install-skills",
            "--target",
            "cursor",
            "--skill",
            "company-security",
            "--force",
            "--apply",
        ])
        .env("RAINY_HOME", &rainy_home)
        .output()
        .expect("force switch selected registry Skill");
    assert!(
        switch.status.success(),
        "{}",
        String::from_utf8_lossy(&switch.stderr)
    );
    assert!(!app.join(".cursor/skills/company-platform").exists());
    assert!(
        app.join(".cursor/skills/company-security/SKILL.md")
            .is_file()
    );

    let remove = rainy()
        .args([
            "--workspace",
            &app_path,
            "registry",
            "remove",
            "company",
            "--apply",
        ])
        .env("RAINY_HOME", &rainy_home)
        .output()
        .expect("remove registry");
    assert!(remove.status.success());
    assert!(cache.exists(), "shared registry cache was deleted");
}

#[cfg(unix)]
#[test]
fn registry_installs_selected_external_skill_with_pinned_skills_cli() {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&[
        "--workspace",
        &root,
        "new",
        "external-skill-demo",
        "--apply",
    ]);
    let app = temp.path().join("external-skill-demo");
    let app_path = app.to_string_lossy().to_string();
    let source = temp.path().join("enterprise-registry");
    let rainy_home = temp.path().join("rainy-home");
    let invocation = temp.path().join("skills-invocation.txt");
    let runner = temp.path().join("fake-skills");

    write(
        &source.join("build/pack.yaml"),
        "apiVersion: rainy.dev/v1\nkind: CapabilityPack\nmetadata:\n  name: build\n  version: 1.0.0\nexports:\n  capabilities: []\n  validators: []\n  skills: []\n  externalSkills:\n    - id: dependencies-gradle-common\n      source: https://git.example.com/build/dependencies-gradle-common\n      skillsPackage: skills@1.5.20\n      description: Shared Gradle build conventions\n  plugins: []\n",
    );
    write(
        &runner,
        "#!/bin/sh\nset -eu\nprintf '%s\\n' \"$@\" > \"$RAINY_TEST_SKILLS_INVOCATION\"\nmkdir -p \"$PWD/.agents/skills/dependencies-gradle-common\"\nprintf '%s\\n' '---' 'name: dependencies-gradle-common' 'description: test external skill' '---' > \"$PWD/.agents/skills/dependencies-gradle-common/SKILL.md\"\n",
    );
    let mut permissions = fs::metadata(&runner)
        .expect("runner metadata")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&runner, permissions).expect("make runner executable");

    let source_path = source.to_string_lossy().to_string();
    let add = rainy()
        .args([
            "--workspace",
            &app_path,
            "registry",
            "add",
            "company",
            &source_path,
            "--apply",
        ])
        .env("RAINY_HOME", &rainy_home)
        .output()
        .expect("add registry");
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );

    let sync = rainy()
        .args([
            "--workspace",
            &app_path,
            "registry",
            "sync",
            "company",
            "--module",
            "build",
            "--install-skills",
            "--target",
            "codex",
            "--skill",
            "dependencies-gradle-common",
            "--apply",
        ])
        .env("RAINY_HOME", &rainy_home)
        .env("RAINY_SKILLS_BIN", &runner)
        .env("RAINY_TEST_SKILLS_INVOCATION", &invocation)
        .output()
        .expect("install external enterprise Skill");
    assert!(
        sync.status.success(),
        "{}",
        String::from_utf8_lossy(&sync.stderr)
    );
    let args = fs::read_to_string(&invocation).expect("recorded skills arguments");
    assert!(args.contains("--yes\n--package\nskills@1.5.20\nskills\nadd"));
    assert!(args.contains("https://git.example.com/build/dependencies-gradle-common"));
    assert!(args.contains("--copy\n--agent\ncodex"));
    assert!(
        app.join(".agents/skills/dependencies-gradle-common/SKILL.md")
            .is_file()
    );
    let lock = fs::read_to_string(app.join(".rainy/registry.lock")).expect("registry lock");
    assert!(lock.contains("kind: external"));
    assert!(lock.contains("source: https://git.example.com/build/dependencies-gradle-common"));
    assert!(lock.contains("installer: skills@1.5.20"));
}

#[test]
fn archive_registry_verifies_sidecar_and_extracts_selected_modules() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "archive-demo", "--apply"]);
    let app = temp.path().join("archive-demo");
    let app_path = app.to_string_lossy().to_string();
    let rainy_home = temp.path().join("rainy-home");
    let source = temp.path().join("archive-source");
    for module in ["platform", "security"] {
        write(
            &source.join(module).join("pack.yaml"),
            &format!(
                "apiVersion: rainy.dev/v1\nkind: CapabilityPack\nmetadata:\n  name: {module}\n  version: 1.0.0\nexports:\n  capabilities: []\n  validators: []\n  skills: []\n  plugins: []\n"
            ),
        );
    }
    let server_root = temp.path().join("archive-server");
    fs::create_dir_all(&server_root).expect("archive server root");
    let archive_path = server_root.join("enterprise-packs.tar.gz");
    let archive_file = fs::File::create(&archive_path).expect("archive file");
    let encoder = flate2::write::GzEncoder::new(archive_file, flate2::Compression::default());
    let mut archive = tar::Builder::new(encoder);
    archive
        .append_dir_all("enterprise-packs", &source)
        .expect("append archive tree");
    archive.finish().expect("finish archive");
    drop(archive);
    write(
        &server_root.join("enterprise-packs.tar.gz.sha256"),
        &format!("{}  enterprise-packs.tar.gz\n", file_sha256(&archive_path)),
    );
    let base_url = serve_static(server_root, 2);
    let source_url = format!("{base_url}/enterprise-packs.tar.gz");

    let add = rainy()
        .args([
            "--workspace",
            &app_path,
            "registry",
            "add",
            "releases",
            &source_url,
            "--apply",
        ])
        .env("RAINY_HOME", &rainy_home)
        .output()
        .expect("add archive registry");
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );
    let sync = rainy()
        .args([
            "--workspace",
            &app_path,
            "registry",
            "sync",
            "releases",
            "--module",
            "security",
            "--apply",
        ])
        .env("RAINY_HOME", &rainy_home)
        .output()
        .expect("sync archive registry");
    assert!(
        sync.status.success(),
        "{}",
        String::from_utf8_lossy(&sync.stderr)
    );

    let cache = first_directory(&rainy_home.join("registries/releases"));
    assert!(cache.join("security/pack.yaml").is_file());
    assert!(!cache.join("platform").exists());
    assert!(!app.join(".rainy/packs").exists());
    let lock = fs::read_to_string(app.join(".rainy/registry.lock")).expect("registry lock");
    assert!(lock.contains("sha256:"));
    assert!(lock.contains("security"));
}

#[test]
fn git_registry_locks_exact_commit_and_selected_module() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "git-demo", "--apply"]);
    let app = temp.path().join("git-demo");
    let app_path = app.to_string_lossy().to_string();
    let rainy_home = temp.path().join("rainy-home");
    let repository = temp.path().join("enterprise-git");
    for module in ["platform", "observability"] {
        write(
            &repository.join(module).join("pack.yaml"),
            &format!(
                "apiVersion: rainy.dev/v1\nkind: CapabilityPack\nmetadata:\n  name: {module}\n  version: 1.0.0\nexports:\n  capabilities: []\n  validators: []\n  skills: []\n  plugins: []\n"
            ),
        );
    }
    for args in [
        vec!["init", "--quiet"],
        vec!["config", "user.email", "rainy-test@example.com"],
        vec!["config", "user.name", "Rainy Test"],
        vec!["add", "."],
        vec!["commit", "--quiet", "-m", "initial registry"],
    ] {
        let status = Command::new("git")
            .arg("-C")
            .arg(&repository)
            .args(args)
            .status()
            .expect("run git");
        assert!(status.success());
    }
    let commit = Command::new("git")
        .arg("-C")
        .arg(&repository)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("resolve commit");
    let commit = String::from_utf8(commit.stdout)
        .expect("commit UTF-8")
        .trim()
        .to_string();
    let source = format!("git+file://{}", repository.display());

    let add = rainy()
        .args([
            "--workspace",
            &app_path,
            "registry",
            "add",
            "gitlab",
            &source,
            "--ref",
            &commit,
            "--apply",
        ])
        .env("RAINY_HOME", &rainy_home)
        .output()
        .expect("add Git registry");
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );
    let sync = rainy()
        .args([
            "--workspace",
            &app_path,
            "registry",
            "sync",
            "gitlab",
            "--module",
            "observability",
            "--apply",
        ])
        .env("RAINY_HOME", &rainy_home)
        .output()
        .expect("sync Git registry");
    assert!(
        sync.status.success(),
        "{}",
        String::from_utf8_lossy(&sync.stderr)
    );
    let lock = fs::read_to_string(app.join(".rainy/registry.lock")).expect("registry lock");
    assert!(lock.contains(&commit));
    let cache = first_directory(&rainy_home.join("registries/gitlab"));
    assert!(cache.join("observability/pack.yaml").is_file());
    assert!(!cache.join("platform").exists());
    assert!(!app.join(".rainy/packs").exists());
}

#[test]
fn schema_validation_org_policy_and_http_plugin_adapter_work() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "demo-saas", "--apply"]);
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
    assert!(bad.stderr.is_empty());
    assert!(String::from_utf8_lossy(&bad.stdout).contains("Status  Failed"));
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
    assert!(bad_empty.stderr.is_empty());
    assert!(String::from_utf8_lossy(&bad_empty.stdout).contains("Status  Failed"));

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
    run(&["--workspace", &root, "new", "demo-saas", "--apply"]);
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
    run(&["--workspace", &root, "new", "demo-saas", "--apply"]);
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
fn wasm_plugin_limits_fuel_memory_input_and_output() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "demo-saas", "--apply"]);
    let app = temp.path().join("demo-saas");
    let app_path = app.to_string_lossy().to_string();
    let plugin_source = temp.path().join("limited-wasm-plugin");

    write(
        &plugin_source.join("rainy-limited-wasm"),
        "#!/bin/sh\necho limited-wasm\n",
    );
    for (name, source) in [
        (
            "infinite.wasm",
            r#"(module
  (memory (export "memory") 1)
  (func (export "rainy_action") (result i64)
    (loop $spin (br $spin))
    unreachable))"#,
        ),
        (
            "memory.wasm",
            r#"(module
  (memory (export "memory") 1025)
  (func (export "rainy_action") (result i64) (i64.const 0)))"#,
        ),
        (
            "output.wasm",
            r#"(module
  (memory (export "memory") 1)
  (func (export "rainy_action") (result i64) (i64.const 5242881)))"#,
        ),
        (
            "input.wasm",
            r#"(module
  (memory (export "memory") 1)
  (func (export "rainy_alloc") (param i32) (result i32) (i32.const 0))
  (func (export "rainy_action") (param i32 i32) (result i64) (i64.const 0)))"#,
        ),
    ] {
        write_bytes(
            &plugin_source.join(name),
            &wat::parse_str(source).expect("compile limited wasm fixture"),
        );
    }
    write(
        &plugin_source.join("plugin.json"),
        r#"{
  "protocolVersion": "rainy.plugin.v1",
  "name": "limited-wasm",
  "version": "0.1.0",
  "description": "Wasm resource limit fixtures",
  "commands": [{"name": "limited-wasm", "description": "Limit fixture"}],
  "actions": [
    {"id": "limit.fuel", "description": "Exhaust fuel", "runtime": "wasm", "wasm": "infinite.wasm"},
    {"id": "limit.memory", "description": "Exceed memory", "runtime": "wasm", "wasm": "memory.wasm"},
    {"id": "limit.output", "description": "Exceed output", "runtime": "wasm", "wasm": "output.wasm"},
    {"id": "limit.input", "description": "Exceed input", "runtime": "wasm", "wasm": "input.wasm"}
  ],
  "permissions": {"fs": {"read": [], "write": []}, "network": "none", "secrets": []}
}
"#,
    );
    let plugin_source_string = plugin_source.to_string_lossy().to_string();
    run(&[
        "--workspace",
        &app_path,
        "plugin",
        "install",
        &plugin_source_string,
        "--apply",
    ]);

    for (action, code) in [
        ("limit.fuel", "PLUGIN_WASM_FAILED"),
        ("limit.memory", "PLUGIN_WASM_FAILED"),
        ("limit.output", "PLUGIN_RESPONSE_TOO_LARGE"),
    ] {
        let output = rainy()
            .args([
                "--workspace",
                &app_path,
                "plugin",
                "call",
                "limited-wasm",
                action,
                "--dry-run",
            ])
            .output()
            .expect("run limited wasm action");
        assert!(!output.status.success(), "{action} unexpectedly succeeded");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(code),
            "{action} did not report {code}"
        );
    }

    let input = temp.path().join("large-input.json");
    write(
        &input,
        &serde_json::json!({"payload": "x".repeat(1024 * 1024) }).to_string(),
    );
    let input_string = input.to_string_lossy().to_string();
    let output = rainy()
        .args([
            "--workspace",
            &app_path,
            "plugin",
            "call",
            "limited-wasm",
            "limit.input",
            "--input",
            &input_string,
            "--dry-run",
        ])
        .output()
        .expect("run oversized wasm input");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("PLUGIN_WASM_INPUT_TOO_LARGE"));
}

#[test]
fn policy_blocks_denied_paths() {
    let temp = TempDir::new().expect("tempdir");
    let root = temp.path().to_string_lossy().to_string();
    run(&["--workspace", &root, "new", "demo-saas", "--apply"]);
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
    run(&["--workspace", &root, "new", "demo-saas", "--apply"]);
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
    run(&["--workspace", &root, "new", "demo-saas", "--apply"]);
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

fn write_source_fixture(root: &Path, version: &str, module_content: &str) {
    write(
        &root.join("rainy-source.yaml"),
        &format!(
            r#"apiVersion: rainy.dev/v1
kind: RainySource
metadata:
  name: company-source
  version: {version}
requires:
  rainy: ">=0.5.0, <0.6.0"
contents:
  - id: enterprise-project-templates
    type: project-template-catalog
    path: catalogs/enterprise
    version: {version}
  - id: service-base
    type: project-template
    path: templates/service-base
    required: true
  - id: backend-a
    type: workspace-module
    path: modules/backend-a
    defaultTarget: services/backend-a
x-company-test: true
"#,
        ),
    );
    write(
        &root.join("catalogs/enterprise/project-templates.yaml"),
        r#"apiVersion: rainy.dev/v1
kind: ProjectTemplateCatalog
templates:
  enterprise-java-service:
    description: Enterprise Java service
    source:
      type: git
      url: https://git.example.com/templates/enterprise-java-service.git
      ref: main
    repository:
      defaultBranch: main
"#,
    );
    write(
        &root.join("templates/service-base/rainy.yaml.hbs"),
        r#"apiVersion: rainy.dev/v1
kind: Project
project:
  name: "{{ project.name }}"
  type: service
paths:
  backend: services/backend-a
  frontend: apps/frontend
  generated: generated
  evidence: evidence
package:
  java: "{{ package.java }}"
capabilityRegistry:
  sources: []
policy: {}
verify:
  profiles: {}
"#,
    );
    write(
        &root.join("templates/service-base/capability.lock.hbs"),
        r#"lockfileVersion: 1
project:
  name: "{{ project.name }}"
rainy:
  version: "0.5.0"
capabilities: {}
skills: []
"#,
    );
    write(
        &root.join("modules/backend-a/module.txt.hbs"),
        &format!("{module_content} for {{{{ project.name }}}}\n"),
    );
}

fn create_zip_from_directory(source: &Path, archive: &Path, prefix: &str) {
    let file = fs::File::create(archive).expect("create source archive");
    let mut archive = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    add_directory_to_zip(&mut archive, source, source, prefix, options);
    archive.finish().expect("finish source archive");
}

fn add_directory_to_zip(
    archive: &mut zip::ZipWriter<fs::File>,
    root: &Path,
    current: &Path,
    prefix: &str,
    options: zip::write::SimpleFileOptions,
) {
    for entry in fs::read_dir(current).expect("read source fixture") {
        let entry = entry.expect("source fixture entry");
        let path = entry.path();
        let relative = path.strip_prefix(root).expect("relative source path");
        let name = format!("{prefix}/{}", relative.to_string_lossy().replace('\\', "/"));
        if entry.file_type().expect("source fixture type").is_dir() {
            archive
                .add_directory(format!("{name}/"), options)
                .expect("add source archive directory");
            add_directory_to_zip(archive, root, &path, prefix, options);
        } else {
            archive
                .start_file(name, options)
                .expect("add source archive file");
            archive
                .write_all(&fs::read(path).expect("read source archive file"))
                .expect("write source archive file");
        }
    }
}

fn copy_directory(source: &Path, target: &Path) {
    fs::create_dir_all(target).expect("create copied directory");
    for entry in fs::read_dir(source).expect("read copied directory") {
        let entry = entry.expect("copied directory entry");
        let destination = target.join(entry.file_name());
        if entry.file_type().expect("copied file type").is_dir() {
            copy_directory(&entry.path(), &destination);
        } else {
            fs::copy(entry.path(), destination).expect("copy fixture file");
        }
    }
}

fn first_directory(path: &Path) -> std::path::PathBuf {
    path.read_dir()
        .expect("read directory")
        .filter_map(Result::ok)
        .find(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .expect("directory entry")
        .path()
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
