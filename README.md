# Rainy CLI

Rainy is a Rust CLI for deterministic capability orchestration. It plans,
diffs, policy-checks, applies, verifies, and records evidence for project
capability changes.

## Install

From this repository:

```bash
cargo install --path crates/rainy-cli
```

For local development:

```bash
cargo build
target/debug/rainy --help
```

## Golden Path

Create the community Spring Boot + Next.js SaaS project:

```bash
rainy new demo-saas --golden-path spring-nextjs-saas
cd demo-saas
rainy capability list
rainy add capability minio-file-storage --provider minio --dry-run
rainy add capability minio-file-storage --provider minio --apply
rainy doctor
rainy verify --profile local
rainy evidence generate
```

All commands support the global `--json` flag for agents, CI, and integrations.
Mutating capability, pack, plugin, and plan commands default to dry-run and need
`--apply` to write changes.

## Extension Points

- Capability packs: see [docs/capability-pack-authoring.md](docs/capability-pack-authoring.md).
- Plugin protocol: see [docs/plugin-protocol.md](docs/plugin-protocol.md).
- MCP wrapper example: see [integrations/mcp](integrations/mcp).
- Backstage example: see [integrations/backstage](integrations/backstage).

## Development Checks

```bash
cargo fmt --check
cargo test --workspace
cargo clippy --all-targets --all-features -- -D warnings
```

