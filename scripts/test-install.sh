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

check_download_install_and_rollback() {
  command -v python3 >/dev/null 2>&1 || {
    echo "python3 not found; skipping installer download E2E"
    return 0
  }
  tmp_dir="$(mktemp -d)"
  server_root="$tmp_dir/server"
  release_dir="$server_root/v0.1.1"
  install_dir="$tmp_dir/install"
  port_file="$tmp_dir/port"
  mkdir -p "$release_dir" "$tmp_dir/archive"

  write_fake_binary() {
    version="$1"
    printf '%s\n' '#!/usr/bin/env sh' "printf '%s\\n' 'rainy $version'" >"$tmp_dir/archive/rainy"
    chmod +x "$tmp_dir/archive/rainy"
    tar -czf "$release_dir/rainy-x86_64-unknown-linux-gnu.tar.gz" -C "$tmp_dir/archive" rainy
    digest="$(checksum_digest "$release_dir/rainy-x86_64-unknown-linux-gnu.tar.gz")"
    printf '%s  %s\n' "$digest" "rainy-x86_64-unknown-linux-gnu.tar.gz" >"$release_dir/rainy-x86_64-unknown-linux-gnu.tar.gz.sha256"
  }

  write_fake_binary 0.1.1
  python3 "$ROOT_DIR/scripts/test-installer-server.py" "$server_root" "$port_file" &
  server_pid=$!
  while [ ! -s "$port_file" ]; do sleep 0.05; done
  base_url="http://127.0.0.1:$(cat "$port_file")/v0.1.1"

  RAINY_VERSION=v0.1.1 \
    RAINY_INSTALLER_OS=Linux \
    RAINY_INSTALLER_ARCH=x86_64 \
    RAINY_INSTALLER_BASE_URL="$base_url" \
    INSTALL_DIR="$install_dir" \
    sh "$INSTALLER" >/dev/null
  [ "$("$install_dir/rainy" --version)" = "rainy 0.1.1" ] || fail "installed binary version was not verified"

  write_fake_binary 9.9.9
  if RAINY_VERSION=v0.1.1 \
    RAINY_INSTALLER_OS=Linux \
    RAINY_INSTALLER_ARCH=x86_64 \
    RAINY_INSTALLER_BASE_URL="$base_url" \
    INSTALL_DIR="$install_dir" \
    sh "$INSTALLER" >/dev/null 2>"$tmp_dir/wrong-version.err"; then
    fail "wrong binary version was installed"
  fi
  [ "$("$install_dir/rainy" --version)" = "rainy 0.1.1" ] || fail "existing binary was not preserved"

  rm -f "$release_dir/rainy-x86_64-unknown-linux-gnu.tar.gz.sha256"
  if RAINY_VERSION=v0.1.1 \
    RAINY_INSTALLER_OS=Linux \
    RAINY_INSTALLER_ARCH=x86_64 \
    RAINY_INSTALLER_BASE_URL="$base_url" \
    INSTALL_DIR="$install_dir" \
    sh "$INSTALLER" >/dev/null 2>"$tmp_dir/missing-checksum.err"; then
    fail "missing checksum was accepted"
  fi
  grep -q "checksum file is required" "$tmp_dir/missing-checksum.err" || fail "missing checksum error was unclear"
  [ "$("$install_dir/rainy" --version)" = "rainy 0.1.1" ] || fail "missing checksum replaced existing binary"

  kill "$server_pid" >/dev/null 2>&1 || true
  wait "$server_pid" 2>/dev/null || true
  rm -rf "$tmp_dir"
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
check_download_install_and_rollback

if RAINY_VERSION=not-a-version sh "$INSTALLER" >/dev/null 2>"${TMPDIR:-/tmp}/rainy-installer-version.err"; then
  fail "invalid version was accepted"
fi
grep -q "RAINY_INSTALL_INVALID_VERSION" "${TMPDIR:-/tmp}/rainy-installer-version.err" || fail "invalid version error code was missing"
rm -f "${TMPDIR:-/tmp}/rainy-installer-version.err"

echo "installer tests passed"
