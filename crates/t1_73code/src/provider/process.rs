use crate::protocol::models::{ModelSelection, ServerProviderModel};
use crate::provider::error::PiError;
use crate::provider::events::{DriverEvent, PiEventTranslator};
use crate::provider::framer::JsonlFramer;
use crate::provider::models::{
    Attachment, PiCommandsData, PiDiscoveredCommands, PiImageAttachment, PiModelsData, PiRpcEvent,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, Command};
use tokio::sync::{broadcast, mpsc, oneshot, Mutex};

pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
pub const DEFAULT_TERMINATION_GRACE: Duration = Duration::from_millis(1000);

type PendingRequestsMap =
    Arc<Mutex<HashMap<String, oneshot::Sender<Result<serde_json::Value, PiError>>>>>;

#[derive(Debug, Clone)]
pub struct PiProcessConfig {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub env: HashMap<String, String>,
    pub max_record_bytes: usize,
    pub request_timeout: Duration,
    pub thread_id: Option<String>,
    pub run_id: Option<String>,
    pub provider_instance_id: Option<String>,
    pub model_selection: Option<ModelSelection>,
}

impl Default for PiProcessConfig {
    fn default() -> Self {
        Self {
            program: PathBuf::from("pi"),
            args: Vec::new(),
            cwd: None,
            env: HashMap::new(),
            max_record_bytes: 16 * 1024 * 1024,
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
            thread_id: None,
            run_id: None,
            provider_instance_id: None,
            model_selection: None,
        }
    }
}

pub struct PiProcess {
    config: PiProcessConfig,
    writer_tx: mpsc::Sender<String>,
    pending_requests: PendingRequestsMap,
    request_counter: AtomicU64,
    event_tx: broadcast::Sender<DriverEvent>,
    child_handle: Arc<Mutex<Option<Child>>>,
    is_running: Arc<AtomicBool>,
    pid: Option<u32>,
}

impl PiProcess {
    pub async fn spawn(config: PiProcessConfig) -> Result<Self, PiError> {
        let mut cmd = Command::new(&config.program);
        cmd.arg("--mode").arg("rpc");
        for arg in &config.args {
            cmd.arg(arg);
        }
        if let Some(cwd) = &config.cwd {
            cmd.current_dir(cwd);
        }
        for (k, v) in &config.env {
            cmd.env(k, v);
        }

        #[cfg(unix)]
        {
            cmd.process_group(0);
        }

        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(PiError::Spawn)?;
        let pid = child.id();

        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| PiError::Io(std::io::Error::other("Failed to capture stdin")))?;
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| PiError::Io(std::io::Error::other("Failed to capture stdout")))?;
        let mut stderr = child.stderr.take();

        let (writer_tx, mut writer_rx) = mpsc::channel::<String>(128);
        let (event_tx, _) = broadcast::channel(1024);

        let pending_requests = Arc::new(Mutex::new(HashMap::<
            String,
            oneshot::Sender<Result<serde_json::Value, PiError>>,
        >::new()));
        let is_running = Arc::new(AtomicBool::new(true));

        // Background stdin writer task
        let is_running_writer = is_running.clone();
        tokio::spawn(async move {
            while let Some(line) = writer_rx.recv().await {
                if !is_running_writer.load(Ordering::SeqCst) {
                    break;
                }
                if let Err(e) = stdin.write_all(line.as_bytes()).await {
                    tracing::debug!("Failed to write to Pi stdin: {e}");
                    break;
                }
                if let Err(e) = stdin.flush().await {
                    tracing::debug!("Failed to flush Pi stdin: {e}");
                    break;
                }
            }
        });

        // Background stderr drain task
        if let Some(mut err_reader) = stderr.take() {
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                use tokio::io::AsyncReadExt;
                while let Ok(n) = err_reader.read(&mut buf).await {
                    if n == 0 {
                        break;
                    }
                }
            });
        }

        // Background stdout reader task
        let pending_clone = pending_requests.clone();
        let event_tx_clone = event_tx.clone();
        let is_running_reader = is_running.clone();
        let max_record_bytes = config.max_record_bytes;

        let thread_id = config
            .thread_id
            .clone()
            .unwrap_or_else(|| "default-thread".to_string());
        let run_id = config
            .run_id
            .clone()
            .unwrap_or_else(|| "default-run".to_string());
        let provider_instance_id = config
            .provider_instance_id
            .clone()
            .unwrap_or_else(|| "pi".to_string());
        let model_selection = config
            .model_selection
            .clone()
            .unwrap_or_else(|| ModelSelection {
                instance_id: provider_instance_id.clone(),
                model: "default".to_string(),
                options: None,
            });

        let translator = Arc::new(Mutex::new(PiEventTranslator::new(
            thread_id,
            run_id,
            provider_instance_id,
            model_selection,
        )));

        tokio::spawn(async move {
            let mut framer = JsonlFramer::new(max_record_bytes);
            loop {
                match framer.read_line(&mut stdout).await {
                    Ok(Some(line)) => {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }

                        if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
                            if value.get("type").and_then(|t| t.as_str()) == Some("response") {
                                if let Some(id) = value.get("id").and_then(|i| i.as_str()) {
                                    let mut pending = pending_clone.lock().await;
                                    if let Some(tx) = pending.remove(id) {
                                        let success = value
                                            .get("success")
                                            .and_then(|s| s.as_bool())
                                            .unwrap_or(false);

                                        if success {
                                            let data = value
                                                .get("data")
                                                .cloned()
                                                .unwrap_or(serde_json::Value::Null);
                                            let _ = tx.send(Ok(data));
                                        } else {
                                            let detail = value
                                                .get("error")
                                                .map(|e| {
                                                    if let Some(s) = e.as_str() {
                                                        s.to_string()
                                                    } else {
                                                        e.to_string()
                                                    }
                                                })
                                                .unwrap_or_else(|| {
                                                    "Unknown Pi RPC error".to_string()
                                                });
                                            let op = value
                                                .get("command")
                                                .and_then(|c| c.as_str())
                                                .unwrap_or("request")
                                                .to_string();
                                            let _ = tx.send(Err(PiError::RpcError {
                                                operation: op,
                                                detail,
                                            }));
                                        }
                                    }
                                }
                                continue;
                            }

                            // Process streaming events
                            if let Ok(event) = serde_json::from_value::<PiRpcEvent>(value) {
                                let mut tr = translator.lock().await;
                                let driver_events = tr.process_event(event);
                                for ev in driver_events {
                                    let _ = event_tx_clone.send(ev);
                                }
                            }
                        }
                    }
                    Ok(None) => {
                        // EOF on stdout
                        break;
                    }
                    Err(e) => {
                        tracing::debug!("Error reading Pi stdout: {e}");
                        break;
                    }
                }
            }

            is_running_reader.store(false, Ordering::SeqCst);
            let mut pending = pending_clone.lock().await;
            for (_, tx) in pending.drain() {
                let _ = tx.send(Err(PiError::StdoutClosed));
            }
        });

        let child_handle = Arc::new(Mutex::new(Some(child)));

        Ok(Self {
            config,
            writer_tx,
            pending_requests,
            request_counter: AtomicU64::new(0),
            event_tx,
            child_handle,
            is_running,
            pid,
        })
    }

    pub fn config(&self) -> &PiProcessConfig {
        &self.config
    }

    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<DriverEvent> {
        self.event_tx.subscribe()
    }

    pub async fn request(
        &self,
        mut record: serde_json::Value,
        timeout: Option<Duration>,
    ) -> Result<serde_json::Value, PiError> {
        if !self.is_running() {
            return Err(PiError::ProcessExited {
                exit_code: None,
                detail: "Process is not running".to_string(),
            });
        }

        let id = format!("t3-{}", self.request_counter.fetch_add(1, Ordering::SeqCst));
        if let Some(obj) = record.as_object_mut() {
            obj.insert("id".to_string(), serde_json::Value::String(id.clone()));
        }

        let op_name = record
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("request")
            .to_string();

        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending_requests.lock().await;
            pending.insert(id.clone(), tx);
        }

        let json_str = serde_json::to_string(&record).map_err(PiError::Json)? + "\n";
        if self.writer_tx.send(json_str).await.is_err() {
            let mut pending = self.pending_requests.lock().await;
            pending.remove(&id);
            return Err(PiError::StdoutClosed);
        }

        let timeout_duration = timeout.unwrap_or(self.config.request_timeout);
        match tokio::time::timeout(timeout_duration, rx).await {
            Ok(Ok(res)) => res,
            Ok(Err(_)) => {
                let mut pending = self.pending_requests.lock().await;
                pending.remove(&id);
                Err(PiError::ProcessExited {
                    exit_code: None,
                    detail: "Pending request dropped before response".to_string(),
                })
            }
            Err(_) => {
                let mut pending = self.pending_requests.lock().await;
                pending.remove(&id);
                Err(PiError::Timeout {
                    operation: op_name,
                    timeout_ms: timeout_duration.as_millis() as u64,
                })
            }
        }
    }

    pub async fn send_prompt(
        &self,
        text: &str,
        attachments: &[Attachment],
    ) -> Result<serde_json::Value, PiError> {
        let mut images = Vec::new();
        let mut prompt_text = text.to_string();

        for att in attachments {
            if att.is_image() {
                if let Some(ref data) = att.data {
                    images.push(PiImageAttachment::new(data, &att.mime_type));
                }
            } else if let Some(ref path) = att.path {
                prompt_text.push_str(&format!("\n[Attached file: {}]", path));
            }
        }

        let mut req_map = serde_json::Map::new();
        req_map.insert("type".to_string(), serde_json::json!("prompt"));
        req_map.insert("message".to_string(), serde_json::json!(prompt_text));
        req_map.insert("streamingBehavior".to_string(), serde_json::json!("steer"));

        if !images.is_empty() {
            req_map.insert("images".to_string(), serde_json::to_value(images)?);
        }

        self.request(serde_json::Value::Object(req_map), None).await
    }

    pub async fn compact(&self, instructions: Option<&str>) -> Result<serde_json::Value, PiError> {
        let mut req_map = serde_json::Map::new();
        req_map.insert("type".to_string(), serde_json::json!("compact"));
        if let Some(inst) = instructions {
            req_map.insert("customInstructions".to_string(), serde_json::json!(inst));
        }

        self.request(serde_json::Value::Object(req_map), None).await
    }

    pub async fn abort(&self) -> Result<serde_json::Value, PiError> {
        let mut req_map = serde_json::Map::new();
        req_map.insert("type".to_string(), serde_json::json!("abort"));
        self.request(serde_json::Value::Object(req_map), None).await
    }

    pub async fn get_available_models(&self) -> Result<Vec<ServerProviderModel>, PiError> {
        let req = serde_json::json!({
            "type": "get_available_models"
        });
        let data = self.request(req, None).await?;
        let models_data: PiModelsData = serde_json::from_value(data).map_err(PiError::Json)?;

        let models = models_data
            .models
            .into_iter()
            .map(|m| m.to_server_provider_model())
            .collect();
        Ok(models)
    }

    pub async fn get_commands(&self) -> Result<PiDiscoveredCommands, PiError> {
        let req = serde_json::json!({
            "type": "get_commands"
        });
        let data = self.request(req, None).await?;
        let commands_data: PiCommandsData = serde_json::from_value(data).map_err(PiError::Json)?;

        Ok(commands_data.into_discovered_commands())
    }

    pub async fn terminate(&self) -> Result<(), PiError> {
        self.is_running.store(false, Ordering::SeqCst);

        // Fail all pending requests
        let mut pending = self.pending_requests.lock().await;
        for (_, tx) in pending.drain() {
            let _ = tx.send(Err(PiError::Cancelled));
        }

        let mut lock = self.child_handle.lock().await;
        if let Some(mut child) = lock.take() {
            #[cfg(unix)]
            if let Some(pid) = child.id() {
                // Send SIGTERM using standard kill
                let _ = Command::new("kill")
                    .args(["-TERM", &pid.to_string()])
                    .status()
                    .await;
            }

            // Await with grace period
            let wait_res = tokio::time::timeout(DEFAULT_TERMINATION_GRACE, child.wait()).await;
            if wait_res.is_err() {
                // Escalate to kill
                let _ = child.kill().await;
            }
        }

        Ok(())
    }
}

impl Drop for PiProcess {
    fn drop(&mut self) {
        self.is_running.store(false, Ordering::SeqCst);
        if let Ok(mut lock) = self.child_handle.try_lock() {
            if let Some(mut child) = lock.take() {
                let _ = child.start_kill();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn create_mock_pi_script(script_content: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempdir().unwrap();
        let script_path = dir.path().join(format!("mock_pi_{}.sh", uuid::Uuid::new_v4()));
        let content = format!("#!/bin/sh\n{}", script_content);
        {
            use std::io::Write;
            let mut file = std::fs::File::create(&script_path).unwrap();
            file.write_all(content.as_bytes()).unwrap();
            file.sync_all().unwrap();
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&script_path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&script_path, perms).unwrap();
        }

        (dir, script_path)
    }

    #[tokio::test]
    async fn test_process_mock_script_models_and_commands() {
        let script = r#"
while IFS= read -r line; do
  id=$(echo "$line" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
  type=$(echo "$line" | sed -n 's/.*"type":"\([^"]*\)".*/\1/p')

  if [ "$type" = "get_available_models" ]; then
    echo "{\"type\":\"response\",\"id\":\"$id\",\"command\":\"get_available_models\",\"success\":true,\"data\":{\"models\":[{\"provider\":\"anthropic\",\"id\":\"claude-3-5\",\"name\":\"Claude 3.5\"}]}}"
  elif [ "$type" = "get_commands" ]; then
    echo "{\"type\":\"response\",\"id\":\"$id\",\"command\":\"get_commands\",\"success\":true,\"data\":{\"commands\":[{\"name\":\"test-cmd\",\"description\":\"A test command\"}]}}"
  elif [ "$type" = "prompt" ]; then
    echo "{\"type\":\"response\",\"id\":\"$id\",\"command\":\"prompt\",\"success\":true,\"data\":{}}"
    echo "{\"type\":\"agent_start\"}"
    echo "{\"type\":\"message_start\",\"message\":{\"role\":\"assistant\"}}"
    echo "{\"type\":\"message_update\",\"assistantMessageEvent\":{\"type\":\"text_delta\",\"delta\":\"Hi there\"}}"
    echo "{\"type\":\"message_end\",\"message\":{\"role\":\"assistant\",\"stopReason\":\"stop\"}}"
    echo "{\"type\":\"agent_settled\",\"aborted\":false}"
  elif [ "$type" = "abort" ]; then
    echo "{\"type\":\"response\",\"id\":\"$id\",\"command\":\"abort\",\"success\":true,\"data\":{}}"
  fi
done
"#;
        let (_dir, script_path) = create_mock_pi_script(script);
        let mut config = PiProcessConfig::default();
        config.program = script_path;

        let process = PiProcess::spawn(config).await.unwrap();
        assert!(process.is_running());

        // 1. Test get_available_models
        let models = process.get_available_models().await.unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "anthropic/claude-3-5");
        assert_eq!(models[0].label, Some("Claude 3.5".to_string()));

        // 2. Test get_commands
        let commands = process.get_commands().await.unwrap();
        // Builtin compact is added
        assert_eq!(commands.slash_commands.len(), 2);
        assert_eq!(commands.slash_commands[0].name, "compact");
        assert_eq!(commands.slash_commands[1].name, "test-cmd");

        // 3. Test send_prompt and event reception
        let mut events_rx = process.subscribe();
        let prompt_res = process.send_prompt("Hello", &[]).await.unwrap();
        assert!(prompt_res.is_object());

        let mut received_turn_items = Vec::new();
        let mut received_run_completed = false;

        let timeout = Duration::from_secs(2);
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            if let Ok(ev) = events_rx.try_recv() {
                match ev {
                    DriverEvent::TurnItemCompleted(it) => {
                        received_turn_items.push(it);
                    }
                    DriverEvent::RunCompleted(_) => {
                        received_run_completed = true;
                        break;
                    }
                    _ => {}
                }
            } else {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }

        assert!(received_run_completed);
        assert_eq!(received_turn_items.len(), 1);
        assert_eq!(received_turn_items[0].text.as_deref(), Some("Hi there"));

        // 4. Test abort
        let abort_res = process.abort().await.unwrap();
        assert!(abort_res.is_object());

        // 5. Terminate
        process.terminate().await.unwrap();
        assert!(!process.is_running());
    }

    #[tokio::test]
    async fn test_process_timeout() {
        let script = r#"
while IFS= read -r line; do
  sleep 5
done
"#;
        let (_dir, script_path) = create_mock_pi_script(script);
        let mut config = PiProcessConfig::default();
        config.program = script_path;
        config.request_timeout = Duration::from_millis(50);

        let process = PiProcess::spawn(config).await.unwrap();
        let result = process.abort().await;
        match result {
            Err(PiError::Timeout {
                operation,
                timeout_ms,
            }) => {
                assert_eq!(operation, "abort");
                assert_eq!(timeout_ms, 50);
            }
            other => panic!("Expected Timeout error, got {:?}", other),
        }

        let _ = process.terminate().await;
    }

    #[tokio::test]
    async fn test_process_unexpected_exit() {
        let script = r#"
exit 42
"#;
        let (_dir, script_path) = create_mock_pi_script(script);
        let mut config = PiProcessConfig::default();
        config.program = script_path;
        config.request_timeout = Duration::from_millis(500);

        let process = PiProcess::spawn(config).await.unwrap();
        // Give the process a moment to exit
        tokio::time::sleep(Duration::from_millis(100)).await;

        let result = process.abort().await;
        assert!(result.is_err());

        let _ = process.terminate().await;
    }
}
