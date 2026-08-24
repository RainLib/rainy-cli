#!/usr/bin/env sh
set -eu

SCRIPT="scripts/check-release-version.sh"
PACKAGE_VERSION="$({ cargo metadata --no-deps --format-version 1; } | python3 -c 'import json,sys; data=json.load(sys.stdin); print(next(p["version"] for p in data["packages"] if p["name"] == "rainy-cli"))')"
RELEASE_TAG="v$PACKAGE_VERSION"
MAJOR_VERSION="${PACKAGE_VERSION%%.*}"
MISMATCHED_TAG="v$((MAJOR_VERSION + 1)).0.0"

[ "$(sh "$SCRIPT" "$RELEASE_TAG")" = "$RELEASE_TAG" ]

if sh "$SCRIPT" "$MISMATCHED_TAG" >/dev/null 2>&1; then
  echo "release test: mismatched Cargo version was accepted" >&2
  exit 1
fi

if sh "$SCRIPT" >/dev/null 2>&1; then
  echo "release test: missing tag was accepted" >&2
  exit 1
fi

if sh "$SCRIPT" main >/dev/null 2>&1; then
  echo "release test: branch name was accepted" >&2
  exit 1
fi

RELEASE_WORKFLOW=".github/workflows/release.yml"
grep -q '^  push:$' "$RELEASE_WORKFLOW"
grep -q '^    tags:$' "$RELEASE_WORKFLOW"
grep -q '^      - "v\*\.\*\.\*"$' "$RELEASE_WORKFLOW"
grep -q '^          make_latest: true$' "$RELEASE_WORKFLOW"
grep -q '^      - name: Verify published latest release assets$' "$RELEASE_WORKFLOW"
grep -q 'releases/latest/download/install.sh' "$RELEASE_WORKFLOW"
if grep -Eq '^  (branches|pull_request|pull_request_target|schedule|workflow_dispatch):' "$RELEASE_WORKFLOW"; then
  echo "release test: release workflow has a non-tag trigger" >&2
  exit 1
fi

CI_WORKFLOW=".github/workflows/ci.yml"
grep -q '^  pull_request:$' "$CI_WORKFLOW"
grep -q '^  push:$' "$CI_WORKFLOW"
grep -q '^    branches: \[main\]$' "$CI_WORKFLOW"
grep -q '^  workflow_dispatch:$' "$CI_WORKFLOW"
if grep -Eq '^  (pull_request_target|schedule):' "$CI_WORKFLOW"; then
  echo "release test: $CI_WORKFLOW has an unsupported trigger" >&2
  exit 1
fi

SECURITY_WORKFLOW=".github/workflows/security.yml"
grep -q '^  workflow_dispatch:$' "$SECURITY_WORKFLOW"
if grep -Eq '^  (push|pull_request|pull_request_target|schedule):' "$SECURITY_WORKFLOW"; then
  echo "release test: $SECURITY_WORKFLOW has an unexpected automatic trigger" >&2
  exit 1
fi

printf '%s\n' 'release input tests passed'
