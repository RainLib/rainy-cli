# Rainy MCP Wrapper

This example exposes Rainy CLI through a small stdio JSON-RPC wrapper. The
wrapper does not reimplement Rainy logic; every tool shells out to `rainy`
with `--json`.

Run locally:

```bash
RAINY_BIN=target/debug/rainy python3 integrations/mcp/rainy_mcp.py
```

Useful methods:

- `tools/list`
- `tools/call` with `list_capabilities`
- `tools/call` with `plan_add_capability`
- `tools/call` with `apply_plan`
- `tools/call` with `run_doctor`
- `tools/call` with `run_verify`
- `tools/call` with `generate_evidence`
- `tools/call` with `get_agent_context`
