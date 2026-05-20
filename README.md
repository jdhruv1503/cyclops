# Cyclops

Cyclops is an early Rust coding-agent harness. The repository has a minimal Cargo workspace and `cyclops` CLI parser; agent behavior is still planned in `PLAN.md` and has not been implemented yet.

Start with:

- `PLAN.md` for architecture, roadmap, event schema, and acceptance gates.
- `AGENTS.md` for agent-specific repo guidance and current verification limits.

## Current Setup

Install a Rust toolchain with Cargo. This workspace was verified with Fedora packages `rust` and `cargo` version 1.95.0.

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

Verified commands:

```bash
cargo fmt --check
cargo build --release
cargo test
./target/release/cyclops --help
./target/release/cyclops "fix it" --worktree /tmp/wt --model accounts/fireworks/routers/kimi-k2p6-turbo
```

`./target/release/cyclops` currently parses CLI flags and exits 0 when required arguments are present. It does not run an agent loop yet.

## Repository Status

- Key design document: `PLAN.md`.
- Current workspace members: `crates/cyclops`.
- `Cargo.lock` is tracked for reproducible binary builds.
- Planned implementation language: Rust.
- Planned future workspace members: `crates/cyclops-web` and later `crates/cyclops-tui`.
- Planned runtime integration: LiteLLM-proxied streaming chat completions.

When more implementation lands, update this README with any new verified setup, build, test, lint, format, typecheck, and smoke-test commands.
