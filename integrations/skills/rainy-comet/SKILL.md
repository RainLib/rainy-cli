---
name: rainy-comet
description: Coordinate OpenSpec requirements, Superpowers engineering practices, Comet phase orchestration, and Rainy CLI execution controls in Rainy-managed projects. Use for feature, tweak, or hotfix work when rainy-skills.yaml selects the comet profile, when openspec changes or .comet state exist, or when an agent must resume a Comet workflow without bypassing Rainy planning, approval, verification, evidence, and audit gates.
---

# Rainy Comet Workflow

Use Comet to control workflow phase and resume state. Use OpenSpec for intent and acceptance criteria, Superpowers for engineering method, and Rainy as the only capability mutation boundary.

## Bootstrap

Load the sibling `rainy-cli` Skill and run its Rainy bootstrap before doing anything else. Use the returned absolute Rainy executable for every Rainy command.

Then inspect the project without mutation:

```sh
"$RAINY_BIN" --workspace "$WORKSPACE" skill doctor --json
comet status "$WORKSPACE" --json
```

Stop when the Skill doctor fails. Do not repair a broken or partially installed profile by manually copying upstream files; use `rainy skill install --apply` or `rainy skill update --apply` after approval.

## Route Work

- Use direct Rainy read-only commands for discovery, doctor, installed capability inspection, schema validation, and evidence reading.
- Use the Comet full workflow for features that need requirements, design, an implementation plan, and acceptance verification.
- Use Comet tweak for bounded changes that still need an OpenSpec delta.
- Use Comet hotfix for urgent defects where the reduced workflow is explicitly acceptable.

Read [references/ownership.md](references/ownership.md) before changing files or moving a phase.

## Execute The Workflow

1. Ask Comet for the current state and next workflow Skill. Resume the existing change instead of creating a duplicate.
2. During open and design, write product intent and acceptance criteria through OpenSpec. Do not mutate application or capability files.
3. During build, follow the selected Superpowers method. For Rainy capability changes, generate and save a dry-run execution plan.
4. Present the Rainy plan, diff, policy result, and warnings. Wait for explicit user approval.
5. Apply the exact saved plan with `rainy apply --plan <path> --apply`. Never infer approval from a Comet phase transition or task checkbox.
6. During verify, run `rainy doctor`, `rainy verify --profile ci`, and `rainy evidence generate`. Attach their results to the Comet/OpenSpec verification record.
7. Archive only after acceptance criteria and Rainy verification pass and branch handling is complete.

Keep Comet `auto_transition` disabled. Phase progression may update Comet state, but it must not authorize Rainy mutation, native plugins, deployment, database migration, or secret writes.

For enterprise capabilities, keep product acceptance criteria in OpenSpec and the company implementation
in a private Rainy pack or plugin. Preserve the same trace ID in the Rainy plan, enterprise approval, strict
verification, and evidence handoff. Never copy credentials into Comet state or Skill content.

## Handle Drift

Treat `rainy-skills.yaml` as desired state and `skills.lock` as installed state. Do not edit generated upstream Skills or locked Rainy Skill copies in place. Use `rainy skill status` to inspect drift and `rainy skill doctor` to enforce it.

When Comet, OpenSpec, and Rainy disagree, stop at the strictest gate: acceptance criteria must be explicit, the current Comet phase must permit the action, and Rainy policy must allow the exact execution plan.
