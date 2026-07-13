# Release Checklist

1. Update `CHANGELOG.md` and the workspace version.
2. Run `make production-check` with a clean worktree.
3. Merge the release commit to `main` and confirm CI/security checks pass.
4. Create an annotated `vX.Y.Z` tag whose version matches Cargo exactly.
5. Push the tag. Do not move or reuse an existing release tag.
6. Confirm all five archives, their checksums, both installers, SBOM, and build
   provenance are attached to the GitHub Release.
7. Run the Unix and Windows installation acceptance tests against the published
   version and verify `rainy --version`.

Manual workflow dispatch is recovery-only and requires an existing tag. It must
never be run against a branch name.
