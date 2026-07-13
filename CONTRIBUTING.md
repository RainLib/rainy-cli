# Contributing

Use Rust 1.88 or newer and run the repository quality gate before opening a PR:

```bash
make production-check
```

Keep changes focused, include tests for behavioral changes, and update public
schemas and documentation when command or JSON output contracts change. New
native plugin execution paths must remain opt-in. New network sources must use
HTTPS, with loopback HTTP reserved for local development and tests.

Release preparation follows `docs/releasing.md`. Commits merged to `main` must
pass CI and security checks.
