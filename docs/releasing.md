# Release Checklist

1. Update `CHANGELOG.md` and the workspace version.
2. Run `make production-check` locally with a clean worktree, then merge the release change to `main`.
   Pushes to `main` and pull requests do not trigger GitHub builds.
3. Create an annotated `vX.Y.Z` tag whose version matches Cargo exactly.
4. Push the tag. Do not move or reuse an existing release tag.
5. Confirm all five CLI archives, both Skill archives in tar/zip form, their
   checksums, both installers, `latest.txt`, SBOM, and build provenance are
   attached to the GitHub Release.
6. Run the Unix and Windows installation acceptance tests against the published
   version and verify `rainy --version`.
7. When an OSS mirror is configured, download the complete release assets and
   run `scripts/publish-oss.sh`. Verify the mirror installer and
   `rainy self check` before announcing the mirror.

The release workflow is tag-triggered only. Do not use a branch name as a release ref.
