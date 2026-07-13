# Rainy Backstage Integration Example

This directory contains a minimal Backstage scaffolder integration shape:

- `templates/rainy-spring-nextjs-saas.yaml`
- `actions/rainy-actions.ts`

The actions shell out to Rainy CLI and keep orchestration logic in the CLI.
Register the exported actions in your Backstage backend and set `RAINY_BIN`
when the Rainy executable is not on `PATH`.

Pin `RAINY_BIN` to a tested Rainy release and run actions in an isolated
scaffolder workspace. The Backstage backend identity must not receive broader
filesystem or cluster credentials than the template requires. Rainy native
plugins remain disabled unless the deployment explicitly opts in.

Compatibility follows the Rainy CLI minor version. Template owners should run
`rainy verify --profile ci --json` in generated repositories before promoting a
new CLI version. The files here are integration source examples, not a published
npm package; consumers own Backstage packaging, authorization, and deployment.
