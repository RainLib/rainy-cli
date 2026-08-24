# Rainy CLI 0.5 to 0.6 Migration

Rainy 0.6 adds build-system-neutral JVM changes, first-class Maven and Gradle execution, and
semantic capability ownership. Upgrade the CLI, Defaults, and enterprise Source together.

## Project configuration

Declare each build root under `build.targets`. `system: auto` fails with
`BUILD_SYSTEM_AMBIGUOUS` when Maven and Gradle markers are both visible. CI requires the matching
Wrapper; local runs may use a system executable and report a warning.

```yaml
build:
  targets:
    backend:
      root: .
      system: auto
      checkArgs: []
      packageArgs: []
      artifacts: []
verify:
  profiles:
    ci:
      - uses: doctor
      - uses: security.basic
      - uses: build.check
        with:
          target: backend
      - uses: build.package
        with:
          target: backend
```

Legacy string verification steps remain readable. New integrations should use structured steps.

## Capability actions and lock ownership

Use `jvm.addBom`, `jvm.addDependency`, and `jvm.removeDependency`. Maven actions update XML
dependency sections. Gradle 0.6 supports Groovy DSL only and creates stable Rainy-managed blocks.
The old `maven.*` actions remain readable but are deprecated.

New locks use `lockfileVersion: 2` and record `managedChanges`. Rainy will not infer ownership of
an existing Maven or Gradle file. A v1 lock must be reinstalled or explicitly reconciled before a
provider switch or destructive removal; otherwise Rainy returns `CAPABILITY_OWNERSHIP_REQUIRED`.

## Provider switching

Provider-specific `actions.install` and `validations` execute only for the selected provider.
Switch with:

```text
rainy capability switch-provider identity --provider ldap --dry-run
rainy capability switch-provider identity --provider ldap --apply
```

Review the dry-run plan before applying. Secrets must be represented by environment-variable
placeholders, never literal values in generated configuration.

## External Skills

Set `sourceSkill` when a repository exports more than one Skill. Keep `skillsPackage` pinned to an
exact installer version. Installation verifies the target directory, `SKILL.md`, and installed
content digest before updating the registry lock.
