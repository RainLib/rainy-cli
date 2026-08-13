# Rainy CLI

[![Release](https://img.shields.io/github/v/release/RainLib/rainy-cli?display_name=tag&sort=semver)](https://github.com/RainLib/rainy-cli/releases)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.88%2B-orange.svg)](https://www.rust-lang.org/)

[English](README.md) | [简体中文](README_CN.md)

Rainy is a deterministic capability orchestration CLI for developers, AI agents, and CI. It turns project setup and platform integration into a reviewable workflow:

```text
Plan -> Diff -> Policy -> Apply -> Doctor -> Verify -> Evidence
```

Instead of embedding enterprise starters into the CLI, Rainy consumes versioned Capability Packs, Sources, project-template catalogs, Skills, and Plugins. Every change can be previewed, policy-gated, validated, and recorded.

## Highlights

- Create built-in golden-path projects or enterprise Git templates without retaining template Git history.
- Discover and apply versioned capabilities with dry-run plans and transactional file updates.
- Distribute enterprise templates, modules, Packs, Skills, and Plugins through immutable cached Sources.
- Manage project-scoped Rainy, OpenSpec, Superpowers, and Comet Skill workflows.
- Provide stable JSON output, exit codes, audit records, verification reports, and evidence for agents and CI.
- Ship verified binaries for Linux x64/arm64, macOS Intel/Apple Silicon, and Windows x64.

## Install

macOS and Linux:

```bash
curl -fsSL https://github.com/RainLib/rainy-cli/releases/latest/download/install.sh | sh
```

Windows PowerShell:

```powershell
irm https://github.com/RainLib/rainy-cli/releases/latest/download/install.ps1 | iex
```

The installer selects the current platform archive, verifies its SHA-256 checksum, installs under `~/.rainy/bin` by default, and updates shell PATH configuration where supported. Open a new terminal after installation, then verify:

```bash
rainy --version
rainy --help
```

For internal mirrors, configure `RAINY_RELEASE_BASE_URL`. See [Release mirrors](docs/release-mirrors.md).

## Quick Start

Create an interactive golden-path or cached enterprise project:

```bash
rainy new demo-saas
```

Create a built-in project non-interactively:

```bash
rainy new demo-saas \
  --golden-path spring-nextjs-saas \
  --package com.example.demo \
  --apply

cd demo-saas
rainy doctor --scope auto
rainy verify --profile local
```

Preview and apply a capability:

```bash
rainy capability list
rainy capability add minio-file-storage --provider minio \
  --output-plan .rainy/plans/minio.json
rainy apply --plan .rainy/plans/minio.json --apply
```

All mutating commands preview by default. Use `--apply` (or `--yes`) to write changes. Relative `--plan` and `--output-plan` paths are resolved from `--workspace` and should normally live under `.rainy/plans/`.

## Enterprise Content

Register a self-describing enterprise Source once; Rainy verifies it and stores immutable content in `RAINY_HOME` rather than copying the whole distribution repository into each project:

```bash
rainy source inspect \
  git+ssh://git@git.example.com/platform/company-rainy-source.git \
  --ref v1.4.0

rainy source add company \
  git+ssh://git@git.example.com/platform/company-rainy-source.git \
  --ref v1.4.0 --apply

rainy new order-service --source company --template service-base \
  --module backend-java,delivery-gitlab --apply
```

Use [Source management](docs/source-management.md) for update behavior, archives, release indexes, and versioning. Use [Enterprise Git authoring](docs/enterprise-git-authoring.md) to build and publish your own Source, Pack, template, Skill, or Plugin.

## Skills and Agents

In a terminal, `rainy skill install` initializes a project profile when needed and presents workflow, agent-host, and project-Skill choices. The `rainy` profile has no Node.js dependency; the `comet` profile installs the Rainy execution Skills plus OpenSpec, Superpowers, and Comet.

```bash
# Interactive setup
rainy skill install

# Non-interactive Rainy-only setup
rainy skill install --profile rainy --target codex --no-custom-skills --apply

# Health checks
rainy skill status
rainy skill doctor
```

Supported project hosts are Universal (`.agents/skills`), Codex, Claude Code, Cursor, GitHub Copilot, Gemini CLI, and OpenCode. Read [Skill management](docs/skills-management.md) for lifecycle and ownership.

## Automation Contract

Use `--workspace`, `--json`, and explicit write flags in CI and agent integrations:

```bash
rainy --workspace "$WORKSPACE" --json capability add minio-file-storage \
  --provider minio --output-plan .rainy/plans/minio.json
rainy --workspace "$WORKSPACE" --json apply \
  --plan .rainy/plans/minio.json --apply
rainy --workspace "$WORKSPACE" --json doctor --scope all
rainy --workspace "$WORKSPACE" --json verify --profile ci
```

Normal results and failed check reports are emitted to stdout. Argument, configuration, network, and integrity errors are emitted to stderr. See the [command reference](docs/command-reference.md) for exit codes and output details.

## Documentation

- [Chinese user guide](docs/user-guide-zh.md)
- [Command reference](docs/command-reference.md)
- [Architecture and flow](docs/architecture-and-flow.md)
- [Capability Pack authoring](docs/capability-pack-authoring.md)
- [Enterprise Git authoring](docs/enterprise-git-authoring.md)
- [Source management](docs/source-management.md)
- [Skill management](docs/skills-management.md)
- [Release checklist](docs/releasing.md)

## Development

Requirements: Rust 1.88+, Python 3, and Node.js only when testing Comet-based Skill workflows.

```bash
make build
target/debug/rainy --help

make check
make release-check
```

Install the current worktree binary and refresh its local Defaults snapshot:

```bash
make install-local
```

Run `make help` for all local build, test, installer, mirror, and release targets.

## Releases

Only a pushed `vX.Y.Z` tag triggers the release workflow. It validates that the tag matches the Cargo version, runs release checks, builds five platform archives, verifies checksums, publishes installers, Skill archives, SBOM, and build provenance, then marks the published GitHub Release as Latest.

```bash
make production-check
git tag -a v0.5.5 -m "Rainy CLI v0.5.5"
git push origin v0.5.5
```

See [Release checklist](docs/releasing.md) for required acceptance checks and mirror publication.

## Repository Layout

```text
crates/rainy-cli/     Rust CLI implementation
community-packs/      Built-in capability packs
schemas/              JSON Schemas for configs, locks, plans, and reports
integrations/         Skills, MCP wrapper, and Backstage examples
examples/             Enterprise Pack and Source examples
docs/                 User, authoring, architecture, and release documentation
```

## License

Licensed under [Apache-2.0](LICENSE).
