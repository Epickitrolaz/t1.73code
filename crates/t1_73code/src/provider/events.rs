use crate::protocol::models::{ModelSelection, OrchestrationV2Run, OrchestrationV2TurnItem};
use crate::provider::error::PiError;
use crate::provider::models::{AssistantMessageDelta, PiRpcEvent};
use chrono::Utc;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum DriverEvent {
    RunStarted(OrchestrationV2Run),
    RunUpdated(OrchestrationV2Run),
    RunCompleted(OrchestrationV2Run),
    TurnItemStarted(OrchestrationV2TurnItem),
    TurnItemUpdated(OrchestrationV2TurnItem),
    TurnItemCompleted(OrchestrationV2TurnItem),
    Raw(PiRpcEvent),
}

pub struct PiEventTranslator {
    thread_id: String,
    run_id: String,
    provider_instance_id: String,
    model_selection: ModelSelection,
    current_run: OrchestrationV2Run,
    ordinal_counter: u64,
    active_message_item_id: Option<String>,
    active_reasoning_item_id: Option<String>,
    active_compaction_item_id: Option<String>,
    active_items: HashMap<String, OrchestrationV2TurnItem>,
    tool_args: HashMap<String, serde_json::Value>,
}

impl PiEventTranslator {
    pub fn new(
        thread_id: impl Into<String>,
        run_id: impl Into<String>,
        provider_instance_id: impl Into<String>,
        model_selection: ModelSelection,
    ) -> Self {
        let thread_id = thread_id.into();
        let run_id = run_id.into();
        let provider_instance_id = provider_instance_id.into();
        let now = Utc::now();

        let current_run = OrchestrationV2Run {
            id: run_id.clone(),
            thread_id: thread_id.clone(),
            ordinal: 1,
            provider_instance_id: provider_instance_id.clone(),
            model_selection: model_selection.clone(),
            status: "requested".to_string(),
            requested_at: now,
            started_at: None,
            completed_at: None,
        };

        Self {
            thread_id,
            run_id,
            provider_instance_id,
            model_selection,
            current_run,
            ordinal_counter: 0,
            active_message_item_id: None,
            active_reasoning_item_id: None,
            active_compaction_item_id: None,
            active_items: HashMap::new(),
            tool_args: HashMap::new(),
        }
    }

    pub fn current_run(&self) -> &OrchestrationV2Run {
        &self.current_run
    }

    pub fn provider_instance_id(&self) -> &str {
        &self.provider_instance_id
    }

    pub fn model_selection(&self) -> &ModelSelection {
        &self.model_selection
    }

    pub fn active_items(&self) -> &HashMap<String, OrchestrationV2TurnItem> {
        &self.active_items
    }

    fn next_ordinal(&mut self) -> u64 {
        self.ordinal_counter += 1;
        self.ordinal_counter
    }

    pub fn process_line(&mut self, line: &str) -> Result<Vec<DriverEvent>, PiError> {
        let event: PiRpcEvent = serde_json::from_str(line).map_err(PiError::Json)?;
        Ok(self.process_event(event))
    }

    pub fn process_event(&mut self, event: PiRpcEvent) -> Vec<DriverEvent> {
        let mut emitted = Vec::new();

        match event {
            PiRpcEvent::AgentStart => {
                self.current_run.status = "running".to_string();
                self.current_run.started_at = Some(Utc::now());
                emitted.push(DriverEvent::RunStarted(self.current_run.clone()));
            }

            PiRpcEvent::MessageStart { message } => {
                let role = message
                    .get("role")
                    .and_then(|r| r.as_str())
                    .unwrap_or("assistant");
                if role == "assistant" {
                    let ordinal = self.next_ordinal();
                    let id = format!("item-msg-{}", ordinal);
                    let item = OrchestrationV2TurnItem {
                        id: id.clone(),
                        thread_id: self.thread_id.clone(),
                        run_id: Some(self.run_id.clone()),
                        ordinal,
                        status: "running".to_string(),
                        item_type: "assistant_message".to_string(),
                        title: None,
                        text: Some(String::new()),
                        input: None,
                        output: None,
                        exit_code: None,
                        created_at: Some(Utc::now()),
                        updated_at: Utc::now(),
                    };
                    self.active_message_item_id = Some(id.clone());
                    self.active_items.insert(id, item.clone());
                    emitted.push(DriverEvent::TurnItemStarted(item));
                }
            }

            PiRpcEvent::MessageUpdate {
                assistant_message_event: Some(delta),
                ..
            } => match delta {
                AssistantMessageDelta::TextDelta {
                    delta: text_delta, ..
                } => {
                    let item_id = if let Some(id) = &self.active_message_item_id {
                        id.clone()
                    } else {
                        let ordinal = self.next_ordinal();
                        let id = format!("item-msg-{}", ordinal);
                        let new_item = OrchestrationV2TurnItem {
                            id: id.clone(),
                            thread_id: self.thread_id.clone(),
                            run_id: Some(self.run_id.clone()),
                            ordinal,
                            status: "running".to_string(),
                            item_type: "assistant_message".to_string(),
                            title: None,
                            text: Some(String::new()),
                            input: None,
                            output: None,
                            exit_code: None,
                            created_at: Some(Utc::now()),
                            updated_at: Utc::now(),
                        };
                        self.active_message_item_id = Some(id.clone());
                        self.active_items.insert(id.clone(), new_item.clone());
                        emitted.push(DriverEvent::TurnItemStarted(new_item));
                        id
                    };

                    if let Some(item) = self.active_items.get_mut(&item_id) {
                        if let Some(ref mut text) = item.text {
                            text.push_str(&text_delta);
                        } else {
                            item.text = Some(text_delta);
                        }
                        item.updated_at = Utc::now();
                        emitted.push(DriverEvent::TurnItemUpdated(item.clone()));
                    }
                }

                AssistantMessageDelta::ThinkingDelta {
                    delta: think_delta, ..
                } => {
                    let item_id = if let Some(id) = &self.active_reasoning_item_id {
                        id.clone()
                    } else {
                        let ordinal = self.next_ordinal();
                        let id = format!("item-reason-{}", ordinal);
                        let new_item = OrchestrationV2TurnItem {
                            id: id.clone(),
                            thread_id: self.thread_id.clone(),
                            run_id: Some(self.run_id.clone()),
                            ordinal,
                            status: "running".to_string(),
                            item_type: "reasoning".to_string(),
                            title: Some("Reasoning".to_string()),
                            text: Some(String::new()),
                            input: None,
                            output: None,
                            exit_code: None,
                            created_at: Some(Utc::now()),
                            updated_at: Utc::now(),
                        };
                        self.active_reasoning_item_id = Some(id.clone());
                        self.active_items.insert(id.clone(), new_item.clone());
                        emitted.push(DriverEvent::TurnItemStarted(new_item));
                        id
                    };

                    if let Some(item) = self.active_items.get_mut(&item_id) {
                        if let Some(ref mut text) = item.text {
                            text.push_str(&think_delta);
                        } else {
                            item.text = Some(think_delta);
                        }
                        item.updated_at = Utc::now();
                        emitted.push(DriverEvent::TurnItemUpdated(item.clone()));
                    }
                }

                AssistantMessageDelta::TextEnd { content, .. } => {
                    if let Some(id) = self.active_message_item_id.take() {
                        if let Some(mut item) = self.active_items.remove(&id) {
                            if let Some(final_content) = content {
                                item.text = Some(final_content);
                            }
                            item.status = "completed".to_string();
                            item.updated_at = Utc::now();
                            emitted.push(DriverEvent::TurnItemCompleted(item));
                        }
                    }
                }

                AssistantMessageDelta::ThinkingEnd { thinking, .. } => {
                    if let Some(id) = self.active_reasoning_item_id.take() {
                        if let Some(mut item) = self.active_items.remove(&id) {
                            if let Some(final_thinking) = thinking {
                                item.text = Some(final_thinking);
                            }
                            item.status = "completed".to_string();
                            item.updated_at = Utc::now();
                            emitted.push(DriverEvent::TurnItemCompleted(item));
                        }
                    }
                }

                AssistantMessageDelta::Other => {}
            },

            PiRpcEvent::MessageEnd { message } => {
                let stop_reason = message.get("stopReason").and_then(|r| r.as_str());
                if let Some(id) = self.active_message_item_id.take() {
                    if let Some(mut item) = self.active_items.remove(&id) {
                        item.status = if stop_reason == Some("error") {
                            "failed".to_string()
                        } else {
                            "completed".to_string()
                        };
                        item.updated_at = Utc::now();
                        emitted.push(DriverEvent::TurnItemCompleted(item));
                    }
                }
            }

            PiRpcEvent::ToolExecutionStart {
                tool_call_id,
                tool_name,
                args,
                ..
            } => {
                let input_str = if let Some(ref a) = args {
                    if let Some(cmd) = a.get("command").and_then(|c| c.as_str()) {
                        cmd.to_string()
                    } else if let Some(s) = a.as_str() {
                        s.to_string()
                    } else {
                        a.to_string()
                    }
                } else {
                    String::new()
                };

                if let Some(a) = args {
                    self.tool_args.insert(tool_call_id.clone(), a);
                }

                let ordinal = self.next_ordinal();
                let item = OrchestrationV2TurnItem {
                    id: tool_call_id.clone(),
                    thread_id: self.thread_id.clone(),
                    run_id: Some(self.run_id.clone()),
                    ordinal,
                    status: "running".to_string(),
                    item_type: "command_execution".to_string(),
                    title: Some(tool_name),
                    text: None,
                    input: Some(input_str),
                    output: None,
                    exit_code: None,
                    created_at: Some(Utc::now()),
                    updated_at: Utc::now(),
                };

                self.active_items.insert(tool_call_id, item.clone());
                emitted.push(DriverEvent::TurnItemStarted(item));
            }

            PiRpcEvent::ToolExecutionUpdate {
                tool_call_id,
                partial_result,
            } => {
                if let Some(item) = self.active_items.get_mut(&tool_call_id) {
                    if let Some(result) = partial_result {
                        let text = extract_content_text(&result);
                        if !text.is_empty() {
                            item.output = Some(text);
                            item.updated_at = Utc::now();
                            emitted.push(DriverEvent::TurnItemUpdated(item.clone()));
                        }
                    }
                }
            }

            PiRpcEvent::ToolExecutionEnd {
                tool_call_id,
                is_error,
                result,
                ..
            } => {
                if let Some(mut item) = self.active_items.remove(&tool_call_id) {
                    if let Some(ref res) = result {
                        let text = extract_content_text(res);
                        if !text.is_empty() {
                            item.output = Some(text);
                        }
                        item.exit_code = extract_exit_code(res);
                    }
                    item.status = if is_error {
                        "failed".to_string()
                    } else {
                        "completed".to_string()
                    };
                    item.updated_at = Utc::now();
                    emitted.push(DriverEvent::TurnItemCompleted(item));
                }
            }

            PiRpcEvent::CompactionStart => {
                let ordinal = self.next_ordinal();
                let id = format!("item-compact-{}", ordinal);
                let item = OrchestrationV2TurnItem {
                    id: id.clone(),
                    thread_id: self.thread_id.clone(),
                    run_id: Some(self.run_id.clone()),
                    ordinal,
                    status: "running".to_string(),
                    item_type: "compaction".to_string(),
                    title: Some("compact".to_string()),
                    text: None,
                    input: None,
                    output: None,
                    exit_code: None,
                    created_at: Some(Utc::now()),
                    updated_at: Utc::now(),
                };
                self.active_compaction_item_id = Some(id.clone());
                self.active_items.insert(id, item.clone());
                emitted.push(DriverEvent::TurnItemStarted(item));
            }

            PiRpcEvent::CompactionEnd {
                aborted,
                error_message,
                result,
            } => {
                if let Some(id) = self.active_compaction_item_id.take() {
                    if let Some(mut item) = self.active_items.remove(&id) {
                        item.status = if aborted || error_message.is_some() {
                            "failed".to_string()
                        } else {
                            "completed".to_string()
                        };
                        if let Some(err) = error_message {
                            item.output = Some(err);
                        } else if let Some(res) = result {
                            let summary = res
                                .get("summary")
                                .and_then(|s| s.as_str())
                                .map(|s| s.to_string());
                            if summary.is_some() {
                                item.output = summary;
                            }
                        }
                        item.updated_at = Utc::now();
                        emitted.push(DriverEvent::TurnItemCompleted(item));
                    }
                }
            }

            PiRpcEvent::AgentSettled { aborted } => {
                self.current_run.status = if aborted {
                    "interrupted".to_string()
                } else {
                    "completed".to_string()
                };
                self.current_run.completed_at = Some(Utc::now());

                // Complete any open items
                for (_, mut item) in self.active_items.drain() {
                    item.status = if aborted {
                        "interrupted".to_string()
                    } else {
                        "completed".to_string()
                    };
                    item.updated_at = Utc::now();
                    emitted.push(DriverEvent::TurnItemCompleted(item));
                }

                emitted.push(DriverEvent::RunCompleted(self.current_run.clone()));
            }

            PiRpcEvent::AutoRetryStart { error_message, .. } => {
                let ordinal = self.next_ordinal();
                let id = format!("item-retry-{}", ordinal);
                let item = OrchestrationV2TurnItem {
                    id: id.clone(),
                    thread_id: self.thread_id.clone(),
                    run_id: Some(self.run_id.clone()),
                    ordinal,
                    status: "running".to_string(),
                    item_type: "retry".to_string(),
                    title: Some("retry".to_string()),
                    text: error_message,
                    input: None,
                    output: None,
                    exit_code: None,
                    created_at: Some(Utc::now()),
                    updated_at: Utc::now(),
                };
                self.active_items.insert(id, item.clone());
                emitted.push(DriverEvent::TurnItemStarted(item));
            }

            PiRpcEvent::AutoRetryEnd {
                success,
                final_error,
                ..
            } => {
                let mut retry_ids: Vec<String> = self
                    .active_items
                    .iter()
                    .filter(|(_, it)| it.item_type == "retry")
                    .map(|(k, _)| k.clone())
                    .collect();

                for id in retry_ids.drain(..) {
                    if let Some(mut item) = self.active_items.remove(&id) {
                        item.status = if success {
                            "completed".to_string()
                        } else {
                            "failed".to_string()
                        };
                        if let Some(ref err) = final_error {
                            item.output = Some(err.clone());
                        }
                        item.updated_at = Utc::now();
                        emitted.push(DriverEvent::TurnItemCompleted(item));
                    }
                }
            }

            _ => {}
        }

        emitted
    }
}

fn extract_content_text(val: &serde_json::Value) -> String {
    if let Some(content) = val.get("content") {
        if let Some(arr) = content.as_array() {
            let mut text = String::new();
            for item in arr {
                if let Some(t) = item.get("text").and_then(|s| s.as_str()) {
                    text.push_str(t);
                }
            }
            return text;
        } else if let Some(s) = content.as_str() {
            return s.to_string();
        }
    }
    if let Some(s) = val.as_str() {
        return s.to_string();
    }
    String::new()
}

fn extract_exit_code(val: &serde_json::Value) -> Option<i32> {
    val.get("details")
        .and_then(|d| d.get("exitCode").or_else(|| d.get("exit_code")))
        .and_then(|c| c.as_i64())
        .map(|c| c as i32)
        .or_else(|| {
            val.get("structuredContent")
                .and_then(|sc| sc.get("exit_code").or_else(|| sc.get("exitCode")))
                .and_then(|c| c.as_i64())
                .map(|c| c as i32)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_translator() -> PiEventTranslator {
        let model = ModelSelection {
            instance_id: "pi".to_string(),
            model: "claude-3-5-sonnet".to_string(),
            options: None,
        };
        PiEventTranslator::new("thread-123", "run-456", "pi", model)
    }

    #[test]
    fn test_agent_start_run_started() {
        let mut translator = setup_translator();
        let events = translator.process_event(PiRpcEvent::AgentStart);
        assert_eq!(events.len(), 1);
        match &events[0] {
            DriverEvent::RunStarted(run) => {
                assert_eq!(run.id, "run-456");
                assert_eq!(run.status, "running");
                assert!(run.started_at.is_some());
            }
            _ => panic!("Expected RunStarted event"),
        }
    }

    #[test]
    fn test_assistant_message_streaming() {
        let mut translator = setup_translator();

        // 1. message_start
        let ev1 = translator.process_event(PiRpcEvent::MessageStart {
            message: serde_json::json!({ "role": "assistant" }),
        });
        assert_eq!(ev1.len(), 1);
        let item_id = match &ev1[0] {
            DriverEvent::TurnItemStarted(item) => {
                assert_eq!(item.item_type, "assistant_message");
                assert_eq!(item.status, "running");
                item.id.clone()
            }
            _ => panic!("Expected TurnItemStarted"),
        };

        // 2. message_update with text_delta 1
        let ev2 = translator.process_event(PiRpcEvent::MessageUpdate {
            assistant_message_event: Some(AssistantMessageDelta::TextDelta {
                delta: "Hello ".to_string(),
                content_index: Some(0),
            }),
            usage: None,
        });
        assert_eq!(ev2.len(), 1);
        match &ev2[0] {
            DriverEvent::TurnItemUpdated(item) => {
                assert_eq!(item.id, item_id);
                assert_eq!(item.text.as_deref(), Some("Hello "));
            }
            _ => panic!("Expected TurnItemUpdated"),
        }

        // 3. message_update with text_delta 2
        let ev3 = translator.process_event(PiRpcEvent::MessageUpdate {
            assistant_message_event: Some(AssistantMessageDelta::TextDelta {
                delta: "world!".to_string(),
                content_index: Some(0),
            }),
            usage: None,
        });
        assert_eq!(ev3.len(), 1);
        match &ev3[0] {
            DriverEvent::TurnItemUpdated(item) => {
                assert_eq!(item.id, item_id);
                assert_eq!(item.text.as_deref(), Some("Hello world!"));
            }
            _ => panic!("Expected TurnItemUpdated"),
        }

        // 4. message_end
        let ev4 = translator.process_event(PiRpcEvent::MessageEnd {
            message: serde_json::json!({ "role": "assistant", "stopReason": "stop" }),
        });
        assert_eq!(ev4.len(), 1);
        match &ev4[0] {
            DriverEvent::TurnItemCompleted(item) => {
                assert_eq!(item.id, item_id);
                assert_eq!(item.status, "completed");
                assert_eq!(item.text.as_deref(), Some("Hello world!"));
            }
            _ => panic!("Expected TurnItemCompleted"),
        }
    }

    #[test]
    fn test_tool_execution_translation() {
        let mut translator = setup_translator();

        // 1. Tool execution start
        let ev1 = translator.process_event(PiRpcEvent::ToolExecutionStart {
            tool_call_id: "call_abc123".to_string(),
            tool_name: "bash".to_string(),
            args: Some(serde_json::json!({ "command": "cargo test" })),
            parent_tool_call_id: None,
        });
        assert_eq!(ev1.len(), 1);
        match &ev1[0] {
            DriverEvent::TurnItemStarted(item) => {
                assert_eq!(item.id, "call_abc123");
                assert_eq!(item.item_type, "command_execution");
                assert_eq!(item.title.as_deref(), Some("bash"));
                assert_eq!(item.input.as_deref(), Some("cargo test"));
                assert_eq!(item.status, "running");
            }
            _ => panic!("Expected TurnItemStarted"),
        }

        // 2. Tool execution update
        let ev2 = translator.process_event(PiRpcEvent::ToolExecutionUpdate {
            tool_call_id: "call_abc123".to_string(),
            partial_result: Some(serde_json::json!({
                "content": [{ "type": "text", "text": "running 1 test..." }]
            })),
        });
        assert_eq!(ev2.len(), 1);
        match &ev2[0] {
            DriverEvent::TurnItemUpdated(item) => {
                assert_eq!(item.id, "call_abc123");
                assert_eq!(item.output.as_deref(), Some("running 1 test..."));
            }
            _ => panic!("Expected TurnItemUpdated"),
        }

        // 3. Tool execution end
        let ev3 = translator.process_event(PiRpcEvent::ToolExecutionEnd {
            tool_call_id: "call_abc123".to_string(),
            tool_name: Some("bash".to_string()),
            is_error: false,
            result: Some(serde_json::json!({
                "content": [{ "type": "text", "text": "test result: ok" }],
                "details": { "exitCode": 0 }
            })),
        });
        assert_eq!(ev3.len(), 1);
        match &ev3[0] {
            DriverEvent::TurnItemCompleted(item) => {
                assert_eq!(item.id, "call_abc123");
                assert_eq!(item.status, "completed");
                assert_eq!(item.output.as_deref(), Some("test result: ok"));
                assert_eq!(item.exit_code, Some(0));
            }
            _ => panic!("Expected TurnItemCompleted"),
        }
    }

    #[test]
    fn test_compaction_translation() {
        let mut translator = setup_translator();

        let ev1 = translator.process_event(PiRpcEvent::CompactionStart);
        assert_eq!(ev1.len(), 1);
        let comp_id = match &ev1[0] {
            DriverEvent::TurnItemStarted(item) => {
                assert_eq!(item.item_type, "compaction");
                assert_eq!(item.status, "running");
                item.id.clone()
            }
            _ => panic!("Expected TurnItemStarted"),
        };

        let ev2 = translator.process_event(PiRpcEvent::CompactionEnd {
            aborted: false,
            error_message: None,
            result: Some(serde_json::json!({
                "summary": "Compacted 10 messages into 1 summary"
            })),
        });
        assert_eq!(ev2.len(), 1);
        match &ev2[0] {
            DriverEvent::TurnItemCompleted(item) => {
                assert_eq!(item.id, comp_id);
                assert_eq!(item.status, "completed");
                assert_eq!(
                    item.output.as_deref(),
                    Some("Compacted 10 messages into 1 summary")
                );
            }
            _ => panic!("Expected TurnItemCompleted"),
        }
    }

    #[test]
    fn test_agent_settled_run_completed() {
        let mut translator = setup_translator();
        let _ = translator.process_event(PiRpcEvent::AgentStart);
        let events = translator.process_event(PiRpcEvent::AgentSettled { aborted: false });
        assert_eq!(events.len(), 1);
        match &events[0] {
            DriverEvent::RunCompleted(run) => {
                assert_eq!(run.id, "run-456");
                assert_eq!(run.status, "completed");
                assert!(run.completed_at.is_some());
            }
            _ => panic!("Expected RunCompleted"),
        }
    }

    #[test]
    fn test_agent_settled_aborted() {
        let mut translator = setup_translator();
        let _ = translator.process_event(PiRpcEvent::AgentStart);
        let events = translator.process_event(PiRpcEvent::AgentSettled { aborted: true });
        assert_eq!(events.len(), 1);
        match &events[0] {
            DriverEvent::RunCompleted(run) => {
                assert_eq!(run.id, "run-456");
                assert_eq!(run.status, "interrupted");
            }
            _ => panic!("Expected RunCompleted with interrupted"),
        }
    }
}
