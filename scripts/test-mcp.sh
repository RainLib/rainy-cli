#!/usr/bin/env sh
set -eu

RAINY_BIN="${RAINY_BIN:-target/debug/rainy}"
export RAINY_BIN
EXPECTED_VERSION="$($RAINY_BIN --version | awk '{print $2}')"
export EXPECTED_VERSION

response="$({
  printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}'
} | python3 integrations/mcp/rainy_mcp.py)"

python3 -c 'import json,os,sys; value=json.loads(sys.stdin.read()); assert value["jsonrpc"] == "2.0"; assert value["result"]["serverInfo"]["version"] == os.environ["EXPECTED_VERSION"]' <<EOF
$response
EOF

printf '%s\n' 'MCP wrapper smoke passed'
