# Rainy MCP Wrapper

This example exposes Rainy CLI through a small stdio JSON-RPC wrapper. The
wrapper does not reimplement Rainy logic; every tool shells out to `rainy`
with `--json`.

The wrapper protocol version follows the Rainy CLI minor release. Pin the
wrapper and `RAINY_BIN` to the same release in production. Run it as an
unprivileged user in a workspace-specific directory; do not expose the stdio
transport directly over a network. Mutating methods still require Rainy's
explicit apply mode and policy checks.

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

Production hosts should enforce process timeouts, restrict the workspace path,
disable native plugins, and collect Rainy's audit log. The wrapper never grants
native plugin trust on behalf of a caller.
