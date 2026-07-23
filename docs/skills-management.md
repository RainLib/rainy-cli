# Skill Profile Management

Rainy provides an opt-in, project-scoped Skill manager. It does not vendor or rewrite OpenSpec, Superpowers, or Comet.

## Ownership

| Component | Responsibility |
| --- | --- |
| OpenSpec | Requirements, delta specifications, acceptance criteria |
| Superpowers | Design, planning, TDD, debugging, review, verification method |
| Comet | Phase orchestration, resume state, handoff, transition guards |
| Rainy | Executable plan, policy, explicit apply, rollback, verification, evidence, audit |

Upstream projects remain independently maintained:

- [OpenSpec](https://github.com/Fission-AI/OpenSpec)
- [Superpowers](https://github.com/obra/superpowers)
- [Comet](https://github.com/rpamis/comet)

## Profiles

`comet` is the default AI development profile. It installs the Rainy execution Skill and Rainy-Comet bridge, then asks a pinned Comet package to install OpenSpec and Comet Skills. Superpowers is an independently installed, optional method library: Rainy detects and locks it when present, but its absence is a warning rather than an installation failure.

`rainy` installs only the Rainy execution Skill. It has no Node.js dependency.

Both profiles are project-scoped. Global host installation is intentionally unsupported because it would alter behavior in unrelated repositories.

## Install

The Comet profile requires Node.js 20 or newer, npm/npx, and Git.

```bash
rainy skill --help
rainy skill init --help
rainy skill init --profile comet --target codex --language zh --dry-run
rainy skill init --profile comet --target codex --language zh --apply
rainy skill status --json
rainy skill doctor --json
```

To add the optional Superpowers method library for Codex, use its upstream installer, then refresh the Rainy lock:

```bash
npx skills add obra/superpowers -y --agent codex
rainy skill install --apply
```

`rainy skill init` uses `comet`, `codex`, and `zh` by default and performs only a preview. Human-readable previews print `Apply this plan` with the exact Rainy command to run next. `--yes` is an explicit compatibility alias for `--apply`. An `Upstream command` shown in the preview is informational: Rainy runs it internally only during apply, so users should not copy its internal `npx --yes` flag into the Rainy command.

Supported targets are `codex`, `claude`, `cursor`, `github-copilot`, `gemini`, and `opencode`. Repeat `--target` or pass comma-separated values for multiple targets.

Rainy invokes Comet without a shell:

```text
npx --yes --package @rpamis/comet@<exact-version> comet init <workspace> \
  --yes --scope project --language <en|zh> --skip-existing --json
```

Set `RAINY_COMET_BIN` to an audited enterprise wrapper or local Comet executable when npm execution is centrally managed.

## Managed State

`rainy-skills.yaml` is desired state and should be committed. It records the profile, language, target hosts, exact Comet package, and Rainy approval policy.

`skills.lock` is installed state and should also be committed. It records:

- Rainy CLI version
- Exact Comet package version
- Rainy-managed Skill paths and SHA-256 content digests
- Comet/OpenSpec paths and aggregate SHA-256 content digests
- Superpowers paths and digests when that optional library is installed in the project
- Installer output digest and installation timestamp

`.comet/config.yaml` remains Comet-owned. Rainy merges `auto_transition: false` into it after install/update.

## Lifecycle

```bash
rainy skill install --dry-run
rainy skill install --apply
rainy skill status
rainy skill doctor
rainy skill update --comet-version <exact-semver> --dry-run
rainy skill update --comet-version <exact-semver> --apply
rainy skill uninstall --dry-run
rainy skill uninstall --apply
```

`init`, `install`, `update`, and `uninstall` default to dry-run. `--apply` or its `--yes` alias is mandatory for mutation. Rainy refuses to overwrite or remove a locked Rainy Skill whose digest changed; use `--force` only after reviewing the local edits. Run `rainy skill <command> --help` for command-specific behavior and runnable examples.

If an older Rainy release left `rainy-skills.yaml` without `skills.lock` after an upstream failure, rerun the same `rainy skill init ... --apply` command or use `rainy skill install --apply`. Rainy treats that state as an interrupted installation and rebuilds the lock without requiring `--force`.

`update` runs the selected pinned Comet package with `init --overwrite`. It does not depend on a mutable global Comet installation.

`uninstall` first asks Comet to remove its project artifacts, then removes Rainy-owned host Skill directories and the Rainy profile/lock. It preserves Rainy project configuration, capabilities, evidence, audit, and enterprise Agent context.

## Agent Context

`rainy agent init` and `rainy skill sync` update only the marked Rainy block in `AGENTS.md`:

```text
<!-- rainy:context:start -->
...
<!-- rainy:context:end -->
```

Comet-managed blocks and user-authored content outside this block are preserved. Old projects without `rainy-skills.yaml` retain the original context-only `rainy skill sync` behavior.

## Approval Rule

Comet phase completion never authorizes Rainy mutation. Commands containing `--apply`, native plugin trust, deployment, database migration, and secret writes retain their own explicit approval boundaries.

Run `rainy doctor`, `rainy verify --profile ci`, and `rainy evidence generate` before Comet archive. Record the resulting evidence path in the verification artifact.
