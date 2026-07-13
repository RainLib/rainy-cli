# Changelog

All notable changes are documented here. Rainy follows Semantic Versioning.

## [0.1.1] - Unreleased

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
