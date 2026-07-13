#!/usr/bin/env sh
set -eu

TAG="${1:-}"
if ! printf '%s\n' "$TAG" | grep -Eq '^v[0-9]+\.[0-9]+\.[0-9]+$'; then
  echo "release tag must match vX.Y.Z: ${TAG:-<missing>}" >&2
  exit 1
fi

PACKAGE_VERSION="$({ cargo metadata --no-deps --format-version 1; } | python3 -c 'import json,sys; data=json.load(sys.stdin); print(next(p["version"] for p in data["packages"] if p["name"] == "rainy-cli"))')"
if [ "$TAG" != "v$PACKAGE_VERSION" ]; then
  echo "tag $TAG does not match Cargo version $PACKAGE_VERSION" >&2
  exit 1
fi

printf '%s\n' "$TAG"
