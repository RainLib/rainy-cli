---
name: rainy-cli
description: Safely inspect, version, compose, plan, apply, verify, audit, and manage Rainy software projects through Rainy CLI. Use when an agent needs to consume a self-describing enterprise Rainy Source, initialize a project, manage capability packs or OpenSpec/Superpowers/Comet integration, inspect project or Skill health, apply a reviewed execution plan, generate evidence, or troubleshoot Rainy manifests and locks. Also use when Rainy CLI may not be installed because this skill bootstraps the verified official release before continuing.
---

# Rainy CLI

Use Rainy CLI as the deterministic execution boundary for project changes. Keep the model responsible for intent and review; keep planning, policy enforcement, file writes, rollback, verification, and audit inside Rainy.

## Bootstrap Rainy

Perform this step before every Rainy workflow. Do not assume a previous shell added Rainy to `PATH`.

On Linux or macOS, resolve this skill directory and run:

```sh
RAINY_BIN="$(sh "<skill-dir>/scripts/ensure-rainy.sh")"
```

On Windows PowerShell, run:

```powershell
$RainyBin = & "<skill-dir>\scripts\ensure-rainy.ps1"
```

The bootstrap script:

1. Uses `RAINY_BIN`, `PATH`, or the default `$HOME/.rainy/bin` installation when available.
2. Runs `rainy --version` to reject a broken executable.
3. If absent, resolves an immutable release from GitHub or `RAINY_RELEASE_BASE_URL`, then downloads the installer and `installers.sha256` from that version.
4. Verifies the installer checksum before execution.
5. Installs Rainy and returns its absolute executable path.

Stop immediately when bootstrap fails. Use the returned absolute path for every subsequent command so installation can continue in the same process without restarting the shell.

## Discover Project State

Locate the intended repository root. A Skill-only repository may not contain `rainy.yaml`; do not
initialize a full Rainy capability project unless the user explicitly requested it. When `rainy.yaml`
exists, use the full project inspection commands below.

For a complete Rainy project, start with read-only JSON commands:

```sh
"$RAINY_BIN" defaults status --json
"$RAINY_BIN" --workspace "$WORKSPACE" agent context --json
"$RAINY_BIN" --workspace "$WORKSPACE" doctor --json
"$RAINY_BIN" --workspace "$WORKSPACE" capability installed --json
```

If defaults are missing, preview `rainy defaults install`, obtain approval, then run it with
`--apply`. Do not invent a source or ref: use the official version-matched package or an explicitly
configured enterprise mirror. In offline mode, report `DEFAULTS_OFFLINE_MISSING` instead of bypassing
the package manager.

When `rainy-skills.yaml` exists, also run:

```sh
"$RAINY_BIN" --workspace "$WORKSPACE" skill status --json
"$RAINY_BIN" --workspace "$WORKSPACE" skill doctor --json
```

If it selects the `comet` profile, load the sibling `rainy-comet` Skill and follow its ownership and phase rules.

When only `rainy-skills.yaml` exists, use `skill status`, `skill doctor`, or top-level
`doctor --scope auto|skills`. Do not request `doctor --scope project` unless `rainy.yaml` exists.

Read [references/commands.md](references/commands.md) when selecting commands. Read [references/safety.md](references/safety.md) before any mutating workflow or plugin operation.

Agents must not rely on terminal selectors. Pass `--profile`, `--language`, every `--target`,
every requested project `--skill`, `--workspace`, and `--json` explicitly on first install. Universal
`.agents/skills` is always added; request platform copies only for hosts the user actually selected.
If `rainy-skills.yaml` already exists, do not repeat profile or target setup options: pass only the
reviewed `--skill` selection, use `--no-custom-skills` for an explicitly empty selection, or omit Skill
flags to preserve the current selection. Never use `--all-custom-skills` without explicit user intent.

Project-owned Skill sources live under `rainy-skills/<SKILL_ID>/`. Use `rainy skill create` only after
the user asks to create one, then let the user or model edit `SKILL.md`, `references/`, and `scripts/`
inside the reviewed project boundary. Rainy installs and hashes scripts but never executes them during
Skill installation. Route reusable company Skills through Registry exports instead of duplicating them
across project libraries.

## Change Capabilities

Always separate planning from mutation:

1. Create a dry-run plan and save it with `--output-plan`.
2. Present the plan, diff, policy result, and warnings for review.
3. Apply only after the user explicitly approves that plan.
4. Apply the saved plan with `rainy apply --plan ... --apply`; do not reconstruct it from prose.
5. Run `doctor`, the appropriate `verify` profile, and evidence generation.
6. Report changed files, verification status, and audit location.

Use `--trace-id` for a user request that spans multiple Rainy commands. Never add `--allow-native-plugin` or set `RAINY_ALLOW_NATIVE_PLUGIN` unless the user explicitly trusts a reviewed native plugin.

## Route Enterprise Content

Read [references/enterprise.md](references/enterprise.md) when the request involves company packages,
internal registries, platform policy, approval, IAM, deployment, or enterprise Skills. Route declarative
project changes to capability packs, multi-content distribution to a self-describing Rainy Source,
project capability selection to a private registry, mandatory boundaries to layered policy, and external
systems to Wasm plugins or HTTPS adapters. Prefer `rainy source check --project` when a generated project
contains `.rainy/project-source.lock`. Never put credentials in Source manifests, Rainy config, packs,
generated templates, locks, or Skills.

## Handle Errors

Parse `rainy.command.v1`. Successful command-specific fields are under `data`; operational errors are
under `error` on `stderr`. A Doctor, Verify, Schema, or Conformance failure remains a complete result on
`stdout` and exits `4`. Address the reported configuration, policy, dependency, or verification problem;
do not bypass the failing gate. Preserve the workspace and plan artifacts when escalation is needed.
