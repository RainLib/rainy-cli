# Command Workflows

Use the absolute executable returned by the bootstrap script as `RAINY_BIN`. Pass `--workspace` explicitly when the current directory is not guaranteed.

## Inspect

```sh
"$RAINY_BIN" --workspace "$WORKSPACE" agent context --json
"$RAINY_BIN" --workspace "$WORKSPACE" capability list --json
"$RAINY_BIN" --workspace "$WORKSPACE" capability installed --json
"$RAINY_BIN" --workspace "$WORKSPACE" capability explain <id> --json
"$RAINY_BIN" --workspace "$WORKSPACE" capability graph --json
"$RAINY_BIN" --workspace "$WORKSPACE" doctor --json
```

## Initialize

Only initialize when explicitly requested:

```sh
"$RAINY_BIN" --workspace "$PARENT" new <name> --golden-path spring-nextjs-saas --package <java-package> --dry-run --json
"$RAINY_BIN" --workspace "$PARENT" new <name> --golden-path spring-nextjs-saas --package <java-package> --apply --json
```

For an enterprise Source, inspect and register the exact immutable release before project creation:

```sh
"$RAINY_BIN" --workspace "$PARENT" source inspect "$SOURCE_URL" --ref "$SOURCE_REF" --json
"$RAINY_BIN" --workspace "$PARENT" source add "$SOURCE_NAME" "$SOURCE_URL" --ref "$SOURCE_REF" --dry-run --json
"$RAINY_BIN" --workspace "$PARENT" source add "$SOURCE_NAME" "$SOURCE_URL" --ref "$SOURCE_REF" --apply --json
"$RAINY_BIN" --workspace "$PARENT" new <name> --source "$SOURCE_NAME" --template "$TEMPLATE" --module "$MODULES" --package <java-package> --dry-run --json
"$RAINY_BIN" --workspace "$PARENT" new <name> --source "$SOURCE_NAME" --template "$TEMPLATE" --module "$MODULES" --package <java-package> --apply --json
```

Use `--channel`/`--version` for a Rainy Source Index and `--sha256` for a direct archive. Do not combine
transport-specific options or infer source/module selections.

## Plan And Apply

Create and review a stable plan file:

```sh
"$RAINY_BIN" --workspace "$WORKSPACE" --trace-id "$TRACE_ID" capability add <id> --provider <provider> --dry-run --output-plan "$PLAN" --json
```

After explicit approval, apply that exact file:

```sh
"$RAINY_BIN" --workspace "$WORKSPACE" --trace-id "$TRACE_ID" apply --plan "$PLAN" --apply --json
```

Use the same dry-run, review, and explicit apply sequence for capability upgrade/removal, pack installation/update, and plugin installation/calls.

## Verify

```sh
"$RAINY_BIN" --workspace "$WORKSPACE" doctor --json
"$RAINY_BIN" --workspace "$WORKSPACE" verify --profile local --json
"$RAINY_BIN" --workspace "$WORKSPACE" verify --profile ci --json
"$RAINY_BIN" --workspace "$WORKSPACE" evidence generate --format all --apply --json
```

Use `local` during interactive development. Use `ci` as the strict production gate.

## Synchronize Agent Context

```sh
"$RAINY_BIN" --workspace "$WORKSPACE" agent init --apply --json
"$RAINY_BIN" --workspace "$WORKSPACE" skill sync --apply --json
```

These commands synchronize project context and installed capability information. They do not replace this model-facing Skill package.

## Manage Skill Profiles

Create a project-owned Skill only when requested:

```sh
"$RAINY_BIN" --workspace "$WORKSPACE" skill create <skill-id> --description <text> --dry-run --json
"$RAINY_BIN" --workspace "$WORKSPACE" skill create <skill-id> --description <text> --apply --json
```

When `rainy-skills.yaml` is missing, preview and install the default project-scoped OpenSpec +
Superpowers + Comet profile with an explicit project Skill selection:

```sh
"$RAINY_BIN" --workspace "$WORKSPACE" skill install --profile comet --target codex,claude,cursor --language zh --skill <skill-id> --dry-run --json
"$RAINY_BIN" --workspace "$WORKSPACE" skill install --profile comet --target codex,claude,cursor --language zh --skill <skill-id> --apply --json
"$RAINY_BIN" --workspace "$WORKSPACE" skill status --json
"$RAINY_BIN" --workspace "$WORKSPACE" skill doctor --json
```

`init`, `install`, `update`, and `uninstall` preview by default. In a JSON preview, read
`data.report.applyCommand`, present it for approval, and execute that exact Rainy command only after
approval. `data.report.command` is an upstream command Rainy may invoke internally; never execute it as
a substitute. `--yes` is an explicit alias for `--apply`, but generated automation should prefer
the canonical spelling.

The target and Skill lists above are illustrative. Agents must pass only hosts and project Skills selected
by the user and must not enter interactive selectors. Universal `.agents/skills` is added automatically.
Use repeatable `--skill` or comma-separated IDs. Use `--all-custom-skills` only when the user explicitly
approved every valid directory under `rainy-skills/`. Use `--no-custom-skills` when the user explicitly
wants no project-owned Skills; do not use an omitted selection to infer removal.

Manage an existing profile:

```sh
"$RAINY_BIN" --workspace "$WORKSPACE" skill install --skill <skill-id> --dry-run --json
"$RAINY_BIN" --workspace "$WORKSPACE" skill install --skill <skill-id> --apply --json
"$RAINY_BIN" --workspace "$WORKSPACE" skill install --no-custom-skills --apply --json
"$RAINY_BIN" --workspace "$WORKSPACE" skill update --dry-run --json
"$RAINY_BIN" --workspace "$WORKSPACE" skill update --apply --json
"$RAINY_BIN" --workspace "$WORKSPACE" skill uninstall --dry-run --json
"$RAINY_BIN" --workspace "$WORKSPACE" skill uninstall --apply --json
```

An existing profile can be intentionally reconfigured with explicit `--profile`, `--language`, and
`--target` values. Omit selectors only when the current value must be preserved. Source directories under
`rainy-skills/` remain user-owned and are not removed by uninstall.

Never infer `--apply` approval from a Comet transition. Use `--force` only after reviewing modified managed Skill files.

## Manage Rainy Version

```sh
"$RAINY_BIN" self check --json
"$RAINY_BIN" self update
"$RAINY_BIN" self update --apply
"$RAINY_BIN" self skip <version> --apply
```

## Enterprise Capabilities

```sh
"$RAINY_BIN" defaults status --json
"$RAINY_BIN" defaults install --dry-run --json
"$RAINY_BIN" defaults doctor --json
"$RAINY_BIN" source list --json
"$RAINY_BIN" --workspace "$WORKSPACE" source check --project --json
"$RAINY_BIN" --workspace "$WORKSPACE" source update --project --json
"$RAINY_BIN" --workspace "$WORKSPACE" source update --project --apply --json
"$RAINY_BIN" source resolve "$SOURCE_NAME" "$CONTENT_ID" --json
"$RAINY_BIN" schema validate --schema org-policy --file "$WORKSPACE/.rainy/org-policy.yaml" --json
"$RAINY_BIN" conformance check --path "$PACK_ROOT" --json
"$RAINY_BIN" --workspace "$WORKSPACE" pack install "$PACK_SOURCE" --dry-run --json
"$RAINY_BIN" --workspace "$WORKSPACE" pack install "$PACK_SOURCE" --apply --json
"$RAINY_BIN" --workspace "$WORKSPACE" registry add "$REGISTRY" "$PACK_SOURCE" --apply --json
"$RAINY_BIN" --workspace "$WORKSPACE" registry sync "$REGISTRY" --module "$MODULES" --dry-run --json
"$RAINY_BIN" --workspace "$WORKSPACE" registry sync "$REGISTRY" --module "$MODULES" --apply --json
```

Use `git+https://...` for Git, an HTTPS archive plus `--sha256`, or `http+https://.../index.json`.
Remote content is stored under `RAINY_HOME/registries`; never copy registry caches into the project.
Require a reviewed plan, checksums, strict verification, and evidence before reporting completion.
Self-describing Source content is stored under `RAINY_HOME/sources`; generated projects commit only
`.rainy/project-source.lock`, selected template/module output, and normal project files. Source update
refreshes the managed cache but never rewrites generated project files.
