# Plugin Protocol

Rainy supports three plugin shapes:

- external `rainy-*` commands discovered on `PATH` and Rainy plugin bins
- action plugins that return a Rainy `ChangeSet`
- HTTP adapters that return `rainy.plugin-rpc.v1` responses

Plugins cannot shadow built-in Rainy commands, cannot bypass policy, and must
declare permissions in `plugin.json`.

## Manifest

```json
{
  "protocolVersion": "rainy.plugin.v1",
  "name": "example",
  "version": "0.1.0",
  "description": "Example Rainy plugin",
  "commands": [
    {
      "name": "example",
      "description": "Run the external rainy-example command"
    }
  ],
  "actions": [
    {
      "id": "write-example",
      "description": "Write an example file"
    }
  ],
  "permissions": {
    "fs": {
      "read": ["rainy.yaml", "capability.lock"],
      "write": ["generated/**"]
    },
    "network": "none",
    "secrets": []
  }
}
```

Install:

```bash
rainy plugin install path/to/plugin --apply
rainy plugin list
rainy plugin inspect example
```

## External Commands

An installed plugin can expose `rainy-example`. Rainy forwards unknown top-level
commands to matching external commands:

```bash
rainy example hello
```

The command above forwards to:

```bash
rainy-example hello
```

## HTTP Action Adapter

For an HTTP adapter, `adapter` declares the URL:

```json
{
  "adapter": {
    "type": "http",
    "url": "http://127.0.0.1:8080/rainy"
  }
}
```

The adapter returns a `ChangeSet` and Rainy performs policy and apply:

```json
{
  "protocolVersion": "rainy.plugin-rpc.v1",
  "changeSet": {
    "changes": [
      {
        "kind": "create-file",
        "path": "generated/plugin.txt",
        "before": null,
        "after": "hello\n",
        "summary": "write plugin output",
        "noop": false
      }
    ]
  }
}
```

Run dry-run first:

```bash
rainy plugin call example write-example --dry-run
rainy plugin call example write-example --apply
```

## Wasm Action Plugins

Wasm action plugins are installed from manifest-declared action files. A module
can export `rainy_action(ptr, len) -> i64` plus `rainy_alloc`, or a zero-arg
`rainy_action() -> i64`. The returned packed offset/length points at a UTF-8
JSON `rainy.plugin-rpc.v1` response in guest memory.

Rainy validates returned paths against manifest `permissions.fs.write` and then
runs the normal policy gate before apply.

## Validation

```bash
rainy schema validate --schema plugin-manifest --file plugin.json
rainy conformance check --path path/to/plugins --json
```
