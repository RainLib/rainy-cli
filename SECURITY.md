# Security Policy

## Supported Versions

Only the latest GitHub Release receives security fixes. Pre-release builds and
source snapshots are not supported production distributions.

## Reporting a Vulnerability

Use GitHub Security Advisories for private vulnerability reports. Do not open a
public issue before a fix is available. Include the affected version, platform,
reproduction steps, impact, and any proposed mitigation.

RainLib will acknowledge a complete report within five business days, provide
status updates while triage is active, and coordinate disclosure with the
reporter. Never include production credentials or customer data in a report.

## Trust Boundaries

Rainy verifies release checksums and publishes build provenance. Wasm plugins
are the default extension runtime. Native plugins run with the invoking user's
host permissions and require explicit trust via `--allow-native-plugin`.

Capability Pack integrity manifests detect content changes. Set
`RAINY_PACK_TRUSTED_PUBLIC_KEY` to require a cosign publisher signature for
loaded packs.
