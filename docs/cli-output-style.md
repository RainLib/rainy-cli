# CLI Output Style

Rainy uses one output contract for humans and a separate stable JSON protocol for automation.

## Human Output Order

Every multi-step command should present information in this order:

1. Command and operation name.
2. Summary with status and primary scope.
3. Effective components or checks.
4. One executable next step when action is required.
5. Grouped affected locations.
6. Optional details.

The default view answers three questions without exposing implementation noise:

- What happened?
- What will take effect?
- What command should run next?

Internal package-runner commands, every successful check, and every individual changed path belong
under `--verbose`. They must not compete with the next Rainy command.

## Skill Example

```text
Skill install

Summary
  Status    Preview only; no files changed
  Bundle    Complete workflow
  Targets   codex
  Language  zh

Enabled Skills
  Rainy CLI        execution, approval, verify, and evidence
  OpenSpec         requirements and acceptance criteria
  Superpowers      engineering methods and delivery workflow
  Comet            phase orchestration and recovery state

Next step
  $ rainy skill install --apply

Planned locations
  .agents/skills
  .comet
```

## Errors

An error is rendered once. Progress may report only `Failed in <duration>` and must not repeat the
error message.

```text
Error
  Code    SKILL_PROFILE_NOT_FOUND
  Reason  rainy-skills.yaml not found; run rainy skill init first

Next steps
  $ rainy skill init
  $ rainy skill init --help
```

Error codes and JSON error envelopes remain stable for scripts. Human recovery commands are
additive guidance.

## Streams And Modes

- Final human or JSON results go to `stdout`.
- Progress, prompts, and errors go to `stderr`.
- `--json` and `--quiet` disable progress and interaction.
- Redirected or non-terminal input disables interaction.
- `--no-color` disables color but keeps terminal cursor control required by selectors.
- `--verbose` expands diagnostics without changing the JSON schema.

## Interaction

Interactive selectors use arrow keys to move, Space to toggle multi-select entries, and Enter to
confirm. Detected platforms are preselected. A required selector cannot accept an empty result.
Universal `.agents/skills` is displayed as always included and is added to every Skill profile.
Before installation, Rainy prints the selected bundle, targets, and effective Skills and requires a
separate yes/no confirmation. Declining continues as a preview; accepting is equivalent to explicit
interactive apply approval.

Every interactive choice has an equivalent explicit flag so that the same operation can be replayed
without a terminal.
