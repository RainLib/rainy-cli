#!/usr/bin/env sh
set -eu

REPO="${RAINY_REPO:-RainLib/rainy-cli}"
VERSION="${RAINY_VERSION:-${1:-latest}}"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.rainy/bin}"
RELEASE_BASE_URL="${RAINY_RELEASE_BASE_URL:-}"
LATEST_VERSION_URL="${RAINY_LATEST_VERSION_URL:-}"

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
  version_url="$LATEST_VERSION_URL"
  if [ -z "$version_url" ] && [ -n "$RELEASE_BASE_URL" ]; then
    version_url="${RELEASE_BASE_URL%/}/latest.txt"
  fi

  if [ -n "$version_url" ]; then
    validate_download_url "$version_url"
    curl -fsSL \
      --connect-timeout 20 \
      --max-time 90 \
      --retry 3 \
      --retry-delay 2 \
      --retry-all-errors \
      -H "User-Agent: rainy-installer" \
      "$version_url" \
      | sed -n '1{s/\r$//;p;}'
  else
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
  fi
}

validate_download_url() {
  case "$1" in
    https://* | http://127.0.0.1:* | http://localhost:*) ;;
    *)
      echo "rainy installer: download URL must use HTTPS or loopback HTTP: $1" >&2
      return 1
      ;;
  esac
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

shell_profile() {
  if [ -n "${RAINY_SHELL_PROFILE:-}" ]; then
    printf '%s\n' "$RAINY_SHELL_PROFILE"
    return
  fi

  shell_name="$(basename "${SHELL:-sh}")"
  case "$shell_name" in
    zsh)
      printf '%s\n' "${ZDOTDIR:-$HOME}/.zshrc"
      ;;
    bash)
      profile_os="${RAINY_INSTALLER_OS:-$(uname -s)}"
      if [ "$profile_os" = "Darwin" ]; then
        printf '%s\n' "$HOME/.bash_profile"
      else
        printf '%s\n' "$HOME/.bashrc"
      fi
      ;;
    fish)
      printf '%s\n' "$HOME/.config/fish/config.fish"
      ;;
    *)
      printf '%s\n' "$HOME/.profile"
      ;;
  esac
}

persist_install_path() {
  if [ "${RAINY_NO_MODIFY_PATH:-0}" = "1" ]; then
    echo "PATH was not modified because RAINY_NO_MODIFY_PATH=1."
    return
  fi

  path_was_available=0
  case ":$PATH:" in
    *":$INSTALL_DIR:"*) path_was_available=1 ;;
  esac

  case "$INSTALL_DIR" in
    *'
'*)
      echo "rainy installer: INSTALL_DIR must not contain a newline" >&2
      return 1
      ;;
  esac

  profile="$(shell_profile)"
  profile_dir="$(dirname "$profile")"
  mkdir -p "$profile_dir"
  touch "$profile"

  quoted_install_dir="$(printf '%s' "$INSTALL_DIR" | sed "s/'/'\\\\''/g")"
  shell_name="$(basename "${SHELL:-sh}")"
  if [ "$shell_name" = "fish" ]; then
    path_line="fish_add_path -- '$quoted_install_dir'"
  else
    path_line="export PATH='$quoted_install_dir':\$PATH"
  fi

  if ! grep -Fqx "$path_line" "$profile"; then
    printf '\n%s\n%s\n' '# Added by the Rainy CLI installer.' "$path_line" >>"$profile"
    echo "Added $INSTALL_DIR to PATH in $profile."
  else
    echo "$INSTALL_DIR is already configured in $profile."
  fi

  export PATH="$INSTALL_DIR:$PATH"
  if [ "$path_was_available" -eq 0 ]; then
    echo "Open a new terminal, or refresh the current shell with:"
    if [ "$shell_name" = "fish" ]; then
      echo "  source \"$profile\""
    else
      echo "  . \"$profile\""
    fi
  fi
}

persist_release_source() {
  if [ -z "$RELEASE_BASE_URL" ]; then
    return
  fi
  source_home="${RAINY_HOME:-$HOME/.rainy}"
  source_file="$source_home/release-source"
  source_tmp="$source_file.tmp.$$"
  mkdir -p "$source_home"
  printf '%s\n' "${RELEASE_BASE_URL%/}" >"$source_tmp"
  mv "$source_tmp" "$source_file"
  echo "Saved Rainy release mirror to $source_file."
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

if [ -n "${RAINY_INSTALLER_BASE_URL:-}" ]; then
  BASE_URL="${RAINY_INSTALLER_BASE_URL%/}"
elif [ -n "$RELEASE_BASE_URL" ]; then
  BASE_URL="${RELEASE_BASE_URL%/}/$VERSION"
else
  BASE_URL="https://github.com/$REPO/releases/download/$VERSION"
fi
validate_download_url "$BASE_URL"
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
persist_release_source
persist_install_path
