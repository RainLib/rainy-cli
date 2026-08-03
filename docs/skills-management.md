# Skill Profile Management

Rainy provides an opt-in, project-scoped Skill manager. It installs pinned upstream releases without rewriting their content.

Skill management is also available in a standard Git repository without `rainy.yaml` or
`capability.lock`. Standalone mode owns only `rainy-skills.yaml`, `skills.lock`, the managed block in
`AGENTS.md`, and selected host Skill directories. When both Rainy project files exist, synchronization
also refreshes the capability-aware `.enterprise-agent/` context.

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

The `comet` profile is an atomic workflow bundle: Rainy requires Rainy CLI, Rainy-Comet, OpenSpec,
Superpowers, and Comet together so doctor and lifecycle operations remain coherent. Individual enterprise
Skills are selected separately from Registry exports. Run `rainy registry sync <NAME> --module <PACK>
--install-skills --apply` in a terminal to select target hosts and Skill IDs, or pass `--target` plus
repeatable `--skill` values in automation. Use `--all-skills` only when every export is intended.

## Project Skill Library

Project-specific rules and optional helper commands live in `rainy-skills/<SKILL_ID>/`. They are source
content owned by the repository, not generated host copies and not embedded in the Rainy binary.

```bash
rainy skill create release-review \
  --description "Review enterprise releases" --apply
```

The scaffold contains:

```text
rainy-skills/release-review/
  SKILL.md
  references/README.md
  scripts/README.md
```

`SKILL.md` must start with YAML frontmatter containing `name` and `description`. The `name` must match
the directory. IDs use lowercase ASCII letters, digits, and internal hyphens, with a maximum length of
64 characters. Every direct child of `rainy-skills/` must be a real Skill directory; symlinks and loose
files are rejected so lock digests remain reproducible.

Edit the scaffold to add user rules, command documentation, reference material, and reviewed scripts.
Rainy copies the complete selected directory to each chosen host, but never executes scripts during
create or install. Add `rainy-skills/**` to `policy.allowEdit` in older projects; newly generated Rainy
projects include it by default.

Use this library for Skills owned by one project. Publish reusable company Skills as Registry Pack
`exports.skills`, so multiple projects can download pinned versions and receive controlled updates.

## Install

The Comet profile requires Node.js 20 or newer, npm/npx, and Git.

```bash
rainy skill --help
rainy skill install --help
rainy skill install
rainy skill install --profile comet --target codex --language zh \
  --skill release-review --dry-run
rainy skill install --profile comet --target codex --language zh \
  --skill release-review --apply
rainy skill status --json
rainy skill doctor --json
```

`rainy skill install` is the normal entry point. When `rainy-skills.yaml` is missing, it automatically
starts initialization. In a terminal, omitted values open three selection phases: the complete workflow
or Rainy-only bundle; one or more agent hosts; and zero or more valid project Skills discovered from
`rainy-skills/`. Universal `.agents/skills` is always included. Detected hosts are preselected and Codex
is selected when no host is detected. Rainy then displays the selected bundle, targets, project Skills,
and effective Skills and asks for explicit installation confirmation. Enter accepts the default `yes`;
choosing `no` returns the preview without writing files.

When a profile already exists, `skill install` keeps its workflow, language, targets, and package pins,
then opens only the project Skill selector and confirmation. To change profile setup, uninstall first.
The explicit `skill init` command remains available for callers that need to distinguish initialization,
but normal users do not need to invoke it.

Non-interactive callers never receive a prompt. Scripts, agents, JSON mode, redirected input, and CI use
`comet`, `codex`, and `zh` on first install when setup options are omitted. Pass `--skill` repeatedly or
as a comma-separated list, or use `--all-custom-skills`; on an existing profile, omitting both preserves
its current custom selection. Pass all initial options explicitly when the exact setup must be visible in
automation.

Human-readable previews show the result summary, enabled Skills, exact next Rainy command, and affected locations. Upstream commands and individual paths are available with `--verbose`. `--yes` is an explicit compatibility alias for `--apply`.

Supported targets are `universal`, `codex`, `claude`, `cursor`, `github-copilot`, `gemini`, and `opencode`. Universal is normalized into every profile even when it is omitted from explicit flags. Repeat `--target` or pass comma-separated values for multiple additional targets.

## Platform Directories

| Target | Canonical project Skill directory |
| --- | --- |
| Universal | `.agents/skills` |
| Codex | `.agents/skills` |
| Claude Code | `.claude/skills` |
| Cursor | `.cursor/skills` |
| GitHub Copilot | `.github/skills` |
| Gemini CLI | `.gemini/skills` |
| OpenCode | `.opencode/skills` |

Codex and Universal share `.agents/skills`. Cursor keeps its platform-specific Rainy copy in `.cursor/skills`, while upstream components that follow the universal Skills standard may also be discovered in `.agents/skills`. Codex previously used `.codex/skills`, while some OpenSpec integrations also emitted `.agent/skills`. During install and update, Rainy consolidates recognized Rainy, OpenSpec, Superpowers, and Comet Skill directories into `.agents/skills`. Identical duplicates are removed. Different copies produce `SKILL_LAYOUT_CONFLICT`; Rainy keeps both until the user reviews them or explicitly chooses `--force`. Non-Skill files such as `.codex/rules`, hooks, and `.agent/workflows` are preserved.

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

`rainy-skills.yaml` is desired state and should be committed. It records the profile, language, target
hosts, selected project Skill IDs, exact Comet, `skills` CLI, and Superpowers versions, plus Rainy
approval policy. Profiles written by older Rainy versions are normalized to the current pinned defaults
on the next install.

`skills.lock` is installed state and should also be committed. It records:

- Rainy CLI version
- Exact Comet package version
- Rainy-managed Skill paths and SHA-256 content digests
- Comet/OpenSpec paths and aggregate SHA-256 content digests
- Rainy-managed Superpowers paths and aggregate digest
- Selected project Skill IDs, installed host paths, and SHA-256 content digests
- Installer output digest and installation timestamp

`skills-lock.json` is the pinned `skills` CLI's project index and should be committed as well. Rainy reads only entries sourced from `obra/superpowers`; unrelated Skill entries are preserved during uninstall.

`.comet/config.yaml` remains Comet-owned. Rainy merges `auto_transition: false` into it after install/update.

## Lifecycle

```bash
rainy skill create <SKILL_ID> --description <TEXT> --apply
rainy skill install --dry-run
rainy skill install --skill <SKILL_ID> --dry-run
rainy skill install --skill <SKILL_ID> --apply
rainy skill install --all-custom-skills --apply
rainy skill install --no-custom-skills --apply
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

Interactive `init` and `install` may mutate only after the user accepts the terminal confirmation.
`--dry-run` always suppresses that confirmation and previews. Non-interactive `init`, `install`,
`update`, and `uninstall` require `--apply` or its `--yes` alias for mutation. Rainy refuses to overwrite
or remove any locked Rainy-managed, upstream, or project Skill copy whose digest changed; use `--force`
only after reviewing the local edits. Source directories under `rainy-skills/` are never removed by
install, selection changes, or uninstall. Run `rainy skill <command> --help` for command-specific
behavior and runnable examples.

`--skill`, `--all-custom-skills`, and `--no-custom-skills` are mutually exclusive. The final option is
the non-interactive equivalent of clearing every item in the project Skill selector: managed installed
copies are removed after drift validation, while the project-owned sources under `rainy-skills/` remain.

If an older Rainy release left `rainy-skills.yaml` without `skills.lock` after an upstream failure, run
`rainy skill install --apply`. Rainy treats that state as an interrupted installation and rebuilds the
lock without requiring `--force`.

Running `rainy skill init` again with the same configuration is idempotent. It reports `Already configured` and points to `rainy skill install --apply` instead of failing. Changing the bundle, language, targets, or package pins still requires an explicit uninstall first.

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
