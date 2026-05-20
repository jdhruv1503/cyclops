# Cyclops

Cyclops is currently a design-plan repository for a planned Rust coding-agent harness. There is no implemented application, Cargo workspace, CI, or test harness yet.

Start with:

- `PLAN.md` for architecture, roadmap, event schema, and acceptance gates.
- `AGENTS.md` for agent-specific repo guidance and current verification limits.

## Current Setup

No package setup is required yet because there is no source tree or package manifest.

Local Fireworks credentials can be provided with an untracked `.env` file:

```bash
cp .env.example .env
```

Fill in `FIREWORKS_API_KEY` locally. Do not commit `.env`.

Useful inspection commands:

```bash
rg --files
git status --short
```

## Build And Test

No build, test, lint, format, or typecheck commands are currently available.

`PLAN.md` documents future acceptance commands such as `cargo build --release`, `cargo test`, and `./target/release/cyclops --help`; treat them as planned until `Cargo.toml` and the relevant crates exist.

## Repository Status

- Current tracked project document: `PLAN.md`.
- Planned implementation language: Rust.
- Planned workspace members: `crates/cyclops`, `crates/cyclops-web`, and later `crates/cyclops-tui`.
- Planned runtime integration: LiteLLM-proxied streaming chat completions.

When implementation begins, update this README with verified setup, build, test, lint, format, typecheck, and smoke-test commands.
