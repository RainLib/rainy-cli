#!/usr/bin/env sh
set -eu

REPO="${RAINY_REPO:-rainy-dev/rainy}"
VERSION="${RAINY_VERSION:-${1:-latest}}"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.rainy/bin}"

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "rainy installer: required command not found: $1" >&2
    exit 1
  fi
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
  curl -fsSL -H "User-Agent: rainy-installer" "https://api.github.com/repos/$REPO/releases/latest" \
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
    echo "rainy installer: sha256 tool not found; skipping checksum verification" >&2
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

BASE_URL="https://github.com/$REPO/releases/download/$VERSION"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT INT TERM

echo "Installing rainy $VERSION for $TARGET"
curl -fL "$BASE_URL/$ASSET" -o "$TMP_DIR/$ASSET"

if curl -fsSL "$BASE_URL/$ASSET.sha256" -o "$TMP_DIR/$ASSET.sha256"; then
  checksum_verify "$TMP_DIR/$ASSET" "$TMP_DIR/$ASSET.sha256"
else
  echo "rainy installer: checksum file not found; continuing without checksum" >&2
fi

mkdir -p "$INSTALL_DIR"
tar -xzf "$TMP_DIR/$ASSET" -C "$TMP_DIR"
install -m 0755 "$TMP_DIR/rainy" "$INSTALL_DIR/rainy"

echo "rainy installed to $INSTALL_DIR/rainy"
case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    echo "Add this directory to PATH if needed:"
    echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
    ;;
esac
