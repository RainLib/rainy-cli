# Skill Profile Management

Rainy provides an opt-in, project-scoped Skill manager. It installs pinned upstream releases without rewriting their content.

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

`comet` is the default AI development profile. It installs the Rainy execution Skill and Rainy-Comet bridge, asks a pinned Comet package to install OpenSpec and Comet Skills, and installs a pinned Superpowers release through a pinned `skills` CLI. Rainy requires and locks all three upstream components.

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

`rainy skill init` uses `comet`, `codex`, and `zh` by default and performs only a preview. Human-readable previews print `Apply this plan` with the exact Rainy command to run next. `--yes` is an explicit compatibility alias for `--apply`. An `Upstream command` shown in the preview is informational: Rainy runs it internally only during apply, so users should not copy its internal `npx --yes` flag into the Rainy command.

Supported targets are `codex`, `claude`, `cursor`, `github-copilot`, `gemini`, and `opencode`. Repeat `--target` or pass comma-separated values for multiple targets.

Rainy invokes Comet without a shell:

```text
npx --yes --package @rpamis/comet@<exact-version> comet init <workspace> \
  --yes --scope project --language <en|zh> --skip-existing --json
```

It installs Superpowers from an exact release tag without a shell:

```text
npx --yes --package skills@<exact-version> skills add \
  https://github.com/obra/superpowers/tree/v<exact-version>/skills \
  --yes --copy --agent <agent-host>
```

Set `RAINY_COMET_BIN` and `RAINY_SKILLS_BIN` to audited enterprise wrappers or local executables when npm execution is centrally managed.

## Managed State

`rainy-skills.yaml` is desired state and should be committed. It records the profile, language, target hosts, exact Comet, `skills` CLI, and Superpowers versions, plus Rainy approval policy. Profiles written by older Rainy versions are normalized to the current pinned defaults on the next install.

`skills.lock` is installed state and should also be committed. It records:

- Rainy CLI version
- Exact Comet package version
- Rainy-managed Skill paths and SHA-256 content digests
- Comet/OpenSpec paths and aggregate SHA-256 content digests
- Rainy-managed Superpowers paths and aggregate digest
- Installer output digest and installation timestamp

`skills-lock.json` is the pinned `skills` CLI's project index and should be committed as well. Rainy reads only entries sourced from `obra/superpowers`; unrelated Skill entries are preserved during uninstall.

`.comet/config.yaml` remains Comet-owned. Rainy merges `auto_transition: false` into it after install/update.

## Lifecycle

```bash
rainy skill install --dry-run
rainy skill install --apply
rainy skill status
rainy skill doctor
rainy skill update --comet-version <exact-semver> --dry-run
rainy skill update --comet-version <exact-semver> \
  --skills-version <exact-semver> \
  --superpowers-version <exact-semver> --apply
rainy skill uninstall --dry-run
rainy skill uninstall --apply
```

`init`, `install`, `update`, and `uninstall` default to dry-run. `--apply` or its `--yes` alias is mandatory for mutation. Rainy refuses to overwrite or remove any locked Rainy-managed or upstream Skill whose digest changed; use `--force` only after reviewing the local edits. Run `rainy skill <command> --help` for command-specific behavior and runnable examples.

If an older Rainy release left `rainy-skills.yaml` without `skills.lock` after an upstream failure, rerun the same `rainy skill init ... --apply` command or use `rainy skill install --apply`. Rainy treats that state as an interrupted installation and rebuilds the lock without requiring `--force`.

`update` runs the selected pinned Comet and Superpowers installers. It does not depend on mutable global installations.

`uninstall` first validates locked digests and asks Comet to remove its project artifacts, then removes only Rainy-owned and lock-recorded Skill directories plus the Rainy profile/lock. It preserves Rainy project configuration, capabilities, evidence, audit, and enterprise Agent context.

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
