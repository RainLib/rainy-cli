#!/usr/bin/env sh
set -eu

RAINY_BIN="${RAINY_BIN:-target/debug/rainy}"
export RAINY_BIN

response="$({
  printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}'
} | python3 integrations/mcp/rainy_mcp.py)"

python3 -c 'import json,sys; value=json.loads(sys.stdin.read()); assert value["jsonrpc"] == "2.0"; assert value["result"]["serverInfo"]["version"] == "0.1.2"' <<EOF
$response
EOF

printf '%s\n' 'MCP wrapper smoke passed'
