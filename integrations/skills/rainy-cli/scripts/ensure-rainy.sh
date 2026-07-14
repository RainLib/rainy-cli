#!/usr/bin/env sh
set -eu

REPO="${RAINY_REPO:-RainLib/rainy-cli}"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.rainy/bin}"
RELEASE_URL="${RAINY_SKILL_RELEASE_URL:-https://github.com/$REPO/releases/latest/download}"

fail() {
  echo "rainy skill bootstrap: $1" >&2
  exit 1
}

resolve_command() {
  candidate="$1"
  [ -n "$candidate" ] || return 1
  if [ -x "$candidate" ]; then
    resolved="$candidate"
  elif command -v "$candidate" >/dev/null 2>&1; then
    resolved="$(command -v "$candidate")"
  else
    return 1
  fi
  case "$resolved" in
    /*) ;;
    *) resolved="$(CDPATH= cd -- "$(dirname -- "$resolved")" && pwd)/$(basename -- "$resolved")" ;;
  esac
  "$resolved" --version >&2 || return 1
  printf '%s\n' "$resolved"
}

checksum_digest() {
  file="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$file" | awk '{print $1}'
  else
    fail "sha256sum or shasum is required to verify the installer"
  fi
}

download() {
  url="$1"
  output="$2"
  curl -fsSL \
    --connect-timeout 20 \
    --max-time 90 \
    --retry 3 \
    --retry-delay 2 \
    --retry-all-errors \
    "$url" -o "$output"
}

if [ "${RAINY_SKILL_FORCE_INSTALL:-0}" != "1" ]; then
  if resolved="$(resolve_command "${RAINY_BIN:-}" 2>/dev/null)"; then
    printf '%s\n' "$resolved"
    exit 0
  fi
  if resolved="$(resolve_command rainy 2>/dev/null)"; then
    printf '%s\n' "$resolved"
    exit 0
  fi
  if resolved="$(resolve_command "$INSTALL_DIR/rainy" 2>/dev/null)"; then
    printf '%s\n' "$resolved"
    exit 0
  fi
fi

command -v curl >/dev/null 2>&1 || fail "curl is required to install Rainy CLI"
if ! printf '%s\n' "$REPO" | grep -Eq '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$'; then
  fail "invalid repository; expected owner/repo, got $REPO"
fi
case "$RELEASE_URL" in
  https://* | http://127.0.0.1:* | http://localhost:*) ;;
  *) fail "release URL must use HTTPS or loopback HTTP: $RELEASE_URL" ;;
esac

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT INT TERM
installer="$tmp_dir/install.sh"
checksums="$tmp_dir/installers.sha256"

echo "rainy command not found; installing the verified latest release" >&2
download "$RELEASE_URL/install.sh" "$installer"
download "$RELEASE_URL/installers.sha256" "$checksums"

expected="$(awk '$2 == "install.sh" { print $1; exit }' "$checksums")"
case "$expected" in
  '' | *[!0-9a-fA-F]*) fail "installers.sha256 has no valid install.sh digest" ;;
esac
[ "${#expected}" -eq 64 ] || fail "installers.sha256 has an invalid install.sh digest"
actual="$(checksum_digest "$installer")"
[ "$(printf '%s' "$actual" | tr 'A-F' 'a-f')" = "$(printf '%s' "$expected" | tr 'A-F' 'a-f')" ] \
  || fail "install.sh checksum verification failed"

RAINY_REPO="$REPO" INSTALL_DIR="$INSTALL_DIR" sh "$installer" >&2
resolved="$(resolve_command "$INSTALL_DIR/rainy")" \
  || fail "Rainy CLI was installed but its executable could not be verified"
printf '%s\n' "$resolved"
