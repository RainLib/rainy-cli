---
name: rainy-cli
description: Safely inspect, plan, apply, verify, audit, and manage model Skill profiles for Rainy-managed software projects through Rainy CLI. Use when an agent needs to initialize a Rainy project, manage capability packs or OpenSpec/Superpowers/Comet integration, inspect project or Skill health, apply a reviewed execution plan, generate evidence, or troubleshoot rainy.yaml, capability.lock, rainy-skills.yaml, and skills.lock. Also use when Rainy CLI may not be installed because this skill bootstraps the verified official release before continuing.
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
3. If absent, downloads the official installer and `installers.sha256` from the latest GitHub Release.
4. Verifies the installer checksum before execution.
5. Installs Rainy and returns its absolute executable path.

Stop immediately when bootstrap fails. Use the returned absolute path for every subsequent command so installation can continue in the same process without restarting the shell.

## Discover Project State

Locate the workspace containing `rainy.yaml`. Do not initialize or overwrite a project unless the user explicitly requested initialization.

Start with read-only JSON commands:

```sh
"$RAINY_BIN" --workspace "$WORKSPACE" agent context --json
"$RAINY_BIN" --workspace "$WORKSPACE" doctor --json
"$RAINY_BIN" --workspace "$WORKSPACE" capability installed --json
```

When `rainy-skills.yaml` exists, also run:

```sh
"$RAINY_BIN" --workspace "$WORKSPACE" skill status --json
"$RAINY_BIN" --workspace "$WORKSPACE" skill doctor --json
```

If it selects the `comet` profile, load the sibling `rainy-comet` Skill and follow its ownership and phase rules.

Read [references/commands.md](references/commands.md) when selecting commands. Read [references/safety.md](references/safety.md) before any mutating workflow or plugin operation.

## Change Capabilities

Always separate planning from mutation:

1. Create a dry-run plan and save it with `--output-plan`.
2. Present the plan, diff, policy result, and warnings for review.
3. Apply only after the user explicitly approves that plan.
4. Apply the saved plan with `rainy apply --plan ... --apply`; do not reconstruct it from prose.
5. Run `doctor`, the appropriate `verify` profile, and evidence generation.
6. Report changed files, verification status, and audit location.

Use `--trace-id` for a user request that spans multiple Rainy commands. Never add `--allow-native-plugin` or set `RAINY_ALLOW_NATIVE_PLUGIN` unless the user explicitly trusts a reviewed native plugin.

## Handle Errors

Parse the JSON error body and stable Rainy error code. Address the reported configuration, policy, dependency, or verification problem; do not bypass the failing gate. Preserve the workspace and plan artifacts when escalation is needed.
