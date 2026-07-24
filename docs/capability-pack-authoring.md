# Capability Pack Authoring

A capability pack is a directory with `pack.yaml`, one or more capability
definitions, optional templates, validators, and skills. Rainy loads packs from
`rainy.yaml` registry sources plus the managed official defaults package.

For a complete enterprise GitHub/GitLab repository layout, CI gate, release policy,
Skills, Plugins, organization Policy, update, and rollback workflow, see
[Enterprise Git capability repository authoring](enterprise-git-authoring.md).

## Minimal Pack

```yaml
apiVersion: rainy.dev/v1
kind: CapabilityPack
metadata:
  name: example-pack
  version: 0.1.0
  owner: platform
  description: Example Rainy pack
requires:
  rainy: ">=0.1.0"
exports:
  capabilities:
    - capabilities/example.yaml
  validators: []
  skills: []
```

## Minimal Capability

```yaml
apiVersion: rainy.dev/v1
kind: Capability
id: example-capability
name: Example Capability
version: 0.1.0
description: Adds one generated file.
dependsOn: []
providers:
  - id: local
    default: true
inputs:
  message:
    type: string
    default: hello
actions:
  install:
    - id: create-example
      uses: file.create
      with:
        path: generated/example.txt
        content: "{{ inputs.message }}\n"
validations: []
doctor:
  checks:
    - id: example-file
      uses: file.exists
      with:
        path: generated/example.txt
agentRules:
  - Prefer this capability before manually wiring the same files.
```

If a capability declares multiple providers, exactly one provider should be
marked `default: true`, or callers must pass `--provider`. Passing `--provider`
to a providerless capability is rejected.

## Supported Built-in Actions

- `maven.addDependency`
- `maven.addBom`
- `yaml.merge`
- `json.merge`
- `jsonc.merge`
- `toml.merge`
- `template.render`
- `file.create`
- `file.append`
- `dockerCompose.addService`
- `packageJson.addDependency`
- `packageJson.addScript`
- `githubActions.addWorkflow`
- `devcontainer.merge`
- `helm.renderChart`
- `capabilityLock.update`
- `agentsMd.generate`
- `command.runValidation`

Template and path values use Handlebars variables:

- `paths.backend`, `paths.frontend`, `paths.generated`, `paths.evidence`
- `package.java`, `package.npmScope`
- `packagePath`
- `inputs.<name>`

Rendering runs in strict mode. Unknown variables fail planning instead of
silently producing broken files.

## Policy

Capabilities can declare local policy:

```yaml
policy:
  allowEdit:
    - generated/**
  denyEdit:
    - "**/*.pem"
  requireApproval:
    - db.migrate
```

Policy is checked before apply. Built-in sensitive path and dangerous command
denies cannot be bypassed by pack policy.

## Validation

Check schemas and conformance before publishing a pack:

```bash
rainy schema validate --schema capability-pack --file pack.yaml
rainy conformance check --path path/to/packs --json
```

Install and test locally:

```bash
rainy pack install path/to/packs --apply
rainy add capability example-capability --dry-run
rainy add capability example-capability --apply
rainy doctor --capability example-capability
rainy verify --profile local --capability example-capability
```
