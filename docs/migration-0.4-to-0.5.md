# Rainy CLI 0.4 to 0.5 Migration

Rainy 0.5 intentionally changes automation contracts. Upgrade Defaults and CLI together, then update
CI and Agent callers before creating a `v0.5.0` release tag.

## JSON Results

Successful command output now uses one envelope:

```json
{
  "protocolVersion": "rainy.command.v1",
  "type": "doctor",
  "status": "failed",
  "traceId": "build-123",
  "data": {
    "report": {
      "protocolVersion": "rainy.doctor.v1",
      "status": "failed",
      "checks": []
    }
  }
}
```

Read command-specific fields below `data`. Existing report protocols remain nested unchanged.
Operational errors use `rainy.command.v1` on `stderr` with
`error.code/category/message/details/retryable/nextSteps`.

## Streams And Exit Codes

- Normal and failed diagnostic reports use `stdout`.
- Argument, configuration, policy, network, integrity, I/O, and execution errors use `stderr`.
- Exit codes are `0` complete/preview/warning, `1` runtime or I/O, `2` argument/configuration,
  `3` policy/approval, `4` failed check report, `5` network/authentication, `6` integrity, and `130`
  cancellation.

CI must treat exit `4` as a parsed failed report, not an absent report or malformed invocation.

## Mutation And Help

Mutating commands, including `new`, `init app`, and `pack sign`, preview unless `--apply` or
its `--yes` alias is present. Interactive `rainy skill install` is the ergonomic exception: final
terminal confirmation applies the reviewed selection, while `--dry-run` keeps it preview-only.
`--force` never implies apply. Running `rainy` or a command group without
a child command prints that level's full help and exits `0`. Unknown spellings now return
`CLI_ARGUMENT_INVALID` with Clap suggestions; installed native plugin shortcuts still work.

## Workspace Discovery

Without `--workspace`, Rainy searches upward for the nearest `rainy.yaml` or `rainy-skills.yaml` and
stops at the nearest Git root. Scripts should continue to pass an explicit absolute workspace.

## Verify Definitions

Replace legacy shell strings:

```yaml
validations:
  - id: backend-tests
    command: ./mvnw test
```

with structured execution:

```yaml
validations:
  - id: backend-tests
    run:
      program: ./mvnw
      args: [test]
    workingDirectory: apps/backend
    timeoutSeconds: 900
    platforms: [linux, macos, windows]
```

Legacy strings are accepted only when they contain a simple executable and arguments. Shell operators
are rejected. External commands have bounded output, cancellation, and timeouts.

## Doctor And Configuration

Use `doctor --scope auto|project|skills|runtime|defaults|registries|all`. Network checks are opt-in via
`--network`. Core configuration fields are strict; put organization-specific data under `extensions`
or top-level `x-*` keys. `rainy.yaml` must use `apiVersion: rainy.dev/v1` and `kind: Project`.

## Defaults, Skills, And Mirrors

- Publish a Defaults tag compatible with `>=0.5.0, <0.6.0` before tagging the CLI.
- Reinstall Defaults once so `~/.rainy/defaults.lock` records `lockfileVersion: 1`, `packageVersion`,
  resolved revision, and the verified cache content digest. Offline mode rejects a drifted cache.
- `rainy skill install` can reselect the bundle, target hosts, and project Skills in a TTY; automation
  must pass explicit selectors.
- Universal `.agents/skills` remains mandatory. Codex, Claude, and Cursor copies are opt-in targets.
- Static release mirrors expose immutable `vX.Y.Z/` assets plus root `install.sh`, `install.ps1`,
  `installers.sha256`, and a `latest.txt` updated last.
- Set `RAINY_RELEASE_BASE_URL` so installation, Skill bootstrap, self-check, and self-update remain on
  the same mirror.

## Upgrade Checklist

1. Publish and verify the 0.5-compatible Defaults package.
2. Update capability validations to structured `run` definitions.
3. Update JSON consumers to read `data` and accept the new error shape and exit codes.
4. Add `--apply` to project creation and pack signing automation.
5. Run `rainy doctor --scope all`, then `rainy verify --profile ci`.
6. Install `cargo-audit` 0.22.x and `cargo-deny` 0.19.x, then run `make production-check` before
   creating `v0.5.0`.
