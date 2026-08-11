# Enterprise Routing

When authoring or reviewing enterprise GitHub/GitLab content, follow `docs/source-management.md` for
self-describing distribution and `docs/enterprise-git-authoring.md` for Capability Registry details.
They define root manifests, module boundaries, CI gates, immutable releases, project consumption,
updates, and rollback.

## Decision Table

| Request | Rainy extension |
| --- | --- |
| Add dependencies, config, templates, CI, Helm, or SDK files | Capability pack |
| Publish templates, modules, Packs, Skills, and Plugins as one version | `rainy-source.yaml` plus `rainy source` |
| Create one project from a template plus selected modules | `rainy new --source --template --module` |
| Consume a legacy private Git starter | ProjectTemplateCatalog plus `rainy new --template` |
| Publish and pin internal capability versions | Named Git, archive, or HTTP registry |
| Deny paths or require approval action IDs | Layered policy |
| Call approval, IAM, CMDB, artifact, or deployment APIs | Wasm plugin or HTTPS adapter |
| Explain company terminology and workflow to a model | Enterprise Skill plus Rainy Skill |
| Prove installed versions and delivery checks | Lock, evidence, audit, trace ID |

## Workflow

1. If `rainy-source.yaml` exists, validate it and run `rainy source inspect` before inspecting nested content.
2. Inspect `rainy.yaml`, installed capabilities, policy, and doctor output with explicit `--workspace`
   and `--json` when a project already exists.
3. Validate private pack manifests and capability documents with Rainy schemas.
4. Run conformance against the containing pack or plugin directory.
5. Register or install in preview mode and present the exact JSON report.
6. After explicit approval, apply the same reviewed Rainy operation.
7. Add capabilities through a saved plan, then run doctor, strict CI verification, and evidence.
8. Report Source/Registry lock versions, digests, evidence paths, trace ID, and unresolved warnings.

For a self-describing Source, register Git, Archive, Index, or local content with `rainy source add`.
Use explicit `--ref`, `--version`, `--channel`, and `--sha256` values from the user or repository release;
never invent them. Agents must pass `--template` and every `--module` explicitly instead of opening a
terminal selector. Use `source resolve` to obtain a validated immutable path before handing a Pack or
Plugin to its dedicated command. Do not treat a refreshed Source cache as an upgraded generated project.
When `source check --project` reports `project-update-available`, review and migrate differences through
a PR; never overwrite project files automatically.

Associate named registries with `rainy registry add`, select modules with `rainy registry sync
<NAME> --module ...`, and use `--all` only when every module is intended. Git refs resolve to commit
IDs; archives require SHA-256; HTTP indexes verify every file and downloaded pack identity. Remote
content belongs under `RAINY_HOME/registries`, never under the workspace. In Agent and CI flows, install
exported enterprise Skills only with explicit `--install-skills --target ... --skill ...`; do not invoke
the terminal selector. Use `--all-skills` only when the user explicitly requests every export. Do not continue when checksums, publisher
signatures, local Skill drift, policy, approval, or verification fail.

Organization policy files are loaded from `/etc/rainy/policy.yaml`, `~/.rainy/policy.yaml`, and
`<workspace>/.rainy/org-policy.yaml` before project and capability policy. Denies and approval IDs
accumulate. `allowEdit` entries are additive, so absolute restrictions belong in `denyEdit`.

Never generate real credentials. Generate references to the enterprise secret provider and leave value
injection to workload identity, CI, Vault, or KMS.

For a legacy new project, inspect and validate the selected `ProjectTemplateCatalog`, preview `rainy new
<PROJECT_NAME> --template <TEMPLATE_ID>`, and require explicit approval before `--apply`. For a Rainy
Source project, preview the exact template and module set in the same way. Rainy must exclude source
`.git`, validate generated Rainy files, record project provenance, and print destination repository setup
commands. Do not run those Git commands or push until the user has created and confirmed the target URL.
