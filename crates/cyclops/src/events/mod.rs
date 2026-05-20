use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventMeta {
    pub ts_ns: u64,
    pub ts_wall: String,
    pub seq: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    TaskStart {
        #[serde(flatten)]
        meta: EventMeta,
        task_id: String,
        model: String,
        max_turns: u32,
        worktree: String,
        cyclops_version: String,
        prompts_hash: String,
        tools_hash: String,
    },
    TaskEnd {
        #[serde(flatten)]
        meta: EventMeta,
        status: TaskEndStatus,
        reason: String,
        turns: u32,
        tokens_in: u64,
        tokens_out: u64,
        cache_read: u64,
        cache_write: u64,
        wallclock_ms: u64,
    },
    TurnStart {
        #[serde(flatten)]
        meta: EventMeta,
        turn: u32,
        messages_in_context: u32,
        prompt_tokens_estimate: u64,
    },
    TurnEnd {
        #[serde(flatten)]
        meta: EventMeta,
        turn: u32,
        stop_reason: String,
        tokens_in: u64,
        tokens_out: u64,
        cache_read: u64,
        cache_write: u64,
        duration_ms: u64,
    },
    LlmRequest {
        #[serde(flatten)]
        meta: EventMeta,
        turn: u32,
        model: String,
        n_messages: u32,
        n_tools: u32,
        cache_breakpoints: u32,
    },
    LlmFirstToken {
        #[serde(flatten)]
        meta: EventMeta,
        turn: u32,
        ttft_ms: u64,
    },
    TextDelta {
        #[serde(flatten)]
        meta: EventMeta,
        turn: u32,
        text: String,
    },
    ThinkingDelta {
        #[serde(flatten)]
        meta: EventMeta,
        turn: u32,
        text: String,
    },
    ToolUseStart {
        #[serde(flatten)]
        meta: EventMeta,
        turn: u32,
        tool_id: String,
        name: String,
        index: u32,
    },
    ToolInputDelta {
        #[serde(flatten)]
        meta: EventMeta,
        turn: u32,
        tool_id: String,
        partial_args: String,
    },
    ToolDispatch {
        #[serde(flatten)]
        meta: EventMeta,
        turn: u32,
        tool_id: String,
        name: String,
        input: Value,
        mode: DispatchMode,
    },
    ToolDispatchCancel {
        #[serde(flatten)]
        meta: EventMeta,
        turn: u32,
        tool_id: String,
        reason: String,
    },
    ToolResult {
        #[serde(flatten)]
        meta: EventMeta,
        turn: u32,
        tool_id: String,
        name: String,
        ok: bool,
        output: String,
        output_bytes: u64,
        duration_ms: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    AssistantMessage {
        #[serde(flatten)]
        meta: EventMeta,
        turn: u32,
        text: String,
        tool_uses: Vec<String>,
    },
    CompletionSignal {
        #[serde(flatten)]
        meta: EventMeta,
        turn: u32,
        kind: CompletionSignalKind,
    },
    MemoryCompaction {
        #[serde(flatten)]
        meta: EventMeta,
        turn_range: TurnRange,
        tokens_before: u64,
        tokens_after: u64,
        duration_ms: u64,
        summary_len: u64,
    },
    MemoryFactWrite {
        #[serde(flatten)]
        meta: EventMeta,
        key: String,
        value: String,
        confidence: f64,
        source: MemoryFactSource,
    },
    MemoryFileCacheStats {
        #[serde(flatten)]
        meta: EventMeta,
        hits: u64,
        misses: u64,
        evictions: u64,
        bytes_cached: u64,
    },
    Cancel {
        #[serde(flatten)]
        meta: EventMeta,
        source: CancelSource,
        at: String,
    },
    Error {
        #[serde(flatten)]
        meta: EventMeta,
        #[serde(rename = "where")]
        where_: String,
        class: String,
        message: String,
        retryable: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskEndStatus {
    Complete,
    MaxTurns,
    Error,
    Preempted,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DispatchMode {
    Speculative,
    Final,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionSignalKind {
    PromiseComplete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryFactSource {
    AgentTool,
    PatternExtract,
    SessionEndDistill,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancelSource {
    Signal,
    Deadline,
    Parent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnRange {
    pub start: u32,
    pub end: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn meta(seq: u64) -> EventMeta {
        EventMeta {
            ts_ns: 1_000 + seq,
            ts_wall: "2026-05-21T10:20:30.123456789Z".to_string(),
            seq,
        }
    }

    fn assert_event_shape(event: Event, expected: Value) {
        let serialized = serde_json::to_value(&event).unwrap();
        assert_eq!(serialized, expected);

        let deserialized: Event = serde_json::from_value(serialized).unwrap();
        assert_eq!(deserialized, event);
    }

    #[test]
    fn task_start_serializes_with_common_fields_and_schema_name() {
        assert_event_shape(
            Event::TaskStart {
                meta: meta(1),
                task_id: "task-1".to_string(),
                model: "accounts/fireworks/routers/kimi-k2p6-turbo".to_string(),
                max_turns: 5,
                worktree: "/tmp/wt".to_string(),
                cyclops_version: "0.1.0".to_string(),
                prompts_hash: "0123456789abcdef".to_string(),
                tools_hash: "fedcba9876543210".to_string(),
            },
            json!({
                "type": "task_start",
                "ts_ns": 1001,
                "ts_wall": "2026-05-21T10:20:30.123456789Z",
                "seq": 1,
                "task_id": "task-1",
                "model": "accounts/fireworks/routers/kimi-k2p6-turbo",
                "max_turns": 5,
                "worktree": "/tmp/wt",
                "cyclops_version": "0.1.0",
                "prompts_hash": "0123456789abcdef",
                "tools_hash": "fedcba9876543210"
            }),
        );
    }

    #[test]
    fn task_end_serializes_status_and_usage_totals() {
        assert_event_shape(
            Event::TaskEnd {
                meta: meta(2),
                status: TaskEndStatus::MaxTurns,
                reason: "limit reached".to_string(),
                turns: 5,
                tokens_in: 100,
                tokens_out: 20,
                cache_read: 10,
                cache_write: 3,
                wallclock_ms: 2_500,
            },
            json!({
                "type": "task_end",
                "ts_ns": 1002,
                "ts_wall": "2026-05-21T10:20:30.123456789Z",
                "seq": 2,
                "status": "max_turns",
                "reason": "limit reached",
                "turns": 5,
                "tokens_in": 100,
                "tokens_out": 20,
                "cache_read": 10,
                "cache_write": 3,
                "wallclock_ms": 2500
            }),
        );
    }

    #[test]
    fn turn_start_serializes_context_counts() {
        assert_event_shape(
            Event::TurnStart {
                meta: meta(3),
                turn: 1,
                messages_in_context: 4,
                prompt_tokens_estimate: 512,
            },
            json!({
                "type": "turn_start",
                "ts_ns": 1003,
                "ts_wall": "2026-05-21T10:20:30.123456789Z",
                "seq": 3,
                "turn": 1,
                "messages_in_context": 4,
                "prompt_tokens_estimate": 512
            }),
        );
    }

    #[test]
    fn turn_end_serializes_stop_reason_and_usage() {
        assert_event_shape(
            Event::TurnEnd {
                meta: meta(4),
                turn: 1,
                stop_reason: "tool_calls".to_string(),
                tokens_in: 100,
                tokens_out: 25,
                cache_read: 12,
                cache_write: 4,
                duration_ms: 800,
            },
            json!({
                "type": "turn_end",
                "ts_ns": 1004,
                "ts_wall": "2026-05-21T10:20:30.123456789Z",
                "seq": 4,
                "turn": 1,
                "stop_reason": "tool_calls",
                "tokens_in": 100,
                "tokens_out": 25,
                "cache_read": 12,
                "cache_write": 4,
                "duration_ms": 800
            }),
        );
    }

    #[test]
    fn llm_request_serializes_request_counts() {
        assert_event_shape(
            Event::LlmRequest {
                meta: meta(5),
                turn: 2,
                model: "claude-sonnet-4-7".to_string(),
                n_messages: 7,
                n_tools: 5,
                cache_breakpoints: 2,
            },
            json!({
                "type": "llm_request",
                "ts_ns": 1005,
                "ts_wall": "2026-05-21T10:20:30.123456789Z",
                "seq": 5,
                "turn": 2,
                "model": "claude-sonnet-4-7",
                "n_messages": 7,
                "n_tools": 5,
                "cache_breakpoints": 2
            }),
        );
    }

    #[test]
    fn llm_first_token_serializes_ttft() {
        assert_event_shape(
            Event::LlmFirstToken {
                meta: meta(6),
                turn: 2,
                ttft_ms: 180,
            },
            json!({
                "type": "llm_first_token",
                "ts_ns": 1006,
                "ts_wall": "2026-05-21T10:20:30.123456789Z",
                "seq": 6,
                "turn": 2,
                "ttft_ms": 180
            }),
        );
    }

    #[test]
    fn text_delta_serializes_text_chunk() {
        assert_event_shape(
            Event::TextDelta {
                meta: meta(7),
                turn: 2,
                text: "hello".to_string(),
            },
            json!({
                "type": "text_delta",
                "ts_ns": 1007,
                "ts_wall": "2026-05-21T10:20:30.123456789Z",
                "seq": 7,
                "turn": 2,
                "text": "hello"
            }),
        );
    }

    #[test]
    fn thinking_delta_serializes_reasoning_chunk() {
        assert_event_shape(
            Event::ThinkingDelta {
                meta: meta(8),
                turn: 2,
                text: "thinking".to_string(),
            },
            json!({
                "type": "thinking_delta",
                "ts_ns": 1008,
                "ts_wall": "2026-05-21T10:20:30.123456789Z",
                "seq": 8,
                "turn": 2,
                "text": "thinking"
            }),
        );
    }

    #[test]
    fn tool_use_start_serializes_tool_identity_and_index() {
        assert_event_shape(
            Event::ToolUseStart {
                meta: meta(9),
                turn: 2,
                tool_id: "toolu_1".to_string(),
                name: "Read".to_string(),
                index: 0,
            },
            json!({
                "type": "tool_use_start",
                "ts_ns": 1009,
                "ts_wall": "2026-05-21T10:20:30.123456789Z",
                "seq": 9,
                "turn": 2,
                "tool_id": "toolu_1",
                "name": "Read",
                "index": 0
            }),
        );
    }

    #[test]
    fn tool_input_delta_serializes_raw_partial_args() {
        assert_event_shape(
            Event::ToolInputDelta {
                meta: meta(10),
                turn: 2,
                tool_id: "toolu_1".to_string(),
                partial_args: "{\"path\":\"src".to_string(),
            },
            json!({
                "type": "tool_input_delta",
                "ts_ns": 1010,
                "ts_wall": "2026-05-21T10:20:30.123456789Z",
                "seq": 10,
                "turn": 2,
                "tool_id": "toolu_1",
                "partial_args": "{\"path\":\"src"
            }),
        );
    }

    #[test]
    fn tool_dispatch_serializes_parsed_input_and_mode() {
        assert_event_shape(
            Event::ToolDispatch {
                meta: meta(11),
                turn: 2,
                tool_id: "toolu_1".to_string(),
                name: "Read".to_string(),
                input: json!({ "path": "src/lib.rs" }),
                mode: DispatchMode::Final,
            },
            json!({
                "type": "tool_dispatch",
                "ts_ns": 1011,
                "ts_wall": "2026-05-21T10:20:30.123456789Z",
                "seq": 11,
                "turn": 2,
                "tool_id": "toolu_1",
                "name": "Read",
                "input": { "path": "src/lib.rs" },
                "mode": "final"
            }),
        );
    }

    #[test]
    fn tool_dispatch_cancel_serializes_reason() {
        assert_event_shape(
            Event::ToolDispatchCancel {
                meta: meta(12),
                turn: 2,
                tool_id: "toolu_1".to_string(),
                reason: "final arguments diverged".to_string(),
            },
            json!({
                "type": "tool_dispatch_cancel",
                "ts_ns": 1012,
                "ts_wall": "2026-05-21T10:20:30.123456789Z",
                "seq": 12,
                "turn": 2,
                "tool_id": "toolu_1",
                "reason": "final arguments diverged"
            }),
        );
    }

    #[test]
    fn tool_result_serializes_error_only_when_present() {
        assert_event_shape(
            Event::ToolResult {
                meta: meta(13),
                turn: 2,
                tool_id: "toolu_1".to_string(),
                name: "Read".to_string(),
                ok: false,
                output: "permission denied".to_string(),
                output_bytes: 17,
                duration_ms: 20,
                error: Some("PermissionDenied".to_string()),
            },
            json!({
                "type": "tool_result",
                "ts_ns": 1013,
                "ts_wall": "2026-05-21T10:20:30.123456789Z",
                "seq": 13,
                "turn": 2,
                "tool_id": "toolu_1",
                "name": "Read",
                "ok": false,
                "output": "permission denied",
                "output_bytes": 17,
                "duration_ms": 20,
                "error": "PermissionDenied"
            }),
        );

        let value = serde_json::to_value(Event::ToolResult {
            meta: meta(14),
            turn: 2,
            tool_id: "toolu_2".to_string(),
            name: "Read".to_string(),
            ok: true,
            output: "body".to_string(),
            output_bytes: 4,
            duration_ms: 5,
            error: None,
        })
        .unwrap();
        assert!(value.get("error").is_none());
    }

    #[test]
    fn assistant_message_serializes_coalesced_tool_use_ids() {
        assert_event_shape(
            Event::AssistantMessage {
                meta: meta(15),
                turn: 2,
                text: "I will inspect the file.".to_string(),
                tool_uses: vec!["toolu_1".to_string(), "toolu_2".to_string()],
            },
            json!({
                "type": "assistant_message",
                "ts_ns": 1015,
                "ts_wall": "2026-05-21T10:20:30.123456789Z",
                "seq": 15,
                "turn": 2,
                "text": "I will inspect the file.",
                "tool_uses": ["toolu_1", "toolu_2"]
            }),
        );
    }

    #[test]
    fn completion_signal_serializes_kind() {
        assert_event_shape(
            Event::CompletionSignal {
                meta: meta(16),
                turn: 3,
                kind: CompletionSignalKind::PromiseComplete,
            },
            json!({
                "type": "completion_signal",
                "ts_ns": 1016,
                "ts_wall": "2026-05-21T10:20:30.123456789Z",
                "seq": 16,
                "turn": 3,
                "kind": "promise_complete"
            }),
        );
    }

    #[test]
    fn memory_compaction_serializes_turn_range_and_token_counts() {
        assert_event_shape(
            Event::MemoryCompaction {
                meta: meta(17),
                turn_range: TurnRange { start: 1, end: 4 },
                tokens_before: 20_000,
                tokens_after: 3_000,
                duration_ms: 120,
                summary_len: 900,
            },
            json!({
                "type": "memory_compaction",
                "ts_ns": 1017,
                "ts_wall": "2026-05-21T10:20:30.123456789Z",
                "seq": 17,
                "turn_range": { "start": 1, "end": 4 },
                "tokens_before": 20000,
                "tokens_after": 3000,
                "duration_ms": 120,
                "summary_len": 900
            }),
        );
    }

    #[test]
    fn memory_fact_write_serializes_source() {
        assert_event_shape(
            Event::MemoryFactWrite {
                meta: meta(18),
                key: "repo.language".to_string(),
                value: "rust".to_string(),
                confidence: 0.95,
                source: MemoryFactSource::PatternExtract,
            },
            json!({
                "type": "memory_fact_write",
                "ts_ns": 1018,
                "ts_wall": "2026-05-21T10:20:30.123456789Z",
                "seq": 18,
                "key": "repo.language",
                "value": "rust",
                "confidence": 0.95,
                "source": "pattern_extract"
            }),
        );
    }

    #[test]
    fn memory_file_cache_stats_serializes_cache_counters() {
        assert_event_shape(
            Event::MemoryFileCacheStats {
                meta: meta(19),
                hits: 10,
                misses: 2,
                evictions: 1,
                bytes_cached: 4_096,
            },
            json!({
                "type": "memory_file_cache_stats",
                "ts_ns": 1019,
                "ts_wall": "2026-05-21T10:20:30.123456789Z",
                "seq": 19,
                "hits": 10,
                "misses": 2,
                "evictions": 1,
                "bytes_cached": 4096
            }),
        );
    }

    #[test]
    fn cancel_serializes_source_and_timestamp() {
        assert_event_shape(
            Event::Cancel {
                meta: meta(20),
                source: CancelSource::Signal,
                at: "2026-05-21T10:21:00.000000000Z".to_string(),
            },
            json!({
                "type": "cancel",
                "ts_ns": 1020,
                "ts_wall": "2026-05-21T10:20:30.123456789Z",
                "seq": 20,
                "source": "signal",
                "at": "2026-05-21T10:21:00.000000000Z"
            }),
        );
    }

    #[test]
    fn error_serializes_reserved_where_field() {
        assert_event_shape(
            Event::Error {
                meta: meta(21),
                where_: "model::stream".to_string(),
                class: "Decode".to_string(),
                message: "invalid chunk".to_string(),
                retryable: true,
            },
            json!({
                "type": "error",
                "ts_ns": 1021,
                "ts_wall": "2026-05-21T10:20:30.123456789Z",
                "seq": 21,
                "where": "model::stream",
                "class": "Decode",
                "message": "invalid chunk",
                "retryable": true
            }),
        );
    }
}
