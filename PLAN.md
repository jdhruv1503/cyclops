# Cyclops — Coding Harness Plan

> A standalone, opinionated, very fast Rust coding-agent harness. LiteLLM-proxied LLM calls only. First-class in-harness memory subsystem. Highly opinionated — every prompt, tool, and knob is hardcoded in source; the meta-agent improves Cyclops by editing source and merging PRs.

---

## Context

Cyclops is a single-binary coding agent. Its job: take a task description and a worktree, run a multi-turn tool-using agent loop against an LLM (via a LiteLLM proxy), and either complete the task or hand back partial progress with a clean status. Speed is the sacrosanct constraint; the secondary constraint is that a meta-agent will rewrite Cyclops's source over time based on observed performance, so the design must minimize incidental complexity that obstructs source edits.

**Research findings that shaped the design (don't re-derive these):**
- **WebSocket is not a win for LiteLLM-proxied chat completions.** LiteLLM's `/v1/realtime` is voice-only; OpenAI's stateful WebSocket Responses API isn't proxyable through LiteLLM's standard path. Model TTFB (~450ms) dominates total latency; LiteLLM's own overhead is 2–12ms. HTTP/1.1 + SSE with tuned keepalive + connection pooling is the optimal transport.
- **Real speed wins, ranked:** prompt caching (Anthropic 90% input-token discount + latency), parallel tool dispatch, speculative tool dispatch from streamed `input_json_delta`, persistent shell, file-content cache with mtime invalidation, PATH-shadowing ripgrep/fd/bat.
- **Memory architectures in 2026 coding agents:** the boring approach wins. Pattern-based facts in SQLite + mtime file cache + sliding-window compaction with cache-aware fork-agent summarization (Claude Code pattern) outperforms knowledge graphs, LLM-based extraction (mem0), and runtime memory-manager agents (MemGPT/Letta) at coding-agent workloads. Cross-session vector recall is the weakest link in every shipping system at production scale — defer.

**Target outcome:** A `cyclops` binary that runs an agent loop at the speed floor for LLM-bound coding work, with an embedded memory subsystem worth A/B-ing against alternatives, and a source layout that a meta-agent can mutate quickly.

---

## Locked-in design decisions

| Decision | Choice |
|---|---|
| Language | **Rust** (Tokio async runtime, single static binary) |
| LLM transport | LiteLLM proxy only; HTTP/1.1 + SSE; custom tuned `hyper::Client` with keepalive pool, no compression (SSE), 5s dial, 30s response-header timeout |
| API shape | **OpenAI `/v1/chat/completions` streaming** (works for every model via LiteLLM's normalization); `cache_control` blocks on system + tools pass through to Anthropic backends |
| Prompts & tools | **Hardcoded in source** — `const &str` for prompts, Rust functions implementing a `Tool` trait. No YAML, no templates, no runtime config loading. The meta-agent edits `src/prompts.rs` and `src/tools/*.rs` |
| Tool dispatch | Parallel by default (Tokio `JoinSet`), speculative for idempotent tools (Read/Glob/Grep) based on incrementally-parsed `tool_calls[i].function.arguments` deltas |
| Tools v0 | **Read, Edit, Bash, Grep** (v1 adds Write, Glob, plus the memory tools below) |
| Edit form | `(path, old_string, new_string)` with uniqueness check (errors if old_string appears 0 or ≥2 times); matches str_replace_editor convention models are tuned on |
| Bash | Persistent `bash` subprocess per invocation, sentinel-framed (random nonce per command), Tokio I/O; `bash.reset` tool to respawn on wedge |
| Memory | **First-class in-harness subsystem**, embedded SQLite (`rusqlite` bundled) for facts, in-memory `HashMap` for file cache, sliding window + fork-agent compaction for turn history. Behind a `Memory` trait so alternatives can be A/B'd |
| Prompt caching | `cache_control: {type: ephemeral}` markers on system prompt and tool definitions every request; compaction calls byte-identical-prefix the parent to keep cache hot |
| Completion protocol | `<promise>COMPLETE</promise>` substring in assistant text **and** zero `tool_calls` that turn |
| Cancellation | Cooperative — checks at (a) before each tool call, (b) between LLM turns, (c) every K streamed tokens; on SIGINT/cancel: bash session SIGINT, in-flight tools ctx-cancelled, emit `task_end(status=preempted)`, exit clean |
| Output | JSONL events on stdout (one event per line, machine-parseable). Human diagnostics on stderr via `tracing` |
| TUI | **Separate workspace member** `cyclops-tui` (ratatui) that execs `cyclops` and subscribes to stdout JSONL. Built in v1; agent core is identical with or without the TUI in front |

---

## Repository layout (Cargo workspace)

```
cyclops/
├── Cargo.toml                              # workspace
├── README.md
├── crates/
│   ├── cyclops/                            # the agent binary + library
│   │   ├── Cargo.toml                      # [lib] + [[bin]] name="cyclops"
│   │   └── src/
│   │       ├── main.rs                     # CLI parse → lib::run
│   │       ├── lib.rs                      # pub fn run(Config) -> Result<Outcome>
│   │       ├── config.rs                   # Config struct, env+CLI resolution
│   │       ├── prompts.rs                  # const SYSTEM_PROMPT: &str = "...";
│   │       ├── version.rs                  # binary version + prompts-hash + tools-hash
│   │       ├── agent/
│   │       │   ├── mod.rs                  # AgentLoop struct
│   │       │   ├── turn.rs                 # one-turn driver
│   │       │   ├── completion.rs           # streaming COMPLETE detector
│   │       │   └── cancel.rs               # cooperative cancel write-out
│   │       ├── model/
│   │       │   ├── mod.rs                  # Model trait, Stream trait
│   │       │   ├── litellm.rs              # impl Model using hyper/reqwest
│   │       │   ├── stream.rs               # SSE chunk → typed StreamEvent
│   │       │   ├── cache.rs                # cache_control annotation helpers
│   │       │   ├── transport.rs            # tuned hyper::Client + pre-warm
│   │       │   └── messages.rs             # types: ChatMessage, ToolDef, etc.
│   │       ├── tools/
│   │       │   ├── mod.rs                  # Tool trait, Registry (static)
│   │       │   ├── dispatch.rs             # JoinSet-based parallel + speculative
│   │       │   ├── specparse.rs            # incremental JSON parse for spec dispatch
│   │       │   ├── read.rs                 # Read tool (uses memory::FileCache)
│   │       │   ├── edit.rs                 # Edit (uniqueness-checked str replace)
│   │       │   ├── write.rs                # Write (v1)
│   │       │   ├── bash.rs                 # Bash (uses shell::Session)
│   │       │   ├── glob.rs                 # Glob (v1, ignore crate)
│   │       │   ├── grep.rs                 # Grep (shells to rg, falls back to grep crate)
│   │       │   └── remember.rs             # remember(key, value, confidence) → FactStore
│   │       ├── shell/
│   │       │   └── session.rs              # persistent bash via tokio::process
│   │       ├── memory/
│   │       │   ├── mod.rs                  # Memory trait + HybridMemory (v1 impl)
│   │       │   ├── turn_buffer.rs          # VecDeque<Turn> + compaction trigger
│   │       │   ├── file_cache.rs           # HashMap<PathBuf, CachedFile> + LRU
│   │       │   ├── fact_store.rs           # SQLite-backed facts (rusqlite)
│   │       │   ├── compaction.rs           # fork-agent compaction call
│   │       │   └── session_index.rs        # v2 stub: trait + NoOp impl
│   │       ├── events/
│   │       │   ├── mod.rs                  # Event enum
│   │       │   ├── emitter.rs              # async JSONL writer (mpsc → stdout)
│   │       │   └── clock.rs                # monotonic ns + RFC3339Nano
│   │       └── fs/
│   │           └── worktree.rs             # rooted-path safety helpers
│   └── cyclops-tui/                        # ratatui subscriber (v1)
│       ├── Cargo.toml
│       └── src/main.rs                     # spawns `cyclops`, parses JSONL, renders
└── tests/
    └── golden/                             # JSONL fixtures for contract tests
```

Everything in `cyclops/src` is mutable by the meta-agent. There is no `internal/` vs `pkg/` split because there is no external SDK consumer that needs surface stability — this is one binary the user invokes, and a meta-agent that rewrites the source.

---

## Event schema (JSONL on stdout)

Every line is one `Event`. All events share `type`, `ts_ns` (monotonic since process start), `ts_wall` (RFC3339Nano), `seq` (strictly-increasing), and `turn` when applicable.

| `type` | Key fields |
|---|---|
| `task_start` | `task_id`, `model`, `max_turns`, `worktree`, `cyclops_version`, `prompts_hash`, `tools_hash` |
| `task_end` | `status` ∈ {complete, max_turns, error, preempted, cancelled}, `reason`, `turns`, `tokens_in`, `tokens_out`, `cache_read`, `cache_write`, `wallclock_ms` |
| `turn_start` | `turn`, `messages_in_context`, `prompt_tokens_estimate` |
| `turn_end` | `turn`, `stop_reason`, `tokens_in`, `tokens_out`, `cache_read`, `cache_write`, `duration_ms` |
| `llm_request` | `turn`, `model`, `n_messages`, `n_tools`, `cache_breakpoints` |
| `llm_first_token` | `turn`, `ttft_ms` |
| `text_delta` | `turn`, `text` |
| `thinking_delta` | `turn`, `text` (when reasoning models surface it) |
| `tool_use_start` | `turn`, `tool_id`, `name`, `index` |
| `tool_input_delta` | `turn`, `tool_id`, `partial_args` (raw argument chunk) |
| `tool_dispatch` | `turn`, `tool_id`, `name`, `input` (parsed), `mode` ∈ {speculative, final} |
| `tool_dispatch_cancel` | `turn`, `tool_id`, `reason` (speculative invalidated by final-bytes divergence) |
| `tool_result` | `turn`, `tool_id`, `name`, `ok`, `output` (truncated to 16KB; full body in `~/.cyclops/sessions/<id>/tools/<tool_id>.out`), `output_bytes`, `duration_ms`, `error?` |
| `assistant_message` | `turn`, `text`, `tool_uses` (ids only) — coalesced view |
| `completion_signal` | `turn`, `kind: promise_complete` |
| `memory_compaction` | `turn_range`, `tokens_before`, `tokens_after`, `duration_ms`, `summary_len` |
| `memory_fact_write` | `key`, `value`, `confidence`, `source` ∈ {agent_tool, pattern_extract, session_end_distill} |
| `memory_file_cache_stats` | `hits`, `misses`, `evictions`, `bytes_cached` (emitted at `task_end`) |
| `cancel` | `source` ∈ {signal, deadline, parent}, `at` |
| `error` | `where`, `class`, `message`, `retryable` |

A small Rust test in `tests/golden/` feeds a recorded SSE transcript through `model::stream` and asserts the resulting JSONL matches a fixture.

---

## Core types (Rust sketches)

```rust
// Tool: a hardcoded coding action. Each tool is a struct implementing this trait;
// the registry is a `const`-built `&[&dyn Tool]` slice.
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn input_schema(&self) -> &'static serde_json::Value;  // pre-baked JSON Schema
    fn speculatable(&self) -> bool { false }               // dispatch on partial input?
    async fn execute(&self, ctx: &ToolCtx, input: serde_json::Value) -> Result<ToolResult>;
}

pub struct ToolCtx<'a> {
    pub worktree: &'a Path,
    pub memory: &'a dyn Memory,
    pub shell: &'a Mutex<ShellSession>,
    pub emit: &'a EventEmitter,
    pub turn: u32,
    pub cancel: CancellationToken,
}

// Model: thin abstraction over the LiteLLM HTTP+SSE client.
#[async_trait]
pub trait Model: Send + Sync {
    async fn stream(&self, req: ChatRequest) -> Result<Box<dyn Stream>>;
    async fn prewarm(&self) -> Result<()>;
}

pub trait Stream: Send {
    fn next(&mut self) -> impl Future<Output = Result<Option<StreamEvent>>> + Send;
    fn usage(&self) -> Option<Usage>;
}

// Memory: the swappable subsystem.
#[async_trait]
pub trait Memory: Send + Sync {
    async fn append_turn(&mut self, turn: Turn);
    async fn snapshot_context(&self) -> ContextSnapshot;      // facts + compacted history
    async fn maybe_compact(&mut self, model: &dyn Model) -> Result<Option<CompactionReport>>;
    async fn recall_file(&self, path: &Path) -> Result<Option<CachedFile>>;
    async fn record_file(&self, path: &Path, content: &str);
    async fn note_fact(&self, fact: Fact);                    // from remember() tool
    async fn distill_facts(&mut self, model: &dyn Model) -> Result<usize>;  // session-end
}

pub struct EventEmitter {
    tx: mpsc::UnboundedSender<Event>,                          // writer task drains
}

pub struct ShellSession {
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    cwd: PathBuf,
}
impl ShellSession {
    pub async fn exec(&mut self, cmd: &str, timeout: Duration) -> Result<ExecResult>;
    pub async fn reset(&mut self) -> Result<()>;
}

pub struct AgentLoop {
    model: Arc<dyn Model>,
    tools: &'static [&'static dyn Tool],
    memory: Box<dyn Memory>,
    shell: Mutex<ShellSession>,
    emit: EventEmitter,
    cfg: Config,
}
impl AgentLoop {
    pub async fn run(self, task: String, cancel: CancellationToken) -> Result<Outcome>;
}
```

`tools::REGISTRY: &[&'static dyn Tool] = &[&Read, &Edit, &Bash, &Grep, &Remember];` — assembled at compile time. Adding a tool = one file in `src/tools/` + one entry in this slice.

---

## Main loop (pseudocode)

```
run(cfg, task, cancel):
  emit task_start{cyclops_version, prompts_hash, tools_hash, model, ...}
  defer emit task_end{status, ...}

  memory  := HybridMemory::open(cfg.data_dir, cfg.worktree).await
  shell   := ShellSession::spawn(cfg.worktree).await
  model   := LiteLlmClient::new(cfg.litellm_url, cfg.litellm_key)
  emit_h  := EventEmitter::new()

  model.prewarm().await    // dial + TLS handshake before turn 1

  // Build initial messages: system prompt with cache_control breakpoints +
  // injected facts (from memory.snapshot_context()) + user task
  system   := annotate_cache(PROMPTS::SYSTEM, TOOL_MANIFEST)
  ctx_snap := memory.snapshot_context().await
  msgs     := [user(task)]

  for turn in 1..=cfg.max_turns:
    if cancel.is_cancelled() { return preempt(memory).await }
    emit turn_start

    // Compaction decision: if estimated tokens > 70% of window, compact before send
    if estimate_tokens(&system, &ctx_snap, &msgs) > cfg.compact_threshold:
      memory.maybe_compact(&model).await    // emits memory_compaction
      ctx_snap := memory.snapshot_context().await

    req := ChatRequest {
      model: cfg.model,
      system, tools: TOOL_MANIFEST,
      messages: ctx_snap.compacted ++ msgs,
      stream: true,
    }
    emit llm_request

    let mut stream = model.stream(req).await?
    let mut text = String::new()
    let mut tool_calls: Vec<ToolCallAccum> = vec![]
    let mut dispatcher = ToolDispatcher::new(&tools, &ctx, turn)

    while let Some(ev) = stream.next().await? {
      match ev {
        FirstToken            => emit llm_first_token{ttft_ms}
        TextDelta(t)          => text.push_str(&t); emit text_delta{t}
                                 if completion::detect_streaming(&text) { /* note flag */ }
        ToolCallStart{i, id, name}
                              => tool_calls.push({i, id, name, args: ""}); emit tool_use_start
        ToolCallArgsDelta{i, partial}
                              => tool_calls[i].args.push_str(&partial)
                                 emit tool_input_delta
                                 dispatcher.on_delta(turn, i, &tool_calls[i]).await  // speculative
        ToolCallEnd{i}        => dispatcher.finalize(turn, i, &tool_calls[i]).await
        MessageEnd{stop}      => break
      }
      cancel.check()?
    }

    emit assistant_message
    let outcomes = dispatcher.await_all().await    // parallel
    let mut tool_msgs = vec![]
    for (tc, outcome) in tool_calls.iter().zip(outcomes) {
      emit tool_result{...}
      tool_msgs.push(tool_msg(tc.id, outcome))
    }

    memory.append_turn(Turn::assistant(text.clone(), &tool_calls)).await
    memory.append_turn(Turn::tool_results(&tool_msgs)).await
    msgs.extend(assistant_msg(text, &tool_calls))
    msgs.extend(tool_msgs)

    emit turn_end

    let completion_signal = completion::detect_final(&text) && tool_calls.is_empty()
    if completion_signal {
      emit completion_signal
      memory.distill_facts(&model).await     // session-end fact extraction
      return ok(complete)
    }
    if cancel.is_cancelled() { return preempt(memory).await }

  return ok(max_turns)
```

`completion::detect_streaming` runs over a sliding-window suffix of `text` to short-circuit unhelpful streaming; `detect_final` is an exact-substring match on the full accumulated text. Both require `tool_calls.is_empty()` for the turn to actually end the loop — the model can say "COMPLETE" mid-tool-call and we ignore it.

---

## Memory subsystem (v1)

The v1 implementation is `HybridMemory`, behind the `Memory` trait so alternatives can be benchmarked side-by-side later.

### Four scopes, one struct

```rust
pub struct HybridMemory {
    // (1) within-session: rolling turn buffer
    turn_buffer: VecDeque<Turn>,
    compacted: Vec<CompactedTurn>,
    // (2) within-session: file cache
    file_cache: HashMap<PathBuf, CachedFile>,
    file_cache_bytes: usize,
    // (3) cross-session: SQLite-backed facts
    facts: rusqlite::Connection,                 // ~/.cyclops/facts.db
    repo_id: String,                             // hash of worktree git origin or path
    // (4) cross-session: vector recall — v1 = NoOp, v2 = LanceDB
    session_index: Box<dyn SessionIndex>,
}
```

### 1. Within-session turn-history compaction

- Sliding window: keep the last 20 turns verbatim.
- Trigger: when estimated prompt tokens > 70% of model's context window, run compaction **before** the next request.
- Compaction is a **forked LLM call** with byte-identical system prompt + tools as the parent → the parent's `cache_control` prefix hits, so compaction costs only the new summary tokens. This is the only viable way to compact cheaply.
- Output: a single `CompactedTurn { range, summary, preserved: [file_paths, tool_results_ids] }` that replaces the original turn range.
- Why this beats Letta-style: no tool calls in the main loop, no per-turn memory-manager overhead.

### 2. File-content cache (mtime + content hash)

- `HashMap<PathBuf, CachedFile { content, mtime, hash, last_read, bytes }>`.
- `Read` tool flow: stat → if cached mtime matches, return cached bytes; else read + hash + insert.
- `Edit` / `Write` flow: punch the cache for the affected path so the next read re-stats.
- LRU eviction at 512MB cap, by `last_read`.
- Token-side benefit: agent doesn't re-Read files mid-session even if it forgets it has them; cache hit means we serve the cached content without round-tripping to the FS, and the model sees consistent content across turns.

### 3. Cross-session facts (SQLite)

```sql
CREATE TABLE IF NOT EXISTS facts (
  id TEXT PRIMARY KEY,         -- hash(repo_id, key, value)
  repo_id TEXT,                -- NULL = user-global
  key TEXT NOT NULL,           -- "build_tool", "test_runner", "lint_command", "prefers_tabs", ...
  value TEXT NOT NULL,
  confidence REAL NOT NULL,    -- 0.0..1.0
  source TEXT NOT NULL,        -- "agent_tool" | "pattern_extract" | "session_end_distill"
  source_turn INTEGER,
  created_at INTEGER NOT NULL, -- unix epoch ns
  updated_at INTEGER NOT NULL,
  metadata TEXT                -- JSON blob, free-form
);
CREATE INDEX IF NOT EXISTS idx_facts_repo_key ON facts(repo_id, key);
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA cache_size = -64000;
```

Three write paths:
1. **`remember(key, value, confidence)` tool** — the agent records facts deliberately. This is the most reliable channel.
2. **Pattern extraction** during tool execution — e.g., a Read of `pyproject.toml` triggers a pattern detector that notes `build_tool = "poetry"` at confidence 0.95. Hardcoded regexes for ~20 common patterns.
3. **Session-end distillation** — at `task_end{complete}`, one cheap LLM call (Cerebras / Groq / whatever the user routes through LiteLLM as a fast model) reads the session summary and emits structured fact candidates. Confidence ≤ 0.7.

Read path: at every `task_start`, query `SELECT key, value FROM facts WHERE (repo_id=? OR repo_id IS NULL) AND confidence >= 0.7 ORDER BY confidence DESC LIMIT 12`, render into a `## Known facts about this repo` block injected into the system prompt **inside the cached prefix** (so the cache hit covers it).

### 4. Cross-session vector recall (deferred to v2)

`trait SessionIndex` with a `NoOpSessionIndex` impl in v1. v2 plugs in LanceDB embedded; embedding either via LiteLLM (cheap, network-bound) or a local ONNX model (binary-size cost). Punted because the literature shows vector recall is the weakest link in every shipping memory system at production scale; the boring stuff above pays first.

### Prompt-cache layout

Every request lays out the prompt this way, with `cache_control: {type: ephemeral}` markers as shown:

```
[system block 1: STATIC SYSTEM PROMPT + TOOL MANIFEST]         ← cache_control
[system block 2: KNOWN FACTS for this repo (top 12)]           ← cache_control
[assistant: compacted history (one big CompactedTurn block)]    ← cache_control
[user/assistant: last N live turns]
[user: new message]
```

Compaction calls reuse blocks 1+2 byte-identical with the parent → cache hit on the prefix, only the summary tokens are uncached.

---

## Tools (v0)

| Tool | Input | Speculatable | Notes |
|---|---|---|---|
| `Read` | `{path: string, offset?: usize, limit?: usize}` | yes | Memory file-cache aware; emits `memory_file_cache_stats` deltas |
| `Edit` | `{path: string, old_string: string, new_string: string}` | no | Errors if old_string appears 0 or >1 times; punches file cache |
| `Bash` | `{command: string, timeout_ms?: u64}` | no | Persistent shell session; nonce-framed sentinels; default 120s timeout |
| `Grep` | `{pattern: string, path?: string, case_insensitive?: bool}` | yes | Shells to `rg` if present; falls back to `grep` crate |
| `Remember` | `{key: string, value: string, confidence?: f32}` | no | Writes to `FactStore` with source=`agent_tool` |

v1 adds `Write`, `Glob`, and a `BashReset` to recover wedged shells.

`SYSTEM_PROMPT` (in `src/prompts.rs`) hardcodes a brief AGENTS hint preferring `rg`/`fd`/`bat` over `grep`/`find`/`cat`, instructions for the completion protocol, and the rule that the agent should call `Remember` whenever it learns something stable about the repo.

---

## Configuration (CLI flags only — no config files)

```
cyclops [TASK]
  --task-file PATH                 # alternative to inline TASK
  --worktree PATH                  # required; rooted-path safety enforced
  --model NAME                     # e.g. claude-sonnet-4-7
  --max-turns N                    # default 50
  --litellm-url URL                # env: CYCLOPS_LITELLM_URL
  --litellm-key KEY                # env: CYCLOPS_LITELLM_KEY
  --data-dir PATH                  # default ~/.cyclops; facts.db lives here
  --deadline 30m                   # wall-clock budget; sends ctx-cancel
  --no-speculative                 # disable speculative tool dispatch
  --no-compaction                  # disable memory compaction (testing only)
  --emit-thinking                  # include thinking_delta events
```

Environment vars are read by `clap` as defaults. No YAML, no TOML, no Hocon. Configuration that's "settable" is exactly what's needed at the command line; everything else is a `const` somewhere in `src/`.

---

## Phasing

### v0 — walking skeleton (one weekend)

- Cargo workspace bootstrapped with the layout above.
- `LiteLlmClient` with tuned hyper transport, SSE parsing, prompt-cache markers, pre-warm.
- `AgentLoop::run` with full event schema emitting on stdout.
- Tools: **Read, Edit, Bash, Grep** (sequential dispatch, no speculation in v0).
- Persistent bash with sentinel framing and timeout.
- Completion protocol detection.
- `HybridMemory` partial: turn buffer (no compaction yet), file cache, fact store with `Remember` tool. Pattern extraction stubbed; session-end distillation stubbed.
- Cooperative cancellation at tool boundaries.
- Golden-fixture contract test.
- **Acceptance:** `cyclops "fix the typo in foo.py" --worktree /tmp/wt --model claude-sonnet-4-7 --litellm-url http://localhost:4000` runs end-to-end, emits clean JSONL, exits.

### v1 — the speed pass + memory subsystem proper

- **Parallel tool dispatch** via Tokio `JoinSet`.
- **Speculative tool dispatch**: incremental JSON parse on `tool_input_delta`; per-tool `speculatable()` flag (Read/Glob/Grep yes, Edit/Bash/Write no); reconcile-and-cancel on final-bytes divergence (`tool_dispatch_cancel`).
- **Compaction**: fork-agent compaction with byte-identical parent prefix; trigger at 70% of context window.
- **Pattern extraction**: hardcoded regexes for ~20 common repo facts (`pyproject.toml`, `package.json`, `Cargo.toml`, `.tool-versions`, `Makefile`, lint configs, etc.).
- **Session-end fact distillation**: one LLM call at `task_end{complete}` summarizing the session into fact candidates.
- Tools: add **Write, Glob, BashReset**.
- Streaming completion detector (short-circuit on COMPLETE seen mid-stream).
- Cooperative cancellation: token-frequency checks in stream consumer.
- `cyclops-tui` workspace member: ratatui subscriber.
- **Acceptance:** on a real 30+ turn task, parallel tool turns visible in events; cache_read non-zero from turn 2; latency floor is model TTFB.

### v2 — meta-agent-friendly + memory experiments

- `SessionIndex` with LanceDB-backed implementation; embedding via LiteLLM (configurable model).
- Multiple `Memory` impls behind the trait: `HybridMemory` (v1 default), `RollingSummaryMemory` (pure rolling summary, no facts), `LlmExtractMemory` (mem0-style). Run-time selection via `--memory-impl` flag for benchmarks.
- Per-prompt and per-tool content hashes in `task_start` for fine-grained A/B by the meta-agent.
- `cyclops eval <tasks-dir>` subcommand: runs N tasks against an oracle, writes results JSON suitable for the meta-agent's eval loop.
- Optional: io_uring filesystem path (`tokio-uring`) for the file cache hot path on Linux.

---

## Tradeoffs flagged

1. **Speculative dispatch correctness.** Risk: partial `{"path":"/etc/passwd"` parses but the model meant `/etc/passwd.bak`. Mitigations: speculate only when (a) the parser sees a closing `}` AND (b) all required schema keys are present AND (c) the tool declares `speculatable()`. Always reconcile against final bytes; on divergence emit `tool_dispatch_cancel` and never feed the speculative result to the model. `--no-speculative` flag as escape hatch.
2. **Persistent bash safety.** Failure modes: cwd drift, env leakage, backgrounded stdout swallowing the sentinel, `exec` replacing the shell. Mitigations: per-command random nonce in the sentinel; commands wrapped `{ <cmd>; } 2>&1; printf '\x1eEXIT:%s:CWD:%s\x1e\n' $? "$PWD"`; Tokio-side timeout (SIGINT then SIGKILL); wedge detector that respawns; explicit `BashReset` tool.
3. **Compaction cache-coherence is fragile.** Any drift in system-prompt bytes between the live request and the compaction call breaks the Anthropic prefix cache. Hard rule: the compaction call constructs its request through the exact same `build_system_blocks()` codepath as the main loop, and the unit test for compaction asserts byte-equality of the system blocks.
4. **Fact extraction false positives.** Pattern extraction can mis-fire (e.g., a vendored `package.json` claiming the repo is a Node project when it's a Python project with a frontend). Mitigations: confidence ≤ 0.85 from patterns; agent's explicit `Remember` calls override (confidence 1.0); a fact that contradicts an existing high-confidence fact for the same key is logged but not auto-merged.
5. **Edit form.** `(path, old_string, new_string)` with uniqueness check chosen over unified-diff. Diff is more compact but parse-failure rates on non-frontier models are materially worse, and the uniqueness check forces the model to provide enough context to disambiguate, which is itself a quality signal.
6. **HTTP client.** `hyper::Client` directly rather than `reqwest`. Reasons: explicit control of connection pool, SSE-friendly read path without higher-level abstractions in the way, no header compression negotiation that can hide chunk boundaries. Cost: more code. Worth it for a binary where transport latency is the whole game.

---

## Out of scope (Cyclops explicitly does not do)

- Outer test-driven retry loop. Cyclops runs the inner agent loop; the caller decides whether to re-invoke after running tests.
- Test/typecheck/lint execution. The agent can run them via Bash if it wants; Cyclops doesn't have an opinion on a test command.
- Worktree creation, git operations, PR opening. The caller hands in `--worktree`; if the agent needs git, it uses Bash.
- Human approval / dangerous-command gating. The worktree is the sandbox.
- Model routing / fallback. The caller picks `--model`; LiteLLM handles provider fallback chains.
- Trace ingestion / rating. LiteLLM emits Langfuse traces if the caller's proxy is configured that way; Cyclops's job is JSONL stdout.
- Scheduling, retries, cron. The caller orchestrates.
- Multi-task batching. One invocation = one task.

---

## Critical files to create (in order)

1. `crates/cyclops/Cargo.toml` — dependencies pinned (tokio, hyper, rustls, serde, serde_json, clap, rusqlite-bundled, tracing, async-trait, anyhow, thiserror, bytes, tokio-util, futures, globset, ignore)
2. `crates/cyclops/src/events/mod.rs` + `emitter.rs` — the contract; everything else depends on these
3. `crates/cyclops/src/model/transport.rs` + `litellm.rs` + `stream.rs` + `cache.rs` — LLM I/O
4. `crates/cyclops/src/shell/session.rs` — persistent bash
5. `crates/cyclops/src/memory/mod.rs` + `turn_buffer.rs` + `file_cache.rs` + `fact_store.rs` — the memory trait and the v1 impl
6. `crates/cyclops/src/tools/mod.rs` + `read.rs` `edit.rs` `bash.rs` `grep.rs` `remember.rs` — tool registry assembled at compile time
7. `crates/cyclops/src/tools/dispatch.rs` + `specparse.rs` — v1 only; v0 does sequential dispatch
8. `crates/cyclops/src/agent/mod.rs` + `turn.rs` + `completion.rs` — the loop
9. `crates/cyclops/src/prompts.rs` — `const SYSTEM_PROMPT: &str`
10. `crates/cyclops/src/main.rs` + `lib.rs` + `config.rs` — entry point
11. `tests/golden/basic_session.jsonl` + the test harness

---

## Verification (end-to-end acceptance)

**v0 acceptance:**
1. `cargo build --release && ./target/release/cyclops --help` prints flags.
2. Local LiteLLM proxy up with one Anthropic model.
3. `mkdir /tmp/wt && cd /tmp/wt && echo 'print("buggy")' > main.py`.
4. `cyclops "make main.py print 'fixed' instead" --worktree /tmp/wt --model claude-sonnet-4-7 --max-turns 5 --litellm-url http://localhost:4000` exits 0, emits `task_start`, ~2 turns of `text_delta` + `tool_use_start` + `tool_dispatch(mode=final)` + `tool_result`, then `task_end{status=complete}`. `main.py` now prints `fixed`.
5. Golden-fixture test: a recorded SSE transcript fed through `model::stream` produces JSONL byte-equal to `tests/golden/basic_session.jsonl`.

**v1 acceptance:**
1. Real 30+ turn task in a small Python repo (e.g., add type hints to a module). `cache_read` non-zero from `turn_end{turn:2}` onward.
2. A turn that emits two tool_calls (e.g., Read+Grep) shows two `tool_dispatch{mode=speculative}` events ordered by streaming arrival, followed by interleaved `tool_result`s; wall-clock of the turn ≈ model TTFB + max(tool durations), not sum.
3. Kill -INT mid-stream → `cancel` + `task_end{status=preempted}` within 2s, bash session torn down.
4. Memory compaction kicks in at the configured threshold; `memory_compaction` event with non-zero `summary_len`; the next request shows `cache_read` covering the new compacted block.
5. `Remember` tool call writes a row to `facts.db`; next session's `task_start` followed immediately by an injected facts block visible in the system prompt (verified by reading the proxy-side request log).

**v2 acceptance:**
1. `cyclops eval ./eval/tasks/` runs N tasks, writes a results JSON.
2. Swapping `--memory-impl rolling_summary` produces measurably different `tokens_in` numbers across a fixed task set, with results JSON ready for the meta-agent's A/B.
3. `prompts_hash` and `tools_hash` in `task_start` change deterministically with source edits.
