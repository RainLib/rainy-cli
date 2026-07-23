# Enterprise Example

This example is intentionally local and contains no credentials. It demonstrates a project policy,
a private capability pack with an exported Skill, deterministic planning, apply, doctor, and verification.

From the repository root:

```bash
cargo run -q -p rainy-cli -- schema validate --schema org-policy \
  --file examples/enterprise/project/.rainy/org-policy.yaml
cargo run -q -p rainy-cli -- conformance check --path examples/enterprise/packs
cargo run -q -p rainy-cli -- --workspace examples/enterprise/project capability list
cargo run -q -p rainy-cli -- --workspace examples/enterprise/project \
  add capability company-platform-baseline --dry-run
```

Copy the example project to a temporary directory before testing `--apply` so the repository fixture
remains unchanged.

To test Registry-managed installation from a copied project:

```bash
rainy --workspace <PROJECT_DIR> registry add company <ABSOLUTE_PATH>/examples/enterprise/packs --apply
rainy --workspace <PROJECT_DIR> registry sync company --module company-platform-baseline \
  --install-skills --target codex --apply
```

The selected Pack is cached under `~/.rainy/registries/company/<SOURCE_HASH>` and the exported Skill
is installed at `<PROJECT_DIR>/.agents/skills/company-platform`.
