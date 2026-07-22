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
"$RAINY_BIN" --workspace "$PARENT" new <name> --golden-path spring-nextjs-saas --package <java-package> --json
```

## Plan And Apply

Create and review a stable plan file:

```sh
"$RAINY_BIN" --workspace "$WORKSPACE" --trace-id "$TRACE_ID" add capability <id> --provider <provider> --dry-run --output-plan "$PLAN" --json
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
"$RAINY_BIN" --workspace "$WORKSPACE" evidence generate --format all --json
```

Use `local` during interactive development. Use `ci` as the strict production gate.

## Synchronize Agent Context

```sh
"$RAINY_BIN" --workspace "$WORKSPACE" agent init --json
"$RAINY_BIN" --workspace "$WORKSPACE" skill sync --json
```

These commands synchronize project context and installed capability information. They do not replace this model-facing Skill package.

## Manage Skill Profiles

Preview and install the default project-scoped OpenSpec + Superpowers + Comet profile:

```sh
"$RAINY_BIN" --workspace "$WORKSPACE" skill init --profile comet --target codex --language zh --dry-run --json
"$RAINY_BIN" --workspace "$WORKSPACE" skill init --profile comet --target codex --language zh --apply --json
"$RAINY_BIN" --workspace "$WORKSPACE" skill status --json
"$RAINY_BIN" --workspace "$WORKSPACE" skill doctor --json
```

Manage an existing profile:

```sh
"$RAINY_BIN" --workspace "$WORKSPACE" skill install --dry-run --json
"$RAINY_BIN" --workspace "$WORKSPACE" skill install --apply --json
"$RAINY_BIN" --workspace "$WORKSPACE" skill update --dry-run --json
"$RAINY_BIN" --workspace "$WORKSPACE" skill update --apply --json
"$RAINY_BIN" --workspace "$WORKSPACE" skill uninstall --dry-run --json
"$RAINY_BIN" --workspace "$WORKSPACE" skill uninstall --apply --json
```

Never infer `--apply` approval from a Comet transition. Use `--force` only after reviewing modified managed Skill files.

## Manage Rainy Version

```sh
"$RAINY_BIN" self check --json
"$RAINY_BIN" self update
"$RAINY_BIN" self skip <version>
```
