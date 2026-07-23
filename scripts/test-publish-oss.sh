#!/usr/bin/env sh
set -eu

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
PUBLISHER="$ROOT_DIR/scripts/publish-oss.sh"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT INT TERM
DIST_DIR="$TMP_DIR/dist"
BIN_DIR="$TMP_DIR/bin"
LOG_FILE="$TMP_DIR/ossutil.log"
mkdir -p "$DIST_DIR" "$BIN_DIR"

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
  printf '%s\n' "$asset" >"$DIST_DIR/$asset"
done

cat >"$BIN_DIR/ossutil" <<'EOF'
#!/usr/bin/env sh
printf '%s\n' "$*" >>"$OSSUTIL_TEST_LOG"
EOF
chmod +x "$BIN_DIR/ossutil"

PATH="$BIN_DIR:$PATH" \
  OSSUTIL_TEST_LOG="$LOG_FILE" \
  RAINY_OSS_DEST=oss://rainy-test/releases \
  sh "$PUBLISHER" v0.3.5 "$DIST_DIR" >/dev/null

last_line="$(tail -n 1 "$LOG_FILE")"
case "$last_line" in
  "cp "*" oss://rainy-test/releases/latest.txt --force") ;;
  *)
    echo "mirror test: latest.txt was not published last: $last_line" >&2
    exit 1
    ;;
esac
grep -F "oss://rainy-test/releases/v0.3.5/rainy-x86_64-apple-darwin.tar.gz --force" "$LOG_FILE" >/dev/null
grep -F "oss://rainy-test/releases/install.sh --force" "$LOG_FILE" >/dev/null

if PATH="$BIN_DIR:$PATH" \
  OSSUTIL_TEST_LOG="$LOG_FILE" \
  RAINY_OSS_DEST=oss://rainy-test/releases \
  sh "$PUBLISHER" latest "$DIST_DIR" >/dev/null 2>&1; then
  echo "mirror test: invalid tag was accepted" >&2
  exit 1
fi

echo "OSS mirror publisher tests passed"
