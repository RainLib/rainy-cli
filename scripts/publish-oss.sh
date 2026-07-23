#!/usr/bin/env sh
set -eu

TAG="${1:-}"
DIST_DIR="${2:-dist}"
OSS_DEST="${RAINY_OSS_DEST:-}"

if ! printf '%s\n' "$TAG" | grep -Eq '^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'; then
  echo "usage: RAINY_OSS_DEST=oss://bucket/prefix $0 vX.Y.Z [dist-dir]" >&2
  exit 1
fi
if [ -z "$OSS_DEST" ]; then
  echo "rainy OSS publisher: RAINY_OSS_DEST is required" >&2
  exit 1
fi
case "$OSS_DEST" in
  oss://*) ;;
  *)
    echo "rainy OSS publisher: destination must start with oss://" >&2
    exit 1
    ;;
esac
if ! command -v ossutil >/dev/null 2>&1; then
  echo "rainy OSS publisher: ossutil is required" >&2
  exit 1
fi
if [ ! -d "$DIST_DIR" ]; then
  echo "rainy OSS publisher: dist directory not found: $DIST_DIR" >&2
  exit 1
fi

required_assets="
install.sh
install.ps1
installers.sha256
rainy-x86_64-unknown-linux-gnu.tar.gz
rainy-x86_64-unknown-linux-gnu.tar.gz.sha256
rainy-aarch64-unknown-linux-gnu.tar.gz
rainy-aarch64-unknown-linux-gnu.tar.gz.sha256
rainy-x86_64-apple-darwin.tar.gz
rainy-x86_64-apple-darwin.tar.gz.sha256
rainy-aarch64-apple-darwin.tar.gz
rainy-aarch64-apple-darwin.tar.gz.sha256
rainy-x86_64-pc-windows-msvc.zip
rainy-x86_64-pc-windows-msvc.zip.sha256
"
for asset in $required_assets; do
  if [ ! -s "$DIST_DIR/$asset" ]; then
    echo "rainy OSS publisher: required asset is missing: $DIST_DIR/$asset" >&2
    exit 1
  fi
done

destination="${OSS_DEST%/}"
for file in "$DIST_DIR"/*; do
  [ -f "$file" ] || continue
  name="$(basename "$file")"
  ossutil cp "$file" "$destination/$TAG/$name" --force
done

# Mutable entrypoints change only after the immutable version is complete.
ossutil cp "$DIST_DIR/install.sh" "$destination/install.sh" --force
ossutil cp "$DIST_DIR/install.ps1" "$destination/install.ps1" --force
latest_file="$(mktemp)"
trap 'rm -f "$latest_file"' EXIT INT TERM
printf '%s\n' "$TAG" >"$latest_file"
ossutil cp "$latest_file" "$destination/latest.txt" --force

echo "Published Rainy $TAG to $destination"
