# Safety Rules

## Mutation Boundary

- Treat inspection, planning, doctor, and local verification as non-mutating operations.
- Require explicit user approval before any command that includes `--apply`.
- Apply a saved plan file rather than rebuilding approved changes from natural language.
- Do not edit generated capability files manually when a Rainy action owns them.
- Stop when Rainy reports a policy, approval, dependency, checksum, signature, or verification failure.

## Workspace Boundary

- Pass an explicit workspace rooted at the intended `rainy.yaml`.
- Do not scan unrelated home or system directories for Rainy projects.
- Do not initialize a missing project implicitly.
- Preserve `capability.lock` and `.rainy/audit.log` as managed records.
- Preserve `rainy-skills.yaml` as desired state and `skills.lock` as installed state.

## Composed Skills

- Keep Comet `auto_transition` disabled in Rainy-managed profiles.
- Treat OpenSpec as intent, Superpowers as engineering method, Comet as phase state, and Rainy as the execution boundary.
- Do not edit locked Rainy Skill copies or Comet-managed upstream Skills in place.
- Do not treat Comet phase advancement as approval for Rainy apply, native plugins, deployment, migration, or secret writes.

## Plugins

- Prefer Wasm plugins.
- Never enable native plugin trust merely to make a command succeed.
- Require explicit user approval after reviewing a native plugin source, manifest, command, and requested permissions.
- Do not expose the MCP stdio process directly over a network.

## Installation

- Use only the bundled bootstrap scripts for automatic installation.
- Keep the default repository `RainLib/rainy-cli` unless the user explicitly selects a trusted fork.
- Require HTTPS except for loopback URLs used in tests.
- Verify `install.sh` or `install.ps1` against `installers.sha256` before execution.
- Retry transient installer downloads with bounded delays; do not retry checksum or execution failures.
- Do not continue when installer verification or installed binary verification fails.

## Reporting

- Reuse one trace ID across commands serving the same request.
- Report plan path, applied files, verification result, and unresolved warnings.
- Do not claim success when strict verification was skipped or failed.
