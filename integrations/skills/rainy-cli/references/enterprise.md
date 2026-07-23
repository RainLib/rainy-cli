# Enterprise Routing

## Decision Table

| Request | Rainy extension |
| --- | --- |
| Add dependencies, config, templates, CI, Helm, or SDK files | Capability pack |
| Publish and pin internal capability versions | Named Git, archive, or HTTP registry |
| Deny paths or require approval action IDs | Layered policy |
| Call approval, IAM, CMDB, artifact, or deployment APIs | Wasm plugin or HTTPS adapter |
| Explain company terminology and workflow to a model | Enterprise Skill plus Rainy Skill |
| Prove installed versions and delivery checks | Lock, evidence, audit, trace ID |

## Workflow

1. Inspect `rainy.yaml`, installed capabilities, policy, and doctor output with explicit `--workspace`
   and `--json`.
2. Validate private pack manifests and capability documents with Rainy schemas.
3. Run conformance against the containing pack or plugin directory.
4. Install the source in dry-run mode and present the exact JSON change report.
5. After explicit approval, apply the same reviewed Rainy operation.
6. Add the capability through a saved plan, then run doctor, strict CI verification, and evidence.
7. Report lock versions, digests, evidence paths, trace ID, and unresolved warnings.

Associate named registries with `rainy registry add`, select modules with `rainy registry sync
<NAME> --module ...`, and use `--all` only when every module is intended. Git refs resolve to commit
IDs; archives require SHA-256; HTTP indexes verify every file and downloaded pack identity. Remote
content belongs under `RAINY_HOME/registries`, never under the workspace. Install exported enterprise
Skills only with explicit `--install-skills --target ...`. Do not continue when checksums, publisher
signatures, local Skill drift, policy, approval, or verification fail.

Organization policy files are loaded from `/etc/rainy/policy.yaml`, `~/.rainy/policy.yaml`, and
`<workspace>/.rainy/org-policy.yaml` before project and capability policy. Denies and approval IDs
accumulate. `allowEdit` entries are additive, so absolute restrictions belong in `denyEdit`.

Never generate real credentials. Generate references to the enterprise secret provider and leave value
injection to workload identity, CI, Vault, or KMS.
