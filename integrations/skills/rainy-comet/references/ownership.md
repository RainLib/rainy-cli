# Workflow Ownership

## Artifacts

| Owner | Purpose | Typical artifacts |
| --- | --- | --- |
| OpenSpec | Product intent, delta requirements, acceptance criteria | `openspec/changes/<change>/` |
| Superpowers | Design method, implementation plan, TDD, debugging, review | `docs/superpowers/` |
| Comet | Phase, resume state, handoff, transition guards | `.comet/`, change `.comet.yaml` |
| Rainy | Executable capability plan, policy, writes, rollback, verification, audit | plan JSON, `capability.lock`, evidence, `.rainy/audit.log` |
| Enterprise platform | Private capability implementation, IAM, approval, deployment, secret injection | private packs/plugins, approval record, platform audit |

Do not copy one owner's document into another owner's file. Link artifacts with change names, plan paths, trace IDs, and digests.

## Approval Boundaries

The following always require explicit approval even when Comet can advance automatically:

- `rainy ... --apply`
- Native plugin trust
- Deployment or cluster changes
- Database migration
- Secret or credential writes

Comet task completion and phase advancement are workflow evidence, not mutation approval.

## Verification Handoff

Before Comet archive:

1. Confirm OpenSpec acceptance criteria are covered.
2. Run the project tests selected by the Superpowers implementation plan.
3. Run Rainy doctor and strict CI verification.
4. Generate Rainy evidence and record its path in the verification report.
5. Preserve failed reports and state for diagnosis; do not mark a failed check as passed.
