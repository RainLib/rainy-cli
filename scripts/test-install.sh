#!/usr/bin/env sh
set -eu

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
INSTALLER="$ROOT_DIR/scripts/install.sh"

fail() {
  echo "installer test failed: $1" >&2
  exit 1
}

check_target() {
  os="$1"
  arch="$2"
  expected="$3"
  actual="$(
    RAINY_INSTALLER_OS="$os" \
      RAINY_INSTALLER_ARCH="$arch" \
      RAINY_INSTALLER_PRINT_TARGET=1 \
      sh "$INSTALLER"
  )"
  [ "$actual" = "$expected" ] || fail "$os/$arch resolved to $actual, expected $expected"
}

checksum_digest() {
  file="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$file" | awk '{print $1}'
  else
    return 1
  fi
}

check_checksum() {
  tmp_dir="$(mktemp -d)"
  trap 'rm -rf "$tmp_dir"' EXIT INT TERM
  archive="$tmp_dir/rainy-x86_64-unknown-linux-gnu.tar.gz"
  checksum="$archive.sha256"
  printf '%s\n' "rainy test archive" >"$archive"
  digest="$(checksum_digest "$archive")" || {
    echo "sha256sum/shasum not found; skipping checksum installer tests"
    return 0
  }
  printf '%s  %s\n' "$digest" "$(basename "$archive")" >"$checksum"
  RAINY_INSTALLER_CHECKSUM_ARCHIVE="$archive" \
    RAINY_INSTALLER_CHECKSUM_FILE="$checksum" \
    sh "$INSTALLER"

  printf '%s  %s\n' "0000000000000000000000000000000000000000000000000000000000000000" "$(basename "$archive")" >"$checksum"
  if RAINY_INSTALLER_CHECKSUM_ARCHIVE="$archive" \
    RAINY_INSTALLER_CHECKSUM_FILE="$checksum" \
    sh "$INSTALLER" >/dev/null 2>"$tmp_dir/checksum.err"; then
    fail "checksum mismatch was accepted"
  fi
  grep -Eqi "FAILED|mismatch|did NOT match" "$tmp_dir/checksum.err" || fail "checksum mismatch did not produce an explanatory error"
  rm -rf "$tmp_dir"
  trap - EXIT INT TERM
}

check_target Linux x86_64 x86_64-unknown-linux-gnu
check_target Linux amd64 x86_64-unknown-linux-gnu
check_target Linux aarch64 aarch64-unknown-linux-gnu
check_target Linux arm64 aarch64-unknown-linux-gnu
check_target Darwin x86_64 x86_64-apple-darwin
check_target Darwin amd64 x86_64-apple-darwin
check_target Darwin arm64 aarch64-apple-darwin
check_target Darwin aarch64 aarch64-apple-darwin

if RAINY_INSTALLER_OS=Linux \
  RAINY_INSTALLER_ARCH=armv7 \
  RAINY_INSTALLER_PRINT_TARGET=1 \
  sh "$INSTALLER" >/dev/null 2>"${TMPDIR:-/tmp}/rainy-installer-unsupported.err"; then
  fail "unsupported Linux/armv7 target was accepted"
fi
grep -q "unsupported platform Linux/armv7" "${TMPDIR:-/tmp}/rainy-installer-unsupported.err" || fail "unsupported platform error was not clear"
rm -f "${TMPDIR:-/tmp}/rainy-installer-unsupported.err"

check_checksum

echo "installer tests passed"
