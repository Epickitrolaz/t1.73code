use crate::protocol::models::{
    ServerProviderModel, ServerProviderSkill, ServerProviderSlashCommand,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub mime_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl Attachment {
    pub fn new(mime_type: impl Into<String>) -> Self {
        Self {
            mime_type: mime_type.into(),
            data: None,
            path: None,
        }
    }

    pub fn image(mime_type: impl Into<String>, base64_data: impl Into<String>) -> Self {
        Self {
            mime_type: mime_type.into(),
            data: Some(base64_data.into()),
            path: None,
        }
    }

    pub fn file(mime_type: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            mime_type: mime_type.into(),
            data: None,
            path: Some(path.into()),
        }
    }

    pub fn is_image(&self) -> bool {
        self.mime_type.starts_with("image/")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PiImageAttachment {
    #[serde(rename = "type")]
    pub item_type: String,
    pub data: String,
    pub mime_type: String,
}

impl PiImageAttachment {
    pub fn new(data: impl Into<String>, mime_type: impl Into<String>) -> Self {
        Self {
            item_type: "image".to_string(),
            data: data.into(),
            mime_type: mime_type.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PiRpcRequest {
    Prompt {
        id: String,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none", rename = "streamingBehavior")]
        streaming_behavior: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        images: Option<Vec<PiImageAttachment>>,
    },
    Compact {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none", rename = "customInstructions")]
        custom_instructions: Option<String>,
    },
    Abort {
        id: String,
    },
    GetAvailableModels {
        id: String,
    },
    GetCommands {
        id: String,
    },
    GetState {
        id: String,
    },
    SetModel {
        id: String,
        provider: String,
        #[serde(rename = "modelId")]
        model_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PiRpcResponse {
    #[serde(rename = "type")]
    pub response_type: String,
    pub id: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    pub success: bool,
    #[serde(default)]
    pub data: Option<serde_json::Value>,
    #[serde(default)]
    pub error: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PiModelItem {
    pub provider: String,
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
}

impl PiModelItem {
    pub fn to_server_provider_model(&self) -> ServerProviderModel {
        let slug = if self.id.contains('/') || self.provider.is_empty() {
            self.id.clone()
        } else {
            format!("{}/{}", self.provider, self.id)
        };
        let label = self.name.clone().or_else(|| Some(slug.clone()));
        ServerProviderModel { id: slug, label }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PiModelsData {
    #[serde(default)]
    pub models: Vec<PiModelItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PiCommandItem {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default, rename = "sourceInfo")]
    pub source_info: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PiCommandsData {
    #[serde(default)]
    pub commands: Vec<PiCommandItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiDiscoveredCommands {
    pub slash_commands: Vec<ServerProviderSlashCommand>,
    pub skills: Vec<ServerProviderSkill>,
}

impl PiCommandsData {
    pub fn into_discovered_commands(self) -> PiDiscoveredCommands {
        let mut slash_commands = Vec::new();
        let mut skills = Vec::new();
        let mut has_compact = false;

        for cmd in self.commands {
            if cmd.name.trim().is_empty() {
                continue;
            }
            if cmd.name == "compact" {
                has_compact = true;
            }

            if cmd.source.as_deref() == Some("skill") {
                let name = cmd
                    .name
                    .strip_prefix("skill:")
                    .unwrap_or(&cmd.name)
                    .to_string();
                let path = cmd
                    .source_info
                    .as_ref()
                    .and_then(|info| info.get("path").and_then(|p| p.as_str()))
                    .or(cmd.path.as_deref())
                    .unwrap_or(&format!("pi:skill:{}", name))
                    .to_string();

                skills.push(ServerProviderSkill {
                    name,
                    path,
                    enabled: true,
                    description: cmd.description,
                });
            } else {
                slash_commands.push(ServerProviderSlashCommand {
                    name: cmd.name,
                    description: cmd.description,
                });
            }
        }

        // T3 Code Pi driver always provides built-in /compact command
        if !has_compact {
            slash_commands.insert(
                0,
                ServerProviderSlashCommand {
                    name: "compact".to_string(),
                    description: Some(
                        "Summarize the conversation and reduce context usage".to_string(),
                    ),
                },
            );
        }

        PiDiscoveredCommands {
            slash_commands,
            skills,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum PiRpcEvent {
    #[serde(rename = "agent_start")]
    AgentStart,

    #[serde(rename = "agent_settled")]
    AgentSettled {
        #[serde(default)]
        aborted: bool,
    },

    #[serde(rename = "message_start")]
    MessageStart {
        #[serde(default)]
        message: serde_json::Value,
    },

    #[serde(rename = "message_update")]
    MessageUpdate {
        #[serde(default, rename = "assistantMessageEvent")]
        assistant_message_event: Option<AssistantMessageDelta>,
        #[serde(default)]
        usage: Option<serde_json::Value>,
    },

    #[serde(rename = "message_end")]
    MessageEnd {
        #[serde(default)]
        message: serde_json::Value,
    },

    #[serde(rename = "tool_execution_start")]
    ToolExecutionStart {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(rename = "toolName")]
        tool_name: String,
        #[serde(default)]
        args: Option<serde_json::Value>,
        #[serde(default, rename = "parentToolCallId")]
        parent_tool_call_id: Option<String>,
    },

    #[serde(rename = "tool_execution_update")]
    ToolExecutionUpdate {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(default, rename = "partialResult")]
        partial_result: Option<serde_json::Value>,
    },

    #[serde(rename = "tool_execution_end")]
    ToolExecutionEnd {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(default, rename = "toolName")]
        tool_name: Option<String>,
        #[serde(default, rename = "isError")]
        is_error: bool,
        #[serde(default)]
        result: Option<serde_json::Value>,
    },

    #[serde(rename = "compaction_start")]
    CompactionStart,

    #[serde(rename = "compaction_end")]
    CompactionEnd {
        #[serde(default)]
        aborted: bool,
        #[serde(default, rename = "errorMessage")]
        error_message: Option<String>,
        #[serde(default)]
        result: Option<serde_json::Value>,
    },

    #[serde(rename = "auto_retry_start")]
    AutoRetryStart {
        #[serde(default)]
        attempt: Option<u32>,
        #[serde(default, rename = "maxAttempts")]
        max_attempts: Option<u32>,
        #[serde(default, rename = "delayMs")]
        delay_ms: Option<u64>,
        #[serde(default, rename = "errorMessage")]
        error_message: Option<String>,
    },

    #[serde(rename = "auto_retry_end")]
    AutoRetryEnd {
        #[serde(default)]
        success: bool,
        #[serde(default)]
        attempt: Option<u32>,
        #[serde(default, rename = "finalError")]
        final_error: Option<String>,
    },

    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum AssistantMessageDelta {
    #[serde(rename = "text_delta")]
    TextDelta {
        delta: String,
        #[serde(default, rename = "contentIndex")]
        content_index: Option<usize>,
    },
    #[serde(rename = "thinking_delta")]
    ThinkingDelta {
        delta: String,
        #[serde(default, rename = "contentIndex")]
        content_index: Option<usize>,
    },
    #[serde(rename = "text_end")]
    TextEnd {
        #[serde(default)]
        content: Option<String>,
        #[serde(default, rename = "contentIndex")]
        content_index: Option<usize>,
    },
    #[serde(rename = "thinking_end")]
    ThinkingEnd {
        #[serde(default)]
        thinking: Option<String>,
        #[serde(default, rename = "contentIndex")]
        content_index: Option<usize>,
    },
    #[serde(other)]
    Other,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_request_serialization() {
        let req = PiRpcRequest::Prompt {
            id: "t3-1".to_string(),
            message: "Hello Pi".to_string(),
            streaming_behavior: Some("steer".to_string()),
            images: Some(vec![PiImageAttachment::new("base64data", "image/png")]),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"type\":\"prompt\""));
        assert!(json.contains("\"id\":\"t3-1\""));
        assert!(json.contains("\"message\":\"Hello Pi\""));
        assert!(json.contains("\"streamingBehavior\":\"steer\""));
        assert!(json.contains("\"mimeType\":\"image/png\""));
    }

    #[test]
    fn test_compact_request_serialization() {
        let req = PiRpcRequest::Compact {
            id: "t3-2".to_string(),
            custom_instructions: Some("Summarize briefly".to_string()),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"type\":\"compact\""));
        assert!(json.contains("\"id\":\"t3-2\""));
        assert!(json.contains("\"customInstructions\":\"Summarize briefly\""));

        let req_no_instructions = PiRpcRequest::Compact {
            id: "t3-3".to_string(),
            custom_instructions: None,
        };
        let json2 = serde_json::to_string(&req_no_instructions).unwrap();
        assert!(!json2.contains("customInstructions"));
    }

    #[test]
    fn test_abort_and_models_request_serialization() {
        let req_abort = PiRpcRequest::Abort {
            id: "t3-4".to_string(),
        };
        assert_eq!(
            serde_json::to_value(&req_abort).unwrap(),
            serde_json::json!({
                "type": "abort",
                "id": "t3-4"
            })
        );

        let req_models = PiRpcRequest::GetAvailableModels {
            id: "t3-5".to_string(),
        };
        assert_eq!(
            serde_json::to_value(&req_models).unwrap(),
            serde_json::json!({
                "type": "get_available_models",
                "id": "t3-5"
            })
        );
    }

    #[test]
    fn test_response_deserialization() {
        let json = r#"{"type":"response","id":"t3-0","command":"get_available_models","success":true,"data":{"models":[{"provider":"anthropic","id":"claude-3-5-sonnet","name":"Claude 3.5 Sonnet"}]}}"#;
        let resp: PiRpcResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.id, Some("t3-0".to_string()));
        assert!(resp.success);
        let data: PiModelsData = serde_json::from_value(resp.data.unwrap()).unwrap();
        assert_eq!(data.models.len(), 1);
        let model = data.models[0].to_server_provider_model();
        assert_eq!(model.id, "anthropic/claude-3-5-sonnet");
        assert_eq!(model.label, Some("Claude 3.5 Sonnet".to_string()));
    }

    #[test]
    fn test_commands_parsing_with_builtin_compact() {
        let data = PiCommandsData {
            commands: vec![
                PiCommandItem {
                    name: "status".to_string(),
                    description: Some("Check status".to_string()),
                    source: None,
                    path: None,
                    source_info: None,
                },
                PiCommandItem {
                    name: "skill:code-review".to_string(),
                    description: Some("Review PR".to_string()),
                    source: Some("skill".to_string()),
                    path: Some("/path/to/skill".to_string()),
                    source_info: None,
                },
            ],
        };
        let discovered = data.into_discovered_commands();
        // Builtin compact was inserted
        assert_eq!(discovered.slash_commands.len(), 2);
        assert_eq!(discovered.slash_commands[0].name, "compact");
        assert_eq!(discovered.slash_commands[1].name, "status");
        assert_eq!(discovered.skills.len(), 1);
        assert_eq!(discovered.skills[0].name, "code-review");
        assert_eq!(discovered.skills[0].path, "/path/to/skill");
    }
}
