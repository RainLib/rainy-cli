#!/usr/bin/env python3
"""Minimal stdio JSON-RPC wrapper for Rainy CLI.

This example intentionally keeps all business logic in `rainy`. It exposes a
small MCP-compatible surface (`tools/list`, `tools/call`) and shells out to the
CLI with `--json`. Set RAINY_BIN to override the executable path.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from typing import Any


RAINY_BIN = os.environ.get("RAINY_BIN", "rainy")


TOOLS = [
    {
        "name": "list_capabilities",
        "description": "List available Rainy capabilities.",
        "inputSchema": {
            "type": "object",
            "properties": {"workspace": {"type": "string"}},
        },
    },
    {
        "name": "get_capability_detail",
        "description": "Explain one Rainy capability.",
        "inputSchema": {
            "type": "object",
            "required": ["capability"],
            "properties": {
                "workspace": {"type": "string"},
                "capability": {"type": "string"},
            },
        },
    },
    {
        "name": "plan_add_capability",
        "description": "Create a dry-run plan for adding a capability.",
        "inputSchema": {
            "type": "object",
            "required": ["capability"],
            "properties": {
                "workspace": {"type": "string"},
                "capability": {"type": "string"},
                "provider": {"type": "string"},
            },
        },
    },
    {
        "name": "apply_plan",
        "description": "Apply a previously reviewed Rainy plan file.",
        "inputSchema": {
            "type": "object",
            "required": ["plan"],
            "properties": {
                "workspace": {"type": "string"},
                "plan": {"type": "string"},
            },
        },
    },
    {
        "name": "run_doctor",
        "description": "Run Rainy doctor.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "workspace": {"type": "string"},
                "capability": {"type": "string"},
            },
        },
    },
    {
        "name": "run_verify",
        "description": "Run Rainy verify.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "workspace": {"type": "string"},
                "profile": {"type": "string"},
                "capability": {"type": "string"},
            },
        },
    },
    {
        "name": "generate_evidence",
        "description": "Generate Rainy evidence.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "workspace": {"type": "string"},
                "format": {"type": "string", "enum": ["markdown", "json", "all"]},
            },
        },
    },
    {
        "name": "get_agent_context",
        "description": "Return Rainy agent context.",
        "inputSchema": {
            "type": "object",
            "properties": {"workspace": {"type": "string"}},
        },
    },
]


def main() -> None:
    for line in sys.stdin:
        if not line.strip():
            continue
        try:
            request = json.loads(line)
            response = handle_request(request)
        except Exception as exc:  # MCP examples must not crash the host.
            response = {
                "jsonrpc": "2.0",
                "id": None,
                "error": {"code": -32000, "message": str(exc)},
            }
        print(json.dumps(response), flush=True)


def handle_request(request: dict[str, Any]) -> dict[str, Any]:
    method = request.get("method")
    request_id = request.get("id")
    if method == "initialize":
        result = {
            "protocolVersion": "2024-11-05",
            "serverInfo": {"name": "rainy-mcp", "version": rainy_version()},
            "capabilities": {"tools": {}},
        }
    elif method == "tools/list":
        result = {"tools": TOOLS}
    elif method == "tools/call":
        params = request.get("params") or {}
        result = call_tool(params.get("name"), params.get("arguments") or {})
    else:
        return {
            "jsonrpc": "2.0",
            "id": request_id,
            "error": {"code": -32601, "message": f"unknown method: {method}"},
        }
    return {"jsonrpc": "2.0", "id": request_id, "result": result}


def call_tool(name: str, arguments: dict[str, Any]) -> dict[str, Any]:
    workspace = arguments.get("workspace")
    if name == "list_capabilities":
        return rainy(workspace, ["capability", "list", "--json"])
    if name == "get_capability_detail":
        return rainy(workspace, ["capability", "explain", arguments["capability"], "--json"])
    if name == "plan_add_capability":
        args = ["add", "capability", arguments["capability"], "--dry-run", "--json"]
        if arguments.get("provider"):
            args.extend(["--provider", arguments["provider"]])
        return rainy(workspace, args)
    if name == "apply_plan":
        return rainy(workspace, ["apply", "--plan", arguments["plan"], "--json"])
    if name == "run_doctor":
        args = ["doctor", "--json"]
        if arguments.get("capability"):
            args.extend(["--capability", arguments["capability"]])
        return rainy(workspace, args)
    if name == "run_verify":
        args = ["verify", "--profile", arguments.get("profile", "local"), "--json"]
        if arguments.get("capability"):
            args.extend(["--capability", arguments["capability"]])
        return rainy(workspace, args)
    if name == "generate_evidence":
        args = ["evidence", "generate", "--format", arguments.get("format", "all"), "--json"]
        return rainy(workspace, args)
    if name == "get_agent_context":
        return rainy(workspace, ["agent", "context", "--json"])
    raise ValueError(f"unknown tool: {name}")


def rainy_version() -> str:
    output = subprocess.run(
        [RAINY_BIN, "--version"], text=True, capture_output=True, check=False
    )
    if output.returncode != 0:
        raise RuntimeError(output.stderr or output.stdout or "rainy --version failed")
    prefix = "rainy "
    version = output.stdout.strip()
    if not version.startswith(prefix):
        raise RuntimeError(f"unexpected rainy version output: {version}")
    return version.removeprefix(prefix)


def rainy(workspace: str | None, args: list[str]) -> dict[str, Any]:
    command = [RAINY_BIN]
    if workspace:
        command.extend(["--workspace", workspace])
    command.extend(args)
    output = subprocess.run(command, text=True, capture_output=True, check=False)
    if output.returncode != 0:
        return {
            "isError": True,
            "content": [{"type": "text", "text": output.stderr or output.stdout}],
        }
    payload = output.stdout.strip()
    try:
        parsed = json.loads(payload) if payload else {}
        text = json.dumps(parsed, indent=2)
    except json.JSONDecodeError:
        text = payload
    return {"content": [{"type": "text", "text": text}]}


if __name__ == "__main__":
    main()
