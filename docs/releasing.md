# Release Checklist

1. Update `CHANGELOG.md` and the workspace version.
2. Run `make production-check` with a clean worktree.
3. Confirm the release pull request's required CI/security checks pass, then
   merge it to `main`. Pushes to `main` do not rerun these workflows.
4. Create an annotated `vX.Y.Z` tag whose version matches Cargo exactly.
5. Push the tag. Do not move or reuse an existing release tag.
6. Confirm all five CLI archives, both Skill archives in tar/zip form, their
   checksums, both installers, `latest.txt`, SBOM, and build provenance are
   attached to the GitHub Release.
7. Run the Unix and Windows installation acceptance tests against the published
   version and verify `rainy --version`.
8. When an OSS mirror is configured, download the complete release assets and
   run `scripts/publish-oss.sh`. Verify the mirror installer and
   `rainy self check` before announcing the mirror.

Manual workflow dispatch is recovery-only and requires an existing tag. It must
never be run against a branch name.
