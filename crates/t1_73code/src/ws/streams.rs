use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};

use crate::protocol::models::*;
use crate::protocol::rpc::FromServerMessage;
use crate::state::AppState;
use super::config::build_server_config;
use super::dispatch::current_sequence;

pub async fn handle_subscribe_server_config(
    request_id: serde_json::Value,
    state: Arc<AppState>,
    out_tx: mpsc::Sender<FromServerMessage>,
) {
    let config = build_server_config(&state);
    let event = serde_json::json!({
        "version": 1,
        "type": "snapshot",
        "config": config,
    });
    let chunk = FromServerMessage::chunk(request_id.clone(), vec![event]);
    if out_tx.send(chunk).await.is_err() {
        return;
    }

    // Keep subscription active awaiting updates or cancellation
    futures::future::pending::<()>().await;
}

pub async fn handle_subscribe_shell(
    request_id: serde_json::Value,
    payload: serde_json::Value,
    state: Arc<AppState>,
    out_tx: mpsc::Sender<FromServerMessage>,
) {
    // 1. Subscribe to broadcast channel before reading snapshot to avoid races
    let mut rx = state.shell_broadcast.subscribe();

    let current_seq = current_sequence();
    let projects = state.storage.list_projects().unwrap_or_default();
    let mut threads = Vec::new();
    for p in &projects {
        if let Ok(mut t) = state.storage.list_threads(&p.id) {
            threads.append(&mut t);
        }
    }

    let snapshot = OrchestrationV2ShellSnapshot {
        sequence: current_seq,
        projects,
        threads,
    };
    let item = OrchestrationV2ShellStreamItem::Snapshot { snapshot };
    if let Ok(val) = serde_json::to_value(&item) {
        if out_tx
            .send(FromServerMessage::chunk(request_id.clone(), vec![val]))
            .await
            .is_err()
        {
            return;
        }
    }

    if payload
        .get("requestCompletionMarker")
        .and_then(|v| v.as_bool())
        == Some(true)
    {
        let sync_item = OrchestrationV2ShellStreamItem::Synchronized;
        if let Ok(val) = serde_json::to_value(&sync_item) {
            let _ = out_tx
                .send(FromServerMessage::chunk(request_id.clone(), vec![val]))
                .await;
        }
    }

    // 2. Stream live updates, skipping any buffered events already included in snapshot
    loop {
        match rx.recv().await {
            Ok(shell_item) => {
                match &shell_item {
                    OrchestrationV2ShellStreamItem::ThreadUpdated { sequence, .. }
                    | OrchestrationV2ShellStreamItem::ThreadRemoved { sequence, .. }
                    | OrchestrationV2ShellStreamItem::ProjectUpdated { sequence, .. } => {
                        if *sequence <= current_seq {
                            continue;
                        }
                    }
                    _ => {}
                }

                if let Ok(val) = serde_json::to_value(&shell_item) {
                    if out_tx
                        .send(FromServerMessage::chunk(request_id.clone(), vec![val]))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            }
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}

pub async fn handle_subscribe_thread(
    request_id: serde_json::Value,
    payload: serde_json::Value,
    state: Arc<AppState>,
    out_tx: mpsc::Sender<FromServerMessage>,
) {
    let thread_id = match payload
        .get("threadId")
        .or_else(|| payload.get("thread_id"))
        .and_then(|v| v.as_str())
    {
        Some(id) => id.to_string(),
        None => {
            let _ = out_tx
                .send(FromServerMessage::failure(
                    request_id,
                    serde_json::json!({ "message": "Missing threadId in subscribeThread" }),
                ))
                .await;
            return;
        }
    };

    // 1. Subscribe to broadcast channel before reading snapshot to avoid races
    let mut rx = state.thread_broadcast.subscribe();

    let projection = match state.storage.get_thread_projection(&thread_id) {
        Ok(Some(p)) => p,
        Ok(None) => {
            let _ = out_tx
                .send(FromServerMessage::failure(
                    request_id,
                    serde_json::json!({ "message": format!("Thread {} not found", thread_id) }),
                ))
                .await;
            return;
        }
        Err(e) => {
            let _ = out_tx
                .send(FromServerMessage::failure(
                    request_id,
                    serde_json::json!({ "message": format!("Storage error: {}", e) }),
                ))
                .await;
            return;
        }
    };

    let current_seq = current_sequence();
    let snapshot_item = OrchestrationV2ThreadStreamItem::Snapshot {
        snapshot_sequence: current_seq,
        projection,
    };
    if let Ok(val) = serde_json::to_value(&snapshot_item) {
        if out_tx
            .send(FromServerMessage::chunk(request_id.clone(), vec![val]))
            .await
            .is_err()
        {
            return;
        }
    }

    if payload
        .get("requestCompletionMarker")
        .and_then(|v| v.as_bool())
        == Some(true)
    {
        let sync_item = OrchestrationV2ThreadStreamItem::Synchronized;
        if let Ok(val) = serde_json::to_value(&sync_item) {
            let _ = out_tx
                .send(FromServerMessage::chunk(request_id.clone(), vec![val]))
                .await;
        }
    }

    // 2. Stream live events for this thread, skipping buffered events <= current_seq
    loop {
        match rx.recv().await {
            Ok((event_tid, thread_item)) => {
                if event_tid == thread_id {
                    match &thread_item {
                        OrchestrationV2ThreadStreamItem::Event { sequence, .. } => {
                            if *sequence <= current_seq {
                                continue;
                            }
                        }
                        _ => {}
                    }

                    if let Ok(val) = serde_json::to_value(&thread_item) {
                        if out_tx
                            .send(FromServerMessage::chunk(request_id.clone(), vec![val]))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            }
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}
