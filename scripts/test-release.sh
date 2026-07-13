#!/usr/bin/env sh
set -eu

SCRIPT="scripts/check-release-version.sh"

[ "$(sh "$SCRIPT" v0.1.1)" = "v0.1.1" ]

if sh "$SCRIPT" v0.1.2 >/dev/null 2>&1; then
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
