# Changelog

All notable changes are documented here. Rainy follows Semantic Versioning.

## [Unreleased]

## [0.3.2] - 2026-07-22

### Changed

- Added workflow guidance and runnable examples to `rainy skill` and every
  Skill lifecycle subcommand.
- Skill dry-run output now distinguishes the next Rainy apply command from the
  internal upstream installer command.

### Added

- Added `--yes` as a visible compatibility alias for `--apply` on mutating
  Skill lifecycle commands.

## [0.3.1] - 2026-07-22

### Fixed

- Restricted PowerShell Skill E2E execution to Windows hosts and made the
  PowerShell Skill and installer tests derive the expected CLI version from the
  binary under test.
- Added Windows installer and Skill acceptance tests to the release build.

## [0.3.0] - 2026-07-22

### Added

- Added a distributable Rainy model Skill with safe plan/apply guidance and
  verified Unix and Windows CLI bootstrapping.
- Added release packaging and CI coverage for the model Skill.
- Added project-scoped Skill profile lifecycle commands for initialization,
  installation, synchronization, status, diagnostics, updates, and removal.
- Added the Comet profile for integrating OpenSpec, Superpowers, Comet, and
  Rainy across Codex, Claude, Cursor, GitHub Copilot, Gemini, and OpenCode.
- Added version-pinned Skill locks, content digests, drift detection, embedded
  Skill assets, JSON schemas, operator documentation, and lifecycle E2E tests.
- Added a Rainy-Comet bridge Skill and release archives for both bundled Skills.

### Changed

- Added bounded retries to installer metadata, archive, and checksum downloads
  and increased slow release download timeouts.
- Agent context updates now preserve content outside Rainy's managed markers.
- Skill-changing commands default to dry-run and require explicit `--apply`;
  Comet automatic transitions are disabled by managed configuration.
- CI and dependency security checks run for pull requests, scheduled scans, or
  manual dispatch instead of rerunning after merges to `main`.

### Security

- Skill installation rejects unsafe lock paths, duplicate targets, symlinks,
  unmanaged destination collisions, and unapproved content drift.

## [0.1.2] - 2026-07-13

### Fixed

- Increased release archive download timeouts on Unix and Windows installers
  so installations remain reliable on slow GitHub Release connections.

## [0.1.1] - 2026-07-13

### Added

- Deterministic multi-platform release validation, SBOM, and provenance.
- Native HTTPS update checks with timeout, backoff, and standard SemVer parsing.
- Explicit trust gate for native plugins and optional cosign Pack verification.
- Cross-platform CI, dependency security checks, and repository governance files.

### Changed

- Default release repository is `RainLib/rainy-cli`.
- Installers now require checksums, verify installed versions, and restore the
  previous binary when replacement fails.
- Capability locks record Pack source and digest.

### Security

- Remote registry and plugin adapter requests are size-limited and restricted
  to HTTPS or loopback HTTP.
