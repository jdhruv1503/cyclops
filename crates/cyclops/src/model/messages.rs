use serde::de::{self, Deserializer};
use serde::ser::{SerializeStruct, Serializer};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum ChatMessage {
    User {
        content: Vec<UserBlock>,
    },
    Assistant {
        blocks: Vec<AssistantBlock>,
    },
    Tool {
        tool_call_id: String,
        content: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UserBlock {
    Text { text: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AssistantBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: ToolCallKind,
    pub function: ToolFunctionCall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallKind {
    Function,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolFunctionCall {
    pub name: String,
    pub arguments: String,
}

impl ChatMessage {
    pub fn user_text(text: impl Into<String>) -> Self {
        Self::User {
            content: vec![UserBlock::Text { text: text.into() }],
        }
    }

    pub fn assistant_text(text: impl Into<String>) -> Self {
        Self::Assistant {
            blocks: vec![AssistantBlock::Text { text: text.into() }],
        }
    }

    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self::Tool {
            tool_call_id: tool_call_id.into(),
            content: content.into(),
        }
    }
}

impl Serialize for ChatMessage {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::User { content } => {
                let mut state = serializer.serialize_struct("ChatMessage", 2)?;
                state.serialize_field("role", "user")?;
                state.serialize_field("content", content)?;
                state.end()
            }
            Self::Assistant { blocks } => {
                let text = assistant_text_content(blocks);
                let tool_calls = assistant_tool_calls(blocks).map_err(serde::ser::Error::custom)?;

                let field_count = if tool_calls.is_empty() { 2 } else { 3 };
                let mut state = serializer.serialize_struct("ChatMessage", field_count)?;
                state.serialize_field("role", "assistant")?;
                state.serialize_field("content", &text)?;
                if !tool_calls.is_empty() {
                    state.serialize_field("tool_calls", &tool_calls)?;
                }
                state.end()
            }
            Self::Tool {
                tool_call_id,
                content,
            } => {
                let mut state = serializer.serialize_struct("ChatMessage", 3)?;
                state.serialize_field("role", "tool")?;
                state.serialize_field("tool_call_id", tool_call_id)?;
                state.serialize_field("content", content)?;
                state.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for ChatMessage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let message = WireChatMessage::deserialize(deserializer)?;
        match message.role.as_str() {
            "user" => {
                let content = message
                    .content
                    .ok_or_else(|| de::Error::missing_field("content"))?
                    .into();
                Ok(Self::User { content })
            }
            "assistant" => {
                let mut blocks = Vec::new();
                if let Some(content) = message.content {
                    let text = String::from(content);
                    if !text.is_empty() {
                        blocks.push(AssistantBlock::Text { text });
                    }
                }
                for tool_call in message.tool_calls.unwrap_or_default() {
                    match tool_call.kind {
                        ToolCallKind::Function => {
                            let input = serde_json::from_str(&tool_call.function.arguments)
                                .map_err(|error| {
                                    de::Error::custom(format!(
                                        "tool call arguments are not JSON: {error}"
                                    ))
                                })?;
                            blocks.push(AssistantBlock::ToolUse {
                                id: tool_call.id,
                                name: tool_call.function.name,
                                input,
                            });
                        }
                    }
                }
                Ok(Self::Assistant { blocks })
            }
            "tool" => {
                let tool_call_id = message
                    .tool_call_id
                    .ok_or_else(|| de::Error::missing_field("tool_call_id"))?;
                let content = message
                    .content
                    .ok_or_else(|| de::Error::missing_field("content"))?
                    .into();
                Ok(Self::Tool {
                    tool_call_id,
                    content,
                })
            }
            role => Err(de::Error::unknown_variant(
                role,
                &["user", "assistant", "tool"],
            )),
        }
    }
}

fn assistant_text_content(blocks: &[AssistantBlock]) -> String {
    blocks
        .iter()
        .filter_map(|block| match block {
            AssistantBlock::Text { text } => Some(text.as_str()),
            AssistantBlock::ToolUse { .. } => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn assistant_tool_calls(blocks: &[AssistantBlock]) -> Result<Vec<ToolCall>, serde_json::Error> {
    blocks
        .iter()
        .filter_map(|block| match block {
            AssistantBlock::Text { .. } => None,
            AssistantBlock::ToolUse { id, name, input } => {
                Some(serde_json::to_string(input).map(|arguments| ToolCall {
                    id: id.clone(),
                    kind: ToolCallKind::Function,
                    function: ToolFunctionCall {
                        name: name.clone(),
                        arguments,
                    },
                }))
            }
        })
        .collect()
}

#[derive(Debug, Deserialize)]
struct WireChatMessage {
    role: String,
    content: Option<ValueOrUserBlocks>,
    tool_calls: Option<Vec<ToolCall>>,
    tool_call_id: Option<String>,
}

#[derive(Debug)]
enum ValueOrUserBlocks {
    Text(String),
    UserBlocks(Vec<UserBlock>),
}

impl<'de> Deserialize<'de> for ValueOrUserBlocks {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        match value {
            Value::String(text) => Ok(Self::Text(text)),
            Value::Array(_) => {
                let blocks = serde_json::from_value(value).map_err(de::Error::custom)?;
                Ok(Self::UserBlocks(blocks))
            }
            other => Err(de::Error::custom(format!(
                "expected string or content block array, got {other}"
            ))),
        }
    }
}

impl Serialize for ValueOrUserBlocks {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Text(text) => serializer.serialize_str(text),
            Self::UserBlocks(blocks) => blocks.serialize(serializer),
        }
    }
}

impl From<ValueOrUserBlocks> for Vec<UserBlock> {
    fn from(content: ValueOrUserBlocks) -> Self {
        match content {
            ValueOrUserBlocks::Text(text) => vec![UserBlock::Text { text }],
            ValueOrUserBlocks::UserBlocks(blocks) => blocks,
        }
    }
}

impl From<ValueOrUserBlocks> for String {
    fn from(content: ValueOrUserBlocks) -> Self {
        match content {
            ValueOrUserBlocks::Text(text) => text,
            ValueOrUserBlocks::UserBlocks(blocks) => blocks
                .into_iter()
                .map(|block| match block {
                    UserBlock::Text { text } => text,
                })
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::*;

    #[test]
    fn user_message_round_trips_as_openai_content_blocks() {
        let message = ChatMessage::user_text("fix the test");
        let expected = json!({
            "role": "user",
            "content": [
                {
                    "type": "text",
                    "text": "fix the test"
                }
            ]
        });

        assert_eq!(serde_json::to_value(&message).unwrap(), expected);
        assert_eq!(
            serde_json::from_value::<ChatMessage>(expected).unwrap(),
            message
        );
    }

    #[test]
    fn assistant_text_message_round_trips_as_openai_message() {
        let message = ChatMessage::assistant_text("I will inspect the failure.");
        let expected = json!({
            "role": "assistant",
            "content": "I will inspect the failure."
        });

        assert_eq!(serde_json::to_value(&message).unwrap(), expected);
        assert_eq!(
            serde_json::from_value::<ChatMessage>(expected).unwrap(),
            message
        );
    }

    #[test]
    fn assistant_tool_use_round_trips_as_openai_tool_call() {
        let message = ChatMessage::Assistant {
            blocks: vec![AssistantBlock::ToolUse {
                id: "call_read_1".to_string(),
                name: "read".to_string(),
                input: json!({ "path": "src/lib.rs" }),
            }],
        };
        let expected = json!({
            "role": "assistant",
            "content": "",
            "tool_calls": [
                {
                    "id": "call_read_1",
                    "type": "function",
                    "function": {
                        "name": "read",
                        "arguments": "{\"path\":\"src/lib.rs\"}"
                    }
                }
            ]
        });

        assert_eq!(serde_json::to_value(&message).unwrap(), expected);
        assert_eq!(
            serde_json::from_value::<ChatMessage>(expected).unwrap(),
            message
        );
    }

    #[test]
    fn assistant_text_and_tool_use_round_trips_as_openai_message() {
        let message = ChatMessage::Assistant {
            blocks: vec![
                AssistantBlock::Text {
                    text: "I need the file.".to_string(),
                },
                AssistantBlock::ToolUse {
                    id: "call_read_1".to_string(),
                    name: "read".to_string(),
                    input: json!({ "path": "src/lib.rs" }),
                },
            ],
        };
        let expected = json!({
            "role": "assistant",
            "content": "I need the file.",
            "tool_calls": [
                {
                    "id": "call_read_1",
                    "type": "function",
                    "function": {
                        "name": "read",
                        "arguments": "{\"path\":\"src/lib.rs\"}"
                    }
                }
            ]
        });

        assert_eq!(serde_json::to_value(&message).unwrap(), expected);
        assert_eq!(
            serde_json::from_value::<ChatMessage>(expected).unwrap(),
            message
        );
    }

    #[test]
    fn tool_result_round_trips_as_openai_tool_message() {
        let message = ChatMessage::tool_result("call_read_1", "file contents");
        let expected = json!({
            "role": "tool",
            "tool_call_id": "call_read_1",
            "content": "file contents"
        });

        assert_eq!(serde_json::to_value(&message).unwrap(), expected);
        assert_eq!(
            serde_json::from_value::<ChatMessage>(expected).unwrap(),
            message
        );
    }

    #[test]
    fn user_openai_string_content_deserializes_to_text_block() {
        let value = json!({
            "role": "user",
            "content": "plain prompt"
        });

        assert_eq!(
            serde_json::from_value::<ChatMessage>(value).unwrap(),
            ChatMessage::user_text("plain prompt")
        );
    }

    #[test]
    fn assistant_tool_call_arguments_must_be_json() {
        let value = json!({
            "role": "assistant",
            "content": "",
            "tool_calls": [
                {
                    "id": "call_read_1",
                    "type": "function",
                    "function": {
                        "name": "read",
                        "arguments": "{not json}"
                    }
                }
            ]
        });

        let error = serde_json::from_value::<ChatMessage>(value).unwrap_err();
        assert!(error
            .to_string()
            .contains("tool call arguments are not JSON"));
    }

    #[test]
    fn assistant_text_block_round_trips_as_internal_block_shape() {
        let block = AssistantBlock::Text {
            text: "done".to_string(),
        };
        let expected = json!({
            "type": "text",
            "text": "done"
        });

        assert_eq!(serde_json::to_value(&block).unwrap(), expected);
        assert_eq!(
            serde_json::from_value::<AssistantBlock>(expected).unwrap(),
            block
        );
    }

    #[test]
    fn assistant_tool_use_block_round_trips_as_internal_block_shape() {
        let block = AssistantBlock::ToolUse {
            id: "call_read_1".to_string(),
            name: "read".to_string(),
            input: json!({ "path": "src/lib.rs" }),
        };

        assert_eq!(
            serde_json::to_value(&block).unwrap(),
            json!({
                "type": "tool_use",
                "id": "call_read_1",
                "name": "read",
                "input": {
                    "path": "src/lib.rs"
                }
            })
        );
        assert_eq!(
            serde_json::from_value::<AssistantBlock>(serde_json::to_value(&block).unwrap())
                .unwrap(),
            block
        );
    }

    fn assert_round_trip(message: ChatMessage) {
        let value: Value = serde_json::to_value(&message).unwrap();
        assert_eq!(
            serde_json::from_value::<ChatMessage>(value).unwrap(),
            message
        );
    }

    #[test]
    fn every_chat_message_variant_round_trips() {
        assert_round_trip(ChatMessage::user_text("hello"));
        assert_round_trip(ChatMessage::assistant_text("done"));
        assert_round_trip(ChatMessage::Tool {
            tool_call_id: "call_1".to_string(),
            content: "ok".to_string(),
        });
    }
}
