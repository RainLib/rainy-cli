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

printf '%s\n' 'release input tests passed'
