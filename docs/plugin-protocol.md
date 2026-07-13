# Plugin Protocol

Rainy supports three plugin shapes:

- external `rainy-*` commands discovered on `PATH` and Rainy plugin bins
- action plugins that return a Rainy `ChangeSet`
- HTTP adapters that return `rainy.plugin-rpc.v1` responses

Plugins cannot shadow built-in Rainy commands, cannot bypass policy, and must
declare permissions in `plugin.json`.

Wasm is the default production runtime. External `rainy-*` executables run with
the invoking user's full host permissions and are not a sandbox. They are
blocked unless the caller explicitly passes `--allow-native-plugin` or sets
`RAINY_ALLOW_NATIVE_PLUGIN=1` after reviewing the executable.

A project may persist the decision with `policy.allowNativePlugins: true`.
Native execution is only permitted inside a Rainy project so every invocation
can be appended to `.rainy/audit.log`; an audit write failure fails the command.

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
rainy --allow-native-plugin plugin install path/to/plugin --apply
rainy plugin list
rainy plugin inspect example
```

## External Commands

An installed plugin can expose `rainy-example`. Rainy forwards unknown top-level
commands to matching external commands:

```bash
rainy --allow-native-plugin example hello
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
