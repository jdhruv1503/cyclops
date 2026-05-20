# AGENTS.md

Guidance for Codex and other coding agents working in this repository.

## Current State

- This repository currently contains a minimal Rust Cargo workspace and a `cyclops` crate with CLI parsing, error types, and event schema serialization types.
- `PLAN.md` is the long-form design source of truth.
- There is no CI config, script directory, agent loop, event emitter, LiteLLM client, tool system, or integration test harness yet.
- `PLAN.md` is the source of truth for the intended Cyclops architecture, milestones, file layout, event schema, and acceptance gates.

## Project Summary

Cyclops is planned as a single-binary Rust coding-agent harness. The design in `PLAN.md` specifies:

- A Cargo workspace rooted at `Cargo.toml`.
- Main binary/library crate under `crates/cyclops`.
- Local web UI crate under `crates/cyclops-web`.
- Optional TUI crate under `crates/cyclops-tui`.
- LiteLLM-proxied `/v1/chat/completions` streaming over HTTP/1.1 + SSE.
- Hardcoded prompts and tool definitions in Rust source.
- JSONL event output and durable session logs.
- A phased roadmap from v0 working agent through v2 memory experiments.

Do not assume future crates or modules exist until they have actually been created.

## Before Editing

1. Inspect the current tree with `rg --files` and include hidden/config files when relevant with `find . -maxdepth 4 -type f -print`.
2. Check existing work with `git status --short`.
3. Read `PLAN.md` before changing architecture, commands, file layout, tool behavior, event schema, or acceptance criteria.
4. Preserve unrelated user edits. `PLAN.md` may be actively edited by the user.
5. Keep changes behavior-preserving unless the user explicitly asks for implementation work.

## Prime Directives

- Work autonomously through the granular tasks in `PLAN.md` until the user redirects or stops the work.
- Use one implementation subagent for each granular implementation step, then use a separate subagent to review that step before moving on.
- Keep commits granular and commit only when the tree is in a coherent, tested state.
- Update documentation in the same commit whenever setup, commands, conventions, architecture, or verification changes.
- Be test driven: add or update tests before or with implementation, run them, and do not handwave failures.
- Do not use mocks for acceptance of live integration behavior; use real services when credentials and local resources are available.
- Keep secrets out of git, logs, docs, and final responses. Local secrets may live in untracked `.env` files only.
- Prefer simple, fast systems. Remove dead code and simplify frequently.
- Include performance checks or benchmarks for behavior where speed is part of the design contract.
- Never assume commands or conventions. Derive them from checked-in files and verified tool output.

## Commands

The Rust workspace is present. Verified commands:

```bash
cargo fmt --check
cargo build --release
cargo test
./target/release/cyclops --help
./target/release/cyclops "fix it" --worktree /tmp/wt --model accounts/fireworks/routers/kimi-k2p6-turbo
```

The current T02 binary parses CLI flags and exits 0 after config parsing when required arguments are present. A no-arg invocation exits 2 with Clap usage because task, `--worktree`, and `--model` are required.

Use these discovery commands now:

```bash
rg --files
find . -maxdepth 4 -type f -print | sort
git status --short
```

Commands in `PLAN.md` for later tasks remain planned until the matching source files and behavior exist.

## Documentation Conventions

- Keep repo guidance short, practical, and derived from checked-in files.
- Separate current facts from planned behavior.
- Use `PLAN.md` for long-form design and roadmap detail.
- Use `README.md` for quick orientation and current setup status.
- Update this file when commands, generated files, or development conventions become real.

## Planned Implementation Conventions

These conventions are derived from `PLAN.md` and apply once the relevant files exist:

- Rust with Tokio is the implementation stack.
- Keep the crate minimal until later tasks add agent behavior.
- Prompts and tools are hardcoded in source, not loaded from YAML/TOML/templates.
- The agent core should remain usable without the web UI or future TUI.
- Provider-specific model features belong behind explicit capability checks.
- Web UI local server binds loopback only until an auth/CSRF/command-risk design exists.
- Generated session output belongs outside the repo under `~/.cyclops/sessions/<task_id>/`.

## Files To Avoid Editing

- Do not edit `.git/` contents.
- Do not edit `.claude/settings.local.json` unless the user explicitly asks to change local Claude permissions.
- Do not create or commit generated session logs, tool outputs, build artifacts, or local scratch files.
- Do track `Cargo.lock`; Cyclops is a binary workspace and lockfile changes are part of reproducible builds.
- Do not rewrite `PLAN.md` wholesale; make narrow edits only when the requested task requires it.

## Done Criteria For Future Tasks

A Codex task in this repo is done when:

- The change matches the current repo state and does not rely on non-existent files or commands.
- `PLAN.md`, `AGENTS.md`, and `README.md` remain consistent when the task affects architecture, setup, commands, or verification.
- Relevant verification was run, or the exact blocker is documented.
- Behavior changes include tests once a test harness exists.
- Generated files and local-only configuration are not committed.
- The final response lists files changed, commands run, results, blockers, and recommended follow-up.

## Known Unknowns

- No package manager or Rust toolchain version is pinned yet.
- No CI provider or workflow exists yet.
- No scripts, fixtures, or full generated-file policy exist yet.
- No agent loop, tools, event emitter, model transport, or session logs exist yet.
- Live LiteLLM model routes and required environment variables are not captured in repo config yet.
