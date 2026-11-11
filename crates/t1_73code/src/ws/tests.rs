use std::collections::HashMap;
use std::sync::Arc;
use tempfile::tempdir;
use tokio::sync::{mpsc, Mutex};

use crate::protocol::models::*;
use crate::protocol::rpc::{FromClientMessage, FromServerMessage, RpcExit};
use crate::state::AppState;
use crate::storage::Storage;
use super::dispatch::handle_dispatch_command;
use super::process_manager::ProcessManager;
use super::session::process_client_message;

fn setup_test_app() -> (tempfile::TempDir, Arc<AppState>, ProcessManager) {
    let dir = tempdir().unwrap();
    let storage = Arc::new(Storage::open_in_memory().unwrap());
    let state = Arc::new(AppState::new(dir.path().to_path_buf(), storage));
    let proc_mgr = ProcessManager::new();
    (dir, state, proc_mgr)
}

#[tokio::test]
async fn test_ping_pong_message() {
    let (_dir, state, proc_mgr) = setup_test_app();
    let (out_tx, mut out_rx) = mpsc::channel(10);
    let subs = Arc::new(Mutex::new(HashMap::new()));

    process_client_message(FromClientMessage::Ping, &state, &out_tx, &subs, &proc_mgr).await;

    let reply = out_rx.recv().await.expect("Expected Pong reply");
    match reply {
        FromServerMessage::Pong => {}
        other => panic!("Expected Pong, got {:?}", other),
    }
}

#[tokio::test]
async fn test_ping_tag_request() {
    let (_dir, state, proc_mgr) = setup_test_app();
    let (out_tx, mut out_rx) = mpsc::channel(10);
    let subs = Arc::new(Mutex::new(HashMap::new()));

    let req = FromClientMessage::Request {
        id: serde_json::json!("req-ping-1"),
        tag: "Ping".to_string(),
        payload: serde_json::json!({}),
        headers: vec![],
        discard: None,
    };

    process_client_message(req, &state, &out_tx, &subs, &proc_mgr).await;

    let reply = out_rx.recv().await.expect("Expected Pong reply");
    match reply {
        FromServerMessage::Pong => {}
        other => panic!("Expected Pong, got {:?}", other),
    }
}

#[tokio::test]
async fn test_server_probe() {
    let (_dir, state, proc_mgr) = setup_test_app();
    let (out_tx, mut out_rx) = mpsc::channel(10);
    let subs = Arc::new(Mutex::new(HashMap::new()));

    let req = FromClientMessage::Request {
        id: serde_json::json!("probe-1"),
        tag: "server.probe".to_string(),
        payload: serde_json::json!({}),
        headers: vec![],
        discard: None,
    };

    process_client_message(req, &state, &out_tx, &subs, &proc_mgr).await;

    let reply = out_rx.recv().await.expect("Expected response");
    match reply {
        FromServerMessage::Exit { request_id, exit } => {
            assert_eq!(request_id, serde_json::json!("probe-1"));
            match exit {
                RpcExit::Success { value } => {
                    assert_eq!(value, serde_json::json!({}));
                }
                RpcExit::Failure { .. } => panic!("Expected success exit"),
            }
        }
        other => panic!("Expected Exit, got {:?}", other),
    }
}

#[tokio::test]
async fn test_server_get_config() {
    let (_dir, state, proc_mgr) = setup_test_app();
    let (out_tx, mut out_rx) = mpsc::channel(10);
    let subs = Arc::new(Mutex::new(HashMap::new()));

    let req = FromClientMessage::Request {
        id: serde_json::json!("config-1"),
        tag: "server.getConfig".to_string(),
        payload: serde_json::json!({}),
        headers: vec![],
        discard: None,
    };

    process_client_message(req, &state, &out_tx, &subs, &proc_mgr).await;

    let reply = out_rx.recv().await.expect("Expected response");
    match reply {
        FromServerMessage::Exit { request_id, exit } => {
            assert_eq!(request_id, serde_json::json!("config-1"));
            match exit {
                RpcExit::Success { value } => {
                    let config: ServerConfig = serde_json::from_value(value).unwrap();
                    assert_eq!(config.environment.environment_id, state.environment_id);
                    assert_eq!(config.environment.server_version, "0.1.73");
                    assert_eq!(config.providers.len(), 1);
                    assert_eq!(config.providers[0].instance_id, "pi");
                }
                RpcExit::Failure { .. } => panic!("Expected success exit"),
            }
        }
        other => panic!("Expected Exit, got {:?}", other),
    }
}

#[tokio::test]
async fn test_server_refresh_providers() {
    let (_dir, state, proc_mgr) = setup_test_app();
    let (out_tx, mut out_rx) = mpsc::channel(10);
    let subs = Arc::new(Mutex::new(HashMap::new()));

    let req = FromClientMessage::Request {
        id: serde_json::json!("providers-1"),
        tag: "server.refreshProviders".to_string(),
        payload: serde_json::json!({}),
        headers: vec![],
        discard: None,
    };

    process_client_message(req, &state, &out_tx, &subs, &proc_mgr).await;

    let reply = out_rx.recv().await.expect("Expected response");
    match reply {
        FromServerMessage::Exit { request_id, exit } => {
            assert_eq!(request_id, serde_json::json!("providers-1"));
            match exit {
                RpcExit::Success { value } => {
                    let providers_val = value.get("providers").expect("Expected providers field");
                    let providers: Vec<ServerProvider> =
                        serde_json::from_value(providers_val.clone()).unwrap();
                    assert_eq!(providers.len(), 1);
                    assert_eq!(providers[0].instance_id, "pi");
                    assert_eq!(providers[0].driver, "pi");
                }
                RpcExit::Failure { .. } => panic!("Expected success exit"),
            }
        }
        other => panic!("Expected Exit, got {:?}", other),
    }
}

#[tokio::test]
async fn test_subscribe_server_config_stream_and_interrupt() {
    let (_dir, state, proc_mgr) = setup_test_app();
    let (out_tx, mut out_rx) = mpsc::channel(10);
    let subs = Arc::new(Mutex::new(HashMap::new()));

    let req = FromClientMessage::Request {
        id: serde_json::json!("sub-cfg-1"),
        tag: "subscribeServerConfig".to_string(),
        payload: serde_json::json!({}),
        headers: vec![],
        discard: None,
    };

    process_client_message(req, &state, &out_tx, &subs, &proc_mgr).await;

    let chunk = out_rx.recv().await.expect("Expected snapshot chunk");
    match chunk {
        FromServerMessage::Chunk { request_id, values } => {
            assert_eq!(request_id, serde_json::json!("sub-cfg-1"));
            assert_eq!(values.len(), 1);
            let first_val = &values[0];
            assert_eq!(first_val.get("type").and_then(|v| v.as_str()), Some("snapshot"));
            assert!(first_val.get("config").is_some());
        }
        other => panic!("Expected Chunk, got {:?}", other),
    }

    // Now send interrupt
    assert_eq!(subs.lock().await.len(), 1);
    let interrupt = FromClientMessage::Interrupt {
        request_id: serde_json::json!("sub-cfg-1"),
    };
    process_client_message(interrupt, &state, &out_tx, &subs, &proc_mgr).await;
    assert_eq!(subs.lock().await.len(), 0);
}

#[tokio::test]
async fn test_subscribe_shell_snapshot_and_live_updates() {
    let (_dir, state, proc_mgr) = setup_test_app();
    let (out_tx, mut out_rx) = mpsc::channel(10);
    let subs = Arc::new(Mutex::new(HashMap::new()));

    // Create a project and thread first
    let project = state.storage.get_or_create_project("/test/path", "Test Project").unwrap();
    let thread = OrchestrationV2ThreadShell {
        id: "thread-shell-test".to_string(),
        project_id: project.id.clone(),
        title: "Test Thread Shell".to_string(),
        provider_instance_id: "pi".to_string(),
        model_selection: ModelSelection {
            instance_id: "pi".to_string(),
            model: "default".to_string(),
            options: None,
        },
        runtime_mode: "autonomous".to_string(),
        interaction_mode: "chat".to_string(),
        branch: None,
        worktree_path: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        archived_at: None,
        deleted_at: None,
        status: Some("idle".to_string()),
        latest_run_id: None,
        item_count: 0,
    };
    state.storage.create_thread(thread.clone()).unwrap();

    let req = FromClientMessage::Request {
        id: serde_json::json!("sub-shell-1"),
        tag: "orchestration.subscribeShell".to_string(),
        payload: serde_json::json!({ "requestCompletionMarker": true }),
        headers: vec![],
        discard: None,
    };

    process_client_message(req, &state, &out_tx, &subs, &proc_mgr).await;

    // First chunk should be Snapshot
    let chunk1 = out_rx.recv().await.expect("Expected snapshot chunk");
    match chunk1 {
        FromServerMessage::Chunk { request_id, values } => {
            assert_eq!(request_id, serde_json::json!("sub-shell-1"));
            assert_eq!(values.len(), 1);
            let item: OrchestrationV2ShellStreamItem =
                serde_json::from_value(values[0].clone()).unwrap();
            match item {
                OrchestrationV2ShellStreamItem::Snapshot { snapshot } => {
                    assert_eq!(snapshot.projects.len(), 1);
                    assert_eq!(snapshot.threads.len(), 1);
                    assert_eq!(snapshot.threads[0].id, "thread-shell-test");
                }
                other => panic!("Expected Snapshot, got {:?}", other),
            }
        }
        other => panic!("Expected Chunk, got {:?}", other),
    }

    // Second chunk should be Synchronized (completion marker)
    let chunk2 = out_rx.recv().await.expect("Expected synchronized chunk");
    match chunk2 {
        FromServerMessage::Chunk { request_id, values } => {
            assert_eq!(request_id, serde_json::json!("sub-shell-1"));
            let item: OrchestrationV2ShellStreamItem =
                serde_json::from_value(values[0].clone()).unwrap();
            match item {
                OrchestrationV2ShellStreamItem::Synchronized => {}
                other => panic!("Expected Synchronized, got {:?}", other),
            }
        }
        other => panic!("Expected Chunk, got {:?}", other),
    }

    // Now emit a live shell update via dispatchCommand thread.create
    let create_cmd = serde_json::json!({
        "type": "thread.create",
        "commandId": "cmd-1",
        "threadId": "thread-live-2",
        "projectId": project.id,
        "title": "Live Created Thread",
    });
    let seq = handle_dispatch_command(&state, create_cmd, &proc_mgr).await.unwrap();
    assert!(seq > 0);

    // Shell subscriber should receive live ThreadUpdated event
    let chunk3 = out_rx.recv().await.expect("Expected live thread.updated chunk");
    match chunk3 {
        FromServerMessage::Chunk { request_id, values } => {
            assert_eq!(request_id, serde_json::json!("sub-shell-1"));
            let item: OrchestrationV2ShellStreamItem =
                serde_json::from_value(values[0].clone()).unwrap();
            match item {
                OrchestrationV2ShellStreamItem::ThreadUpdated { thread, .. } => {
                    assert_eq!(thread.id, "thread-live-2");
                    assert_eq!(thread.title, "Live Created Thread");
                }
                other => panic!("Expected ThreadUpdated, got {:?}", other),
            }
        }
        other => panic!("Expected Chunk, got {:?}", other),
    }

    // Clean up with Interrupt
    let interrupt = FromClientMessage::Interrupt {
        request_id: serde_json::json!("sub-shell-1"),
    };
    process_client_message(interrupt, &state, &out_tx, &subs, &proc_mgr).await;
    assert_eq!(subs.lock().await.len(), 0);
}

#[tokio::test]
async fn test_subscribe_thread_projection_and_live_events() {
    let (_dir, state, proc_mgr) = setup_test_app();
    let (out_tx, mut out_rx) = mpsc::channel(10);
    let subs = Arc::new(Mutex::new(HashMap::new()));

    // Create a project, thread, run, and turn item
    let project = state.storage.get_or_create_project("/test/path", "Test Project").unwrap();
    let thread = OrchestrationV2ThreadShell {
        id: "thread-sub-test".to_string(),
        project_id: project.id.clone(),
        title: "Test Thread".to_string(),
        provider_instance_id: "pi".to_string(),
        model_selection: ModelSelection {
            instance_id: "pi".to_string(),
            model: "default".to_string(),
            options: None,
        },
        runtime_mode: "autonomous".to_string(),
        interaction_mode: "chat".to_string(),
        branch: None,
        worktree_path: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        archived_at: None,
        deleted_at: None,
        status: Some("idle".to_string()),
        latest_run_id: None,
        item_count: 0,
    };
    state.storage.create_thread(thread).unwrap();

    let req = FromClientMessage::Request {
        id: serde_json::json!("sub-thread-1"),
        tag: "orchestration.subscribeThread".to_string(),
        payload: serde_json::json!({
            "threadId": "thread-sub-test",
            "requestCompletionMarker": true
        }),
        headers: vec![],
        discard: None,
    };

    process_client_message(req, &state, &out_tx, &subs, &proc_mgr).await;

    // Snapshot chunk
    let chunk1 = out_rx.recv().await.expect("Expected thread snapshot chunk");
    match chunk1 {
        FromServerMessage::Chunk { request_id, values } => {
            assert_eq!(request_id, serde_json::json!("sub-thread-1"));
            let item: OrchestrationV2ThreadStreamItem =
                serde_json::from_value(values[0].clone()).unwrap();
            match item {
                OrchestrationV2ThreadStreamItem::Snapshot { projection, .. } => {
                    assert_eq!(projection.thread.id, "thread-sub-test");
                }
                other => panic!("Expected Snapshot, got {:?}", other),
            }
        }
        other => panic!("Expected Chunk, got {:?}", other),
    }

    // Synchronized chunk
    let chunk2 = out_rx.recv().await.expect("Expected synchronized chunk");
    match chunk2 {
        FromServerMessage::Chunk { request_id, values } => {
            assert_eq!(request_id, serde_json::json!("sub-thread-1"));
            let item: OrchestrationV2ThreadStreamItem =
                serde_json::from_value(values[0].clone()).unwrap();
            match item {
                OrchestrationV2ThreadStreamItem::Synchronized => {}
                other => panic!("Expected Synchronized, got {:?}", other),
            }
        }
        other => panic!("Expected Chunk, got {:?}", other),
    }

    // Now emit a live event on thread_broadcast
    let live_item = OrchestrationV2ThreadStreamItem::Event {
        sequence: 42,
        event: serde_json::json!({ "type": "test_event", "hello": "world" }),
    };
    state
        .thread_broadcast
        .send(("thread-sub-test".to_string(), live_item))
        .unwrap();

    let chunk3 = out_rx.recv().await.expect("Expected live event chunk");
    match chunk3 {
        FromServerMessage::Chunk { request_id, values } => {
            assert_eq!(request_id, serde_json::json!("sub-thread-1"));
            let item: OrchestrationV2ThreadStreamItem =
                serde_json::from_value(values[0].clone()).unwrap();
            match item {
                OrchestrationV2ThreadStreamItem::Event { sequence, event } => {
                    assert_eq!(sequence, 42);
                    assert_eq!(event["type"], "test_event");
                }
                other => panic!("Expected Event, got {:?}", other),
            }
        }
        other => panic!("Expected Chunk, got {:?}", other),
    }
}

#[tokio::test]
async fn test_dispatch_command_thread_lifecycle() {
    let (_dir, state, proc_mgr) = setup_test_app();
    let (out_tx, mut out_rx) = mpsc::channel(10);
    let subs = Arc::new(Mutex::new(HashMap::new()));

    // 1. thread.create
    let create_cmd = serde_json::json!({
        "type": "thread.create",
        "commandId": "cmd-create",
        "threadId": "thread-lifecycle-1",
        "title": "Lifecycle Thread",
    });
    let req = FromClientMessage::Request {
        id: serde_json::json!("req-create"),
        tag: "orchestration.dispatchCommand".to_string(),
        payload: create_cmd,
        headers: vec![],
        discard: None,
    };
    process_client_message(req, &state, &out_tx, &subs, &proc_mgr).await;

    let reply = out_rx.recv().await.expect("Expected create response");
    match reply {
        FromServerMessage::Exit { exit, .. } => match exit {
            RpcExit::Success { value } => {
                assert!(value.get("sequence").is_some());
            }
            RpcExit::Failure { .. } => panic!("Expected success"),
        },
        other => panic!("Expected Exit, got {:?}", other),
    }

    let th = state.storage.get_thread("thread-lifecycle-1").unwrap().unwrap();
    assert_eq!(th.title, "Lifecycle Thread");
    assert_eq!(th.archived_at, None);

    // 2. thread.archive
    let mut shell_rx = state.shell_broadcast.subscribe();
    let archive_cmd = serde_json::json!({
        "type": "thread.archive",
        "commandId": "cmd-archive",
        "threadId": "thread-lifecycle-1",
    });
    let req = FromClientMessage::Request {
        id: serde_json::json!("req-archive"),
        tag: "orchestration.dispatchCommand".to_string(),
        payload: archive_cmd,
        headers: vec![],
        discard: None,
    };
    process_client_message(req, &state, &out_tx, &subs, &proc_mgr).await;
    let _ = out_rx.recv().await.unwrap();

    let th_archived = state.storage.get_thread("thread-lifecycle-1").unwrap().unwrap();
    assert!(th_archived.archived_at.is_some());

    let shell_event = shell_rx.recv().await.unwrap();
    match shell_event {
        OrchestrationV2ShellStreamItem::ThreadUpdated { location, .. } => {
            assert_eq!(location, "archive");
        }
        other => panic!("Expected ThreadUpdated, got {:?}", other),
    }

    // 3. thread.unarchive
    let unarchive_cmd = serde_json::json!({
        "type": "thread.unarchive",
        "commandId": "cmd-unarchive",
        "threadId": "thread-lifecycle-1",
    });
    let req = FromClientMessage::Request {
        id: serde_json::json!("req-unarchive"),
        tag: "orchestration.dispatchCommand".to_string(),
        payload: unarchive_cmd,
        headers: vec![],
        discard: None,
    };
    process_client_message(req, &state, &out_tx, &subs, &proc_mgr).await;
    let _ = out_rx.recv().await.unwrap();

    let th_unarchived = state.storage.get_thread("thread-lifecycle-1").unwrap().unwrap();
    assert!(th_unarchived.archived_at.is_none());

    let shell_event = shell_rx.recv().await.unwrap();
    match shell_event {
        OrchestrationV2ShellStreamItem::ThreadUpdated { location, .. } => {
            assert_eq!(location, "active");
        }
        other => panic!("Expected ThreadUpdated, got {:?}", other),
    }

    // 4. thread.settle
    let settle_cmd = serde_json::json!({
        "type": "thread.settle",
        "commandId": "cmd-settle",
        "threadId": "thread-lifecycle-1",
    });
    let req = FromClientMessage::Request {
        id: serde_json::json!("req-settle"),
        tag: "orchestration.dispatchCommand".to_string(),
        payload: settle_cmd,
        headers: vec![],
        discard: None,
    };
    process_client_message(req, &state, &out_tx, &subs, &proc_mgr).await;
    let _ = out_rx.recv().await.unwrap();

    // 5. thread.unsettle
    let unsettle_cmd = serde_json::json!({
        "type": "thread.unsettle",
        "commandId": "cmd-unsettle",
        "threadId": "thread-lifecycle-1",
    });
    let req = FromClientMessage::Request {
        id: serde_json::json!("req-unsettle"),
        tag: "orchestration.dispatchCommand".to_string(),
        payload: unsettle_cmd,
        headers: vec![],
        discard: None,
    };
    process_client_message(req, &state, &out_tx, &subs, &proc_mgr).await;
    let _ = out_rx.recv().await.unwrap();

    // 6. thread.delete
    let delete_cmd = serde_json::json!({
        "type": "thread.delete",
        "commandId": "cmd-delete",
        "threadId": "thread-lifecycle-1",
    });
    let req = FromClientMessage::Request {
        id: serde_json::json!("req-delete"),
        tag: "orchestration.dispatchCommand".to_string(),
        payload: delete_cmd,
        headers: vec![],
        discard: None,
    };
    process_client_message(req, &state, &out_tx, &subs, &proc_mgr).await;
    let _ = out_rx.recv().await.unwrap();

    let th_deleted = state.storage.get_thread("thread-lifecycle-1").unwrap().unwrap();
    assert!(th_deleted.deleted_at.is_some());
}

#[tokio::test]
async fn test_dispatch_message_and_mock_pi_process() {
    let (_dir, state, proc_mgr) = setup_test_app();
    let (out_tx, mut out_rx) = mpsc::channel(10);
    let subs = Arc::new(Mutex::new(HashMap::new()));

    // Create a mock script for Pi agent that answers RPC commands
    let script_dir = tempdir().unwrap();
    let script_path = script_dir.path().join("mock_pi.sh");
    let script_content = r#"#!/bin/sh
while IFS= read -r line; do
  if echo "$line" | grep -q '"type":"prompt"'; then
    id=$(echo "$line" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
    echo "{\"type\":\"response\",\"id\":\"$id\",\"success\":true,\"data\":{}}"
    echo "{\"type\":\"agent_start\"}"
    echo "{\"type\":\"message_start\",\"message\":{\"role\":\"assistant\"}}"
    echo "{\"type\":\"message_update\",\"assistantMessageEvent\":{\"type\":\"text_delta\",\"delta\":\"Hello from mock Pi!\"}}"
    echo "{\"type\":\"message_end\",\"message\":{\"role\":\"assistant\"}}"
    echo "{\"type\":\"agent_settled\",\"aborted\":false}"
  fi
done
"#;
    std::fs::write(&script_path, script_content).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script_path, perms).unwrap();
    }

    // 1. Create a thread with mock script in options
    let create_cmd = serde_json::json!({
        "type": "thread.create",
        "commandId": "cmd-create",
        "threadId": "thread-pi-test",
        "title": "Pi Prompt Thread",
        "modelSelection": {
            "instanceId": "pi",
            "model": "default",
            "options": {
                "program": script_path.display().to_string()
            }
        }
    });
    handle_dispatch_command(&state, create_cmd, &proc_mgr).await.unwrap();

    // 2. Dispatch a message
    let dispatch_cmd = serde_json::json!({
        "type": "message.dispatch",
        "commandId": "cmd-msg-1",
        "threadId": "thread-pi-test",
        "messageId": "msg-user-1",
        "text": "Hello assistant",
        "attachments": [],
    });
    let req = FromClientMessage::Request {
        id: serde_json::json!("req-msg-1"),
        tag: "orchestration.dispatchCommand".to_string(),
        payload: dispatch_cmd,
        headers: vec![],
        discard: None,
    };
    process_client_message(req, &state, &out_tx, &subs, &proc_mgr).await;

    // Verify command receipt response
    let reply = out_rx.recv().await.expect("Expected dispatchCommand receipt");
    match reply {
        FromServerMessage::Exit { exit, .. } => match exit {
            RpcExit::Success { value } => {
                assert!(value.get("sequence").is_some());
            }
            RpcExit::Failure { .. } => panic!("Expected success"),
        },
        other => panic!("Expected Exit, got {:?}", other),
    }

    // Verify user message in storage
    let msg = state.storage.get_message("msg-user-1").unwrap().unwrap();
    assert_eq!(msg.text, "Hello assistant");
    assert_eq!(msg.role, "user");

    // Wait a brief moment for the mock Pi agent background task to complete
    let mut turn_items = Vec::new();
    for _ in 0..50 {
        turn_items = state.storage.list_turn_items("thread-pi-test").unwrap();
        if turn_items.len() >= 2 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    assert!(turn_items.len() >= 2, "Expected user item and assistant item in turn items");
    let assistant_item = turn_items.iter().find(|i| i.item_type == "assistant_message").unwrap();
    assert_eq!(assistant_item.text.as_deref(), Some("Hello from mock Pi!"));
}

#[tokio::test]
async fn test_dispatch_message_pi_spawn_failure_graceful() {
    let (_dir, state, proc_mgr) = setup_test_app();
    let (out_tx, mut out_rx) = mpsc::channel(10);
    let subs = Arc::new(Mutex::new(HashMap::new()));

    // Create a thread pointing to non-existent executable
    let create_cmd = serde_json::json!({
        "type": "thread.create",
        "commandId": "cmd-create-fail",
        "threadId": "thread-pi-fail",
        "title": "Pi Fail Thread",
        "modelSelection": {
            "instanceId": "pi",
            "model": "default",
            "options": {
                "program": "/non/existent/path/to/pi_binary_xyz"
            }
        }
    });
    handle_dispatch_command(&state, create_cmd, &proc_mgr).await.unwrap();

    // Dispatch message
    let dispatch_cmd = serde_json::json!({
        "type": "message.dispatch",
        "commandId": "cmd-msg-fail",
        "threadId": "thread-pi-fail",
        "messageId": "msg-user-fail",
        "text": "Hello that fails",
        "attachments": [],
    });
    let req = FromClientMessage::Request {
        id: serde_json::json!("req-msg-fail"),
        tag: "orchestration.dispatchCommand".to_string(),
        payload: dispatch_cmd,
        headers: vec![],
        discard: None,
    };
    process_client_message(req, &state, &out_tx, &subs, &proc_mgr).await;

    // Command receipt should still be success
    let reply = out_rx.recv().await.expect("Expected dispatchCommand receipt");
    match reply {
        FromServerMessage::Exit { exit, .. } => match exit {
            RpcExit::Success { value } => {
                assert!(value.get("sequence").is_some());
            }
            RpcExit::Failure { .. } => panic!("Expected success receipt"),
        },
        other => panic!("Expected Exit, got {:?}", other),
    }

    // Wait briefly for failure turn item
    for _ in 0..50 {
        let runs = state.storage.list_runs("thread-pi-fail").unwrap();
        if runs.first().map(|r| r.status.as_str()) == Some("failed") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    let runs = state.storage.list_runs("thread-pi-fail").unwrap();
    assert_eq!(runs[0].status, "failed");

    let turn_items = state.storage.list_turn_items("thread-pi-fail").unwrap();
    let err_item = turn_items.iter().find(|i| i.status == "failed").expect("Expected failed turn item");
    assert!(err_item.text.as_deref().unwrap().contains("Pi process failed to start"));
}

#[tokio::test]
async fn test_unknown_rpc_tag_returns_failure() {
    let (_dir, state, proc_mgr) = setup_test_app();
    let (out_tx, mut out_rx) = mpsc::channel(10);
    let subs = Arc::new(Mutex::new(HashMap::new()));

    let req = FromClientMessage::Request {
        id: serde_json::json!("req-unknown"),
        tag: "nonExistentRpcMethod".to_string(),
        payload: serde_json::json!({}),
        headers: vec![],
        discard: None,
    };
    process_client_message(req, &state, &out_tx, &subs, &proc_mgr).await;

    let reply = out_rx.recv().await.expect("Expected failure reply");
    match reply {
        FromServerMessage::Exit { request_id, exit } => {
            assert_eq!(request_id, serde_json::json!("req-unknown"));
            match exit {
                RpcExit::Failure { cause } => {
                    assert!(!cause.is_empty());
                }
                RpcExit::Success { .. } => panic!("Expected failure exit"),
            }
        }
        other => panic!("Expected Exit, got {:?}", other),
    }
}

#[tokio::test]
async fn test_ws_handler_signature() {
    // Verifies pub async fn ws_handler(ws: axum::extract::ws::WebSocketUpgrade, state: std::sync::Arc<AppState>) -> axum::response::Response
    use super::session::ws_handler;
    let _fn_ptr: fn(axum::extract::ws::WebSocketUpgrade, Arc<AppState>) -> _ = |ws, state| {
        ws_handler(ws, state)
    };

    let (_dir, state, _proc_mgr) = setup_test_app();
    let state_clone = state.clone();
    let app = axum::Router::new().route(
        "/ws",
        axum::routing::get(move |ws| ws_handler(ws, state_clone)),
    );
    let _service = app.into_make_service();
}

#[tokio::test]
async fn test_dispatch_thread_stop() {
    let (_dir, state, proc_mgr) = setup_test_app();

    // Create a thread and run
    let project = state.storage.get_or_create_project("/test/path", "Test Project").unwrap();
    let thread = OrchestrationV2ThreadShell {
        id: "thread-stop-test".to_string(),
        project_id: project.id.clone(),
        title: "Stop Thread".to_string(),
        provider_instance_id: "pi".to_string(),
        model_selection: ModelSelection {
            instance_id: "pi".to_string(),
            model: "default".to_string(),
            options: None,
        },
        runtime_mode: "autonomous".to_string(),
        interaction_mode: "chat".to_string(),
        branch: None,
        worktree_path: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        archived_at: None,
        deleted_at: None,
        status: Some("running".to_string()),
        latest_run_id: None,
        item_count: 0,
    };
    state.storage.create_thread(thread).unwrap();

    let run = OrchestrationV2Run {
        id: "run-stop-1".to_string(),
        thread_id: "thread-stop-test".to_string(),
        ordinal: 1,
        provider_instance_id: "pi".to_string(),
        model_selection: ModelSelection {
            instance_id: "pi".to_string(),
            model: "default".to_string(),
            options: None,
        },
        status: "running".to_string(),
        requested_at: chrono::Utc::now(),
        started_at: Some(chrono::Utc::now()),
        completed_at: None,
    };
    state.storage.create_run(run).unwrap();

    // Stop thread
    let stop_cmd = serde_json::json!({
        "type": "thread.stop",
        "commandId": "cmd-stop",
        "threadId": "thread-stop-test",
    });
    let seq = handle_dispatch_command(&state, stop_cmd, &proc_mgr).await.unwrap();
    assert!(seq > 0);

    // Verify run is marked cancelled
    let updated_run = state.storage.get_run("run-stop-1").unwrap().unwrap();
    assert_eq!(updated_run.status, "cancelled");

    // Verify thread is idle
    let updated_th = state.storage.get_thread("thread-stop-test").unwrap().unwrap();
    assert_eq!(updated_th.status.as_deref(), Some("idle"));
}
