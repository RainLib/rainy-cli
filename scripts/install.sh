#!/usr/bin/env sh
set -eu

REPO="${RAINY_REPO:-RainLib/rainy-cli}"
VERSION="${RAINY_VERSION:-${1:-latest}}"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.rainy/bin}"

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "rainy installer: required command not found: $1" >&2
    exit 1
  fi
}

download_file() {
  url="$1"
  output="$2"
  max_time="$3"
  curl -fL \
    --connect-timeout 20 \
    --max-time "$max_time" \
    --retry 3 \
    --retry-delay 2 \
    --retry-all-errors \
    "$url" -o "$output"
}

detect_target() {
  os="${RAINY_INSTALLER_OS:-$(uname -s)}"
  arch="${RAINY_INSTALLER_ARCH:-$(uname -m)}"
  case "$os:$arch" in
    Linux:x86_64|Linux:amd64)
      echo "x86_64-unknown-linux-gnu"
      ;;
    Linux:arm64|Linux:aarch64)
      echo "aarch64-unknown-linux-gnu"
      ;;
    Darwin:x86_64|Darwin:amd64)
      echo "x86_64-apple-darwin"
      ;;
    Darwin:arm64|Darwin:aarch64)
      echo "aarch64-apple-darwin"
      ;;
    *)
      echo "rainy installer: unsupported platform $os/$arch" >&2
      exit 1
      ;;
  esac
}

latest_version() {
  curl -fsSL \
    --connect-timeout 20 \
    --max-time 90 \
    --retry 3 \
    --retry-delay 2 \
    --retry-all-errors \
    -H "User-Agent: rainy-installer" \
    "https://api.github.com/repos/$REPO/releases/latest" \
    | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
    | head -n 1
}

checksum_verify() {
  archive="$1"
  checksum_file="$2"
  if command -v sha256sum >/dev/null 2>&1; then
    (cd "$(dirname "$archive")" && sha256sum -c "$(basename "$checksum_file")")
  elif command -v shasum >/dev/null 2>&1; then
    (cd "$(dirname "$archive")" && shasum -a 256 -c "$(basename "$checksum_file")")
  else
    echo "rainy installer: sha256sum or shasum is required" >&2
    return 1
  fi
}

if [ "${RAINY_INSTALLER_PRINT_TARGET:-0}" = "1" ]; then
  detect_target
  exit 0
fi

if [ -n "${RAINY_INSTALLER_CHECKSUM_ARCHIVE:-}" ] || [ -n "${RAINY_INSTALLER_CHECKSUM_FILE:-}" ]; then
  if [ -z "${RAINY_INSTALLER_CHECKSUM_ARCHIVE:-}" ] || [ -z "${RAINY_INSTALLER_CHECKSUM_FILE:-}" ]; then
    echo "rainy installer: RAINY_INSTALLER_CHECKSUM_ARCHIVE and RAINY_INSTALLER_CHECKSUM_FILE must both be set" >&2
    exit 1
  fi
  checksum_verify "$RAINY_INSTALLER_CHECKSUM_ARCHIVE" "$RAINY_INSTALLER_CHECKSUM_FILE"
  exit 0
fi

need curl
need tar

if ! printf '%s\n' "$REPO" | grep -Eq '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$'; then
  echo "RAINY_INSTALL_INVALID_REPOSITORY: expected owner/repo, got $REPO" >&2
  exit 1
fi

TARGET="$(detect_target)"
ASSET="rainy-$TARGET.tar.gz"

if [ "$VERSION" = "latest" ]; then
  VERSION="$(latest_version)"
fi

if [ -z "$VERSION" ]; then
  echo "rainy installer: could not resolve latest release version" >&2
  exit 1
fi

case "$VERSION" in
  v*) ;;
  *) VERSION="v$VERSION" ;;
esac
if ! printf '%s\n' "$VERSION" | grep -Eq '^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'; then
  echo "RAINY_INSTALL_INVALID_VERSION: expected vX.Y.Z, got $VERSION" >&2
  exit 1
fi

BASE_URL="${RAINY_INSTALLER_BASE_URL:-https://github.com/$REPO/releases/download/$VERSION}"
case "$BASE_URL" in
  https://* | http://127.0.0.1:* | http://localhost:*) ;;
  *)
    echo "rainy installer: release URL must use HTTPS or loopback HTTP: $BASE_URL" >&2
    exit 1
    ;;
esac
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT INT TERM

echo "Installing rainy $VERSION for $TARGET"
download_file "$BASE_URL/$ASSET" "$TMP_DIR/$ASSET" 900

download_file "$BASE_URL/$ASSET.sha256" "$TMP_DIR/$ASSET.sha256" 90 || {
  echo "rainy installer: checksum file is required: $ASSET.sha256" >&2
  exit 1
}
checksum_verify "$TMP_DIR/$ASSET" "$TMP_DIR/$ASSET.sha256"

mkdir -p "$INSTALL_DIR"
if tar -tzf "$TMP_DIR/$ASSET" | grep -Eq '(^/|(^|/)\.\.(/|$))'; then
  echo "rainy installer: archive contains an unsafe path" >&2
  exit 1
fi
tar -xzf "$TMP_DIR/$ASSET" -C "$TMP_DIR"
NEW_BIN="$INSTALL_DIR/.rainy.new.$$"
BACKUP_BIN="$INSTALL_DIR/.rainy.backup.$$"
install -m 0755 "$TMP_DIR/rainy" "$NEW_BIN"
if [ "$("$NEW_BIN" --version)" != "rainy ${VERSION#v}" ]; then
  rm -f "$NEW_BIN"
  echo "rainy installer: downloaded binary version does not match $VERSION" >&2
  exit 1
fi
if [ -f "$INSTALL_DIR/rainy" ]; then
  mv "$INSTALL_DIR/rainy" "$BACKUP_BIN"
fi
if mv "$NEW_BIN" "$INSTALL_DIR/rainy" && [ "$("$INSTALL_DIR/rainy" --version)" = "rainy ${VERSION#v}" ]; then
  rm -f "$BACKUP_BIN"
else
  rm -f "$INSTALL_DIR/rainy" "$NEW_BIN"
  if [ -f "$BACKUP_BIN" ]; then
    mv "$BACKUP_BIN" "$INSTALL_DIR/rainy"
  fi
  echo "rainy installer: installation verification failed; previous binary restored" >&2
  exit 1
fi

echo "rainy installed to $INSTALL_DIR/rainy"
case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    echo "Add this directory to PATH if needed:"
    echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
    ;;
esac
