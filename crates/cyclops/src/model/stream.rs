use bytes::{BufMut, Bytes, BytesMut};
use serde::Deserialize;

use crate::{CyclopsError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamEvent {
    TextDelta(String),
    ToolCallStart { index: u32 },
    ToolCallId { index: u32, id: String },
    ToolCallName { index: u32, name: String },
    ToolCallArgumentsDelta { index: u32, arguments: String },
    FinishReason(String),
    Usage(StreamUsage),
    Done,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StreamUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChunk {
    #[serde(default)]
    choices: Vec<StreamChoice>,
    usage: Option<StreamUsage>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    delta: Option<StreamDelta>,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StreamDelta {
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<ToolCallDelta>,
}

#[derive(Debug, Deserialize)]
struct ToolCallDelta {
    index: u32,
    id: Option<String>,
    function: Option<ToolFunctionDelta>,
}

#[derive(Debug, Deserialize)]
struct ToolFunctionDelta {
    name: Option<String>,
    arguments: Option<String>,
}

pub fn parse_stream_frame(frame: &[u8]) -> Result<Vec<StreamEvent>> {
    if frame == b"[DONE]" {
        return Ok(vec![StreamEvent::Done]);
    }

    let chunk: ChatCompletionChunk = serde_json::from_slice(frame).map_err(|error| {
        CyclopsError::Stream(format!(
            "failed to parse chat completion stream chunk: {error}"
        ))
    })?;

    let mut events = Vec::new();
    for choice in chunk.choices {
        if let Some(delta) = choice.delta {
            if let Some(content) = delta.content {
                if !content.is_empty() {
                    events.push(StreamEvent::TextDelta(content));
                }
            }

            for tool_call in delta.tool_calls {
                let starts_tool_call = tool_call.id.is_some()
                    || tool_call
                        .function
                        .as_ref()
                        .is_some_and(|function| function.name.is_some());
                if starts_tool_call {
                    events.push(StreamEvent::ToolCallStart {
                        index: tool_call.index,
                    });
                }

                if let Some(id) = tool_call.id {
                    events.push(StreamEvent::ToolCallId {
                        index: tool_call.index,
                        id,
                    });
                }

                if let Some(function) = tool_call.function {
                    if let Some(name) = function.name {
                        events.push(StreamEvent::ToolCallName {
                            index: tool_call.index,
                            name,
                        });
                    }
                    if let Some(arguments) = function.arguments {
                        if !arguments.is_empty() {
                            events.push(StreamEvent::ToolCallArgumentsDelta {
                                index: tool_call.index,
                                arguments,
                            });
                        }
                    }
                }
            }
        }

        if let Some(reason) = choice.finish_reason {
            events.push(StreamEvent::FinishReason(reason));
        }
    }

    if let Some(usage) = chunk.usage {
        events.push(StreamEvent::Usage(usage));
    }

    Ok(events)
}

#[derive(Debug, Default)]
pub struct SseFramer {
    pending: BytesMut,
    data_lines: Vec<Bytes>,
}

impl SseFramer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, chunk: Bytes) -> Vec<Bytes> {
        self.pending.extend_from_slice(&chunk);

        let mut frames = Vec::new();
        while let Some(line_end) = self.pending.iter().position(|byte| *byte == b'\n') {
            let mut line = self.pending.split_to(line_end + 1).freeze();
            line.truncate(line.len() - 1);
            if line.ends_with(b"\r") {
                line.truncate(line.len() - 1);
            }

            if let Some(frame) = self.process_line(line) {
                frames.push(frame);
            }
        }

        frames
    }

    fn process_line(&mut self, line: Bytes) -> Option<Bytes> {
        if line.is_empty() {
            return self.finish_event();
        }

        if line.starts_with(b":") {
            return None;
        }

        let (field, value) = match line.iter().position(|byte| *byte == b':') {
            Some(colon) => {
                let field = line.slice(..colon);
                let mut value = line.slice(colon + 1..);
                if value.starts_with(b" ") {
                    value = value.slice(1..);
                }
                (field, value)
            }
            None => (line.clone(), Bytes::new()),
        };

        if field.as_ref() == b"data" {
            self.data_lines.push(value);
        }

        None
    }

    fn finish_event(&mut self) -> Option<Bytes> {
        match self.data_lines.len() {
            0 => None,
            1 => self.data_lines.pop(),
            _ => {
                let payload_len = self.data_lines.iter().map(Bytes::len).sum::<usize>()
                    + self.data_lines.len()
                    - 1;
                let mut payload = BytesMut::with_capacity(payload_len);

                for (index, line) in self.data_lines.drain(..).enumerate() {
                    if index > 0 {
                        payload.put_u8(b'\n');
                    }
                    payload.extend_from_slice(&line);
                }

                Some(payload.freeze())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_byte_by_byte(input: &[u8]) -> Vec<Bytes> {
        let mut framer = SseFramer::new();
        let mut frames = Vec::new();

        for byte in input {
            frames.extend(framer.push(Bytes::copy_from_slice(&[*byte])));
        }

        frames
    }

    fn parse_transcript(input: &[u8]) -> Vec<StreamEvent> {
        feed_byte_by_byte(input)
            .into_iter()
            .flat_map(|frame| parse_stream_frame(&frame).unwrap())
            .collect()
    }

    #[test]
    fn parses_recorded_openai_shape_sse_into_stream_events() {
        let transcript = br#"data: {"id":"chatcmpl-test","object":"chat.completion.chunk","created":1710000000,"model":"test-model","choices":[{"index":0,"delta":{"role":"assistant","content":"Hel"},"finish_reason":null}],"usage":null}

data: {"id":"chatcmpl-test","object":"chat.completion.chunk","created":1710000000,"model":"test-model","choices":[{"index":0,"delta":{"content":"lo"},"finish_reason":null}],"usage":null}

data: {"id":"chatcmpl-test","object":"chat.completion.chunk","created":1710000000,"model":"test-model","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_read","type":"function","function":{"name":"read_file","arguments":""}}]},"finish_reason":null}],"usage":null}

data: {"id":"chatcmpl-test","object":"chat.completion.chunk","created":1710000000,"model":"test-model","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"path\""}}]},"finish_reason":null}],"usage":null}

data: {"id":"chatcmpl-test","object":"chat.completion.chunk","created":1710000000,"model":"test-model","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":":\"src/main.rs\"}"}}]},"finish_reason":null}],"usage":null}

data: {"id":"chatcmpl-test","object":"chat.completion.chunk","created":1710000000,"model":"test-model","choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}],"usage":null}

data: {"id":"chatcmpl-test","object":"chat.completion.chunk","created":1710000000,"model":"test-model","choices":[],"usage":{"prompt_tokens":11,"completion_tokens":7,"total_tokens":18}}

data: [DONE]

"#;

        let events = parse_transcript(transcript);

        assert_eq!(
            events,
            vec![
                StreamEvent::TextDelta("Hel".to_string()),
                StreamEvent::TextDelta("lo".to_string()),
                StreamEvent::ToolCallStart { index: 0 },
                StreamEvent::ToolCallId {
                    index: 0,
                    id: "call_read".to_string(),
                },
                StreamEvent::ToolCallName {
                    index: 0,
                    name: "read_file".to_string(),
                },
                StreamEvent::ToolCallArgumentsDelta {
                    index: 0,
                    arguments: "{\"path\"".to_string(),
                },
                StreamEvent::ToolCallArgumentsDelta {
                    index: 0,
                    arguments: ":\"src/main.rs\"}".to_string(),
                },
                StreamEvent::FinishReason("tool_calls".to_string()),
                StreamEvent::Usage(StreamUsage {
                    prompt_tokens: 11,
                    completion_tokens: 7,
                    total_tokens: 18,
                }),
                StreamEvent::Done,
            ]
        );
    }

    #[test]
    fn reports_invalid_stream_chunk_as_stream_error() {
        let error = parse_stream_frame(br#"{"choices":"not a list"}"#).unwrap_err();

        assert!(matches!(
            error.downcast_ref::<CyclopsError>(),
            Some(CyclopsError::Stream(message))
                if message.starts_with("failed to parse chat completion stream chunk:")
        ));
    }

    #[test]
    fn frames_recorded_openai_shape_sse_byte_by_byte() {
        let transcript = b"data: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}\n\
data: {\"choices\":[{\"delta\":{\"content\":\"hel\"}}]}\n\n\
data: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}\r\n\r\n\
data: [DONE]\n\n";

        let frames = feed_byte_by_byte(transcript);

        assert_eq!(
            frames,
            vec![
                Bytes::from_static(
                    b"{\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}\n{\"choices\":[{\"delta\":{\"content\":\"hel\"}}]}"
                ),
                Bytes::from_static(b"{\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}"),
                Bytes::from_static(b"[DONE]"),
            ]
        );
    }

    #[test]
    fn ignores_comments_and_non_data_fields() {
        let transcript = b": keepalive\n\
event: message\n\
id: 42\n\
data: first\n\
: ignored between data lines\n\
retry: 1000\n\
data: second\n\n";

        let frames = feed_byte_by_byte(transcript);

        assert_eq!(frames, vec![Bytes::from_static(b"first\nsecond")]);
    }

    #[test]
    fn waits_for_blank_line_before_emitting() {
        let mut framer = SseFramer::new();

        assert!(framer.push(Bytes::from_static(b"data: partial")).is_empty());
        assert!(framer.push(Bytes::from_static(b"\n")).is_empty());
        assert_eq!(
            framer.push(Bytes::from_static(b"\n")),
            vec![Bytes::from_static(b"partial")]
        );
    }

    #[test]
    fn supports_empty_data_lines_and_field_without_colon() {
        let transcript = b"data\n\
data:\n\
data: third\n\n";

        let frames = feed_byte_by_byte(transcript);

        assert_eq!(frames, vec![Bytes::from_static(b"\n\nthird")]);
    }
}
