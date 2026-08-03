# CLI Output Style

Rainy uses one output contract for humans and a separate stable JSON protocol for automation.

## Help Layout

Top-level help lists only recommended command paths. Compatibility entries remain executable but are
hidden from the primary command list. Resource lifecycle operations use the noun-first form, for example
`rainy capability add|upgrade|remove`. Every command and leaf includes runnable examples, required
values use `<VALUE>`, and command-specific `Options` are separated from inherited `Global Options`.

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
  Code    SKILL_CUSTOM_NOT_FOUND
  Reason  project Skill release-review was not found under rainy-skills/

Next steps
  $ rainy skill create release-review --description "Review releases" --apply
  $ rainy skill install --skill release-review --apply
```

Error codes and JSON error envelopes remain stable for scripts. Human recovery commands are
additive guidance.

Command-line input failures use `CLI_ARGUMENT_INVALID` and exit with code `2`; unknown top-level
commands use `EXTERNAL_COMMAND_NOT_FOUND` and exit with code `1`. In `--json` mode, both use the
same error envelope on `stderr`, while `stdout` stays empty. This makes invalid invocation
unambiguous without contaminating a redirected result stream.

## Streams And Modes

- Final human or JSON results go to `stdout`.
- Progress, prompts, and errors go to `stderr`.
- `--json` and `--quiet` disable progress and interaction.
- Redirected or non-terminal input disables interaction.
- `--progress auto` is reserved for commands that can take noticeable time; read-only output stays
  quiet unless `--progress always` is explicitly requested.
- `--no-color` and `NO_COLOR` disable color. Progress switches to stable line output rather than
  ANSI cursor redraws; interactive selectors retain the cursor control they require.
- `TERM=dumb` disables color and dynamic progress rendering.
- `--verbose` expands diagnostics without changing the JSON schema.

## Cancellation

`Ctrl+C` exits with code `130`. When a dynamic progress line is active, Rainy clears it before
exiting. Atomic filesystem writes already committed remain valid; incomplete work is never
reported as a successful apply. Callers that need recovery should rerun the same preview command
and inspect the resulting plan before applying again.

## Interaction

Interactive selectors use arrow keys to move, Space to toggle multi-select entries, and Enter to
confirm. Detected platforms are preselected. A required platform selector cannot accept an empty result;
the project Skill selector may intentionally select none.
Universal `.agents/skills` is displayed as always included and is added to every Skill profile.
Before installation, Rainy prints the selected bundle, targets, project Skills, and effective Skills and
requires a separate yes/no confirmation. Declining continues as a preview; accepting is equivalent to
explicit interactive apply approval.

Every interactive choice has an equivalent explicit flag so that the same operation can be replayed
without a terminal. For project Skills, those flags are `--skill`, `--all-custom-skills`, and
`--no-custom-skills`.

Skill installation can run in an ordinary Git repository without `rainy.yaml`. Use `rainy skill doctor`
for that workflow; `rainy doctor` validates a full Rainy capability workspace and therefore requires
`rainy.yaml`.

Completion generation is a raw-output exception to the human section layout. `rainy completion
<SHELL>` writes only the generated script to `stdout`, suppresses progress, and does not create an
audit entry, so shell evaluation and redirection remain deterministic.
