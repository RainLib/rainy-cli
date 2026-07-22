#!/usr/bin/env sh
set -eu

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
SKILL_DIR="$ROOT_DIR/integrations/skills/rainy-cli"
COMET_SKILL_DIR="$ROOT_DIR/integrations/skills/rainy-comet"
BOOTSTRAP="$SKILL_DIR/scripts/ensure-rainy.sh"
tmp_dir=""
server_pid=""

fail() {
  echo "skill test failed: $1" >&2
  exit 1
}

checksum_digest() {
  file="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$file" | awk '{print $1}'
  else
    fail "sha256sum or shasum is required"
  fi
}

cleanup() {
  if [ -n "$server_pid" ]; then
    kill "$server_pid" >/dev/null 2>&1 || true
    wait "$server_pid" 2>/dev/null || true
  fi
  if [ -n "$tmp_dir" ]; then
    rm -rf "$tmp_dir"
  fi
}
trap cleanup EXIT INT TERM

sh -n "$BOOTSTRAP"
[ -f "$SKILL_DIR/references/commands.md" ] || fail "commands reference is missing"
[ -f "$SKILL_DIR/references/safety.md" ] || fail "safety reference is missing"
[ -f "$SKILL_DIR/agents/openai.yaml" ] || fail "OpenAI skill metadata is missing"
[ -f "$COMET_SKILL_DIR/references/ownership.md" ] || fail "Rainy Comet ownership reference is missing"
[ -f "$COMET_SKILL_DIR/agents/openai.yaml" ] || fail "Rainy Comet OpenAI metadata is missing"
if grep -R "TODO" "$SKILL_DIR" "$COMET_SKILL_DIR" >/dev/null; then
  fail "skill contains unfinished TODO markers"
fi

python3 - "$SKILL_DIR/SKILL.md" "$COMET_SKILL_DIR/SKILL.md" <<'PY'
from pathlib import Path
import sys

for raw in sys.argv[1:]:
    path = Path(raw)
    lines = path.read_text(encoding="utf-8").splitlines()
    if not lines or lines[0] != "---":
        raise SystemExit(f"{path}: SKILL.md frontmatter is missing")
    try:
        end = lines.index("---", 1)
    except ValueError as exc:
        raise SystemExit(f"{path}: SKILL.md frontmatter is not closed") from exc
    keys = [line.split(":", 1)[0] for line in lines[1:end] if ":" in line]
    if keys != ["name", "description"]:
        raise SystemExit(f"{path}: SKILL.md frontmatter keys are invalid: {keys}")
    expected = f"name: {path.parent.name}"
    if lines[1] != expected or not lines[2].removeprefix("description: ").strip():
        raise SystemExit(f"{path}: SKILL.md metadata is incomplete")
PY

tmp_dir="$(mktemp -d)"
fake_bin="$tmp_dir/existing/rainy"
mkdir -p "$(dirname "$fake_bin")" "$tmp_dir/home"
printf '%s\n' '#!/usr/bin/env sh' "printf '%s\\n' 'rainy 7.8.9'" >"$fake_bin"
chmod +x "$fake_bin"
resolved="$(HOME="$tmp_dir/home" RAINY_BIN="$fake_bin" sh "$BOOTSTRAP" 2>"$tmp_dir/existing.log")"
[ "$resolved" = "$fake_bin" ] || fail "existing RAINY_BIN was not reused"

server_root="$tmp_dir/server"
release_dir="$server_root/release"
install_dir="$tmp_dir/installed"
port_file="$tmp_dir/port"
mkdir -p "$release_dir"
cat >"$release_dir/install.sh" <<'INSTALLER'
#!/usr/bin/env sh
set -eu
mkdir -p "$INSTALL_DIR"
cat >"$INSTALL_DIR/rainy" <<'RAINY'
#!/usr/bin/env sh
printf '%s\n' 'rainy 0.1.2'
RAINY
chmod +x "$INSTALL_DIR/rainy"
INSTALLER
digest="$(checksum_digest "$release_dir/install.sh")"
printf '%s  %s\n' "$digest" install.sh >"$release_dir/installers.sha256"

python3 "$ROOT_DIR/scripts/test-installer-server.py" "$server_root" "$port_file" 2 &
server_pid=$!
attempt=0
while [ ! -s "$port_file" ]; do
  attempt=$((attempt + 1))
  if [ "$attempt" -ge 200 ]; then
    fail "installer test server did not start"
  fi
  sleep 0.05
done
release_url="http://127.0.0.1:$(cat "$port_file")/release"

resolved="$(
  HOME="$tmp_dir/home" \
    INSTALL_DIR="$install_dir" \
    RAINY_SKILL_FORCE_INSTALL=1 \
    RAINY_SKILL_RELEASE_URL="$release_url" \
    sh "$BOOTSTRAP" 2>"$tmp_dir/install.log"
)"
[ "$resolved" = "$install_dir/rainy" ] || fail "installed Rainy path was not returned"
[ "$("$resolved" --version)" = "rainy 0.1.2" ] || fail "installed Rainy executable was not usable"

printf '%064d  install.sh\n' 0 >"$release_dir/installers.sha256"
if HOME="$tmp_dir/home" \
  INSTALL_DIR="$tmp_dir/rejected" \
  RAINY_SKILL_FORCE_INSTALL=1 \
  RAINY_SKILL_RELEASE_URL="$release_url" \
  sh "$BOOTSTRAP" >/dev/null 2>"$tmp_dir/checksum.log"; then
  fail "bootstrap accepted an installer with the wrong checksum"
fi
grep -q "checksum verification failed" "$tmp_dir/checksum.log" \
  || fail "bootstrap checksum failure was not explanatory"

if RAINY_SKILL_FORCE_INSTALL=1 \
  RAINY_SKILL_RELEASE_URL="http://example.com/release" \
  sh "$BOOTSTRAP" >/dev/null 2>"$tmp_dir/url.log"; then
  fail "bootstrap accepted a non-loopback HTTP release URL"
fi
grep -q "release URL must use HTTPS or loopback HTTP" "$tmp_dir/url.log" \
  || fail "bootstrap URL rejection was not explanatory"

echo "skill tests passed"
