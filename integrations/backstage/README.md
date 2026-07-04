# Rainy Backstage Integration Example

This directory contains a minimal Backstage scaffolder integration shape:

- `templates/rainy-spring-nextjs-saas.yaml`
- `actions/rainy-actions.ts`

The actions shell out to Rainy CLI and keep orchestration logic in the CLI.
Register the exported actions in your Backstage backend and set `RAINY_BIN`
when the Rainy executable is not on `PATH`.
