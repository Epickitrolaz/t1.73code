use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use futures::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use tracing::{debug, warn};

use crate::protocol::rpc::FromClientMessage;
use crate::protocol::rpc::FromServerMessage;
use crate::state::AppState;
use super::config::{build_server_config, get_server_providers};
use super::dispatch::handle_dispatch_command;
use super::process_manager::{global_process_manager, ProcessManager};
use super::streams::{
    handle_subscribe_server_config, handle_subscribe_shell, handle_subscribe_thread,
};

pub async fn ws_handler(ws: WebSocketUpgrade, state: Arc<AppState>) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

pub async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut ws_sink, mut ws_stream) = socket.split();
    let (out_tx, mut out_rx) = mpsc::channel::<FromServerMessage>(256);

    let writer_handle = tokio::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            if let Ok(json) = serde_json::to_string(&msg) {
                if ws_sink.send(Message::Text(json.into())).await.is_err() {
                    break;
                }
            }
        }
    });

    let subscriptions: Arc<Mutex<HashMap<String, JoinHandle<()>>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let process_mgr = global_process_manager();

    while let Some(result) = ws_stream.next().await {
        let ws_msg = match result {
            Ok(m) => m,
            Err(e) => {
                debug!("WebSocket read error: {e}");
                break;
            }
        };

        match ws_msg {
            Message::Text(text) => {
                handle_incoming_text(
                    text.as_str(),
                    &state,
                    &out_tx,
                    &subscriptions,
                    process_mgr,
                )
                .await;
            }
            Message::Binary(bin) => {
                if let Ok(text) = std::str::from_utf8(&bin) {
                    handle_incoming_text(
                        text,
                        &state,
                        &out_tx,
                        &subscriptions,
                        process_mgr,
                    )
                    .await;
                }
            }
            Message::Ping(_) => {
                let _ = out_tx.send(FromServerMessage::Pong).await;
            }
            Message::Close(_) => {
                break;
            }
            _ => {}
        }
    }

    let mut subs = subscriptions.lock().await;
    for (_, handle) in subs.drain() {
        handle.abort();
    }
    drop(subs);

    writer_handle.abort();
}

async fn handle_incoming_text(
    text: &str,
    state: &Arc<AppState>,
    out_tx: &mpsc::Sender<FromServerMessage>,
    subscriptions: &Arc<Mutex<HashMap<String, JoinHandle<()>>>>,
    process_mgr: &ProcessManager,
) {
    if let Ok(client_msg) = serde_json::from_str::<FromClientMessage>(text) {
        process_client_message(client_msg, state, out_tx, subscriptions, process_mgr).await;
    } else if let Ok(client_msgs) = serde_json::from_str::<Vec<FromClientMessage>>(text) {
        for msg in client_msgs {
            process_client_message(msg, state, out_tx, subscriptions, process_mgr).await;
        }
    } else {
        warn!("Unparseable client message: {text}");
    }
}

pub async fn process_client_message(
    msg: FromClientMessage,
    state: &Arc<AppState>,
    out_tx: &mpsc::Sender<FromServerMessage>,
    subscriptions: &Arc<Mutex<HashMap<String, JoinHandle<()>>>>,
    process_mgr: &ProcessManager,
) {
    match msg {
        FromClientMessage::Ping => {
            let _ = out_tx.send(FromServerMessage::Pong).await;
        }
        FromClientMessage::Interrupt { request_id } => {
            let req_id_key = request_id.to_string();
            let mut subs = subscriptions.lock().await;
            if let Some(handle) = subs.remove(&req_id_key) {
                handle.abort();
            }
        }
        FromClientMessage::Ack { .. } => {}
        FromClientMessage::Eof => {}
        FromClientMessage::Request {
            id,
            tag,
            payload,
            headers: _,
            discard: _,
        } => {
            match tag.as_str() {
                "Ping" | "ping" => {
                    let _ = out_tx.send(FromServerMessage::Pong).await;
                }
                "server.probe" => {
                    let _ = out_tx
                        .send(FromServerMessage::success(id, serde_json::json!({})))
                        .await;
                }
                "server.getConfig" => {
                    let config = build_server_config(state);
                    if let Ok(val) = serde_json::to_value(&config) {
                        let _ = out_tx.send(FromServerMessage::success(id, val)).await;
                    }
                }
                "server.refreshProviders" => {
                    let providers = get_server_providers(state);
                    let val = serde_json::json!({ "providers": providers });
                    let _ = out_tx.send(FromServerMessage::success(id, val)).await;
                }
                "subscribeServerConfig" => {
                    let req_id_key = id.to_string();
                    let state_clone = state.clone();
                    let out_tx_clone = out_tx.clone();
                    let id_clone = id.clone();
                    let handle = tokio::spawn(async move {
                        handle_subscribe_server_config(id_clone, state_clone, out_tx_clone).await;
                    });
                    let mut subs = subscriptions.lock().await;
                    if let Some(old) = subs.insert(req_id_key, handle) {
                        old.abort();
                    }
                }
                "orchestration.subscribeShell" => {
                    let req_id_key = id.to_string();
                    let state_clone = state.clone();
                    let out_tx_clone = out_tx.clone();
                    let id_clone = id.clone();
                    let handle = tokio::spawn(async move {
                        handle_subscribe_shell(id_clone, payload, state_clone, out_tx_clone).await;
                    });
                    let mut subs = subscriptions.lock().await;
                    if let Some(old) = subs.insert(req_id_key, handle) {
                        old.abort();
                    }
                }
                "orchestration.subscribeThread" => {
                    let req_id_key = id.to_string();
                    let state_clone = state.clone();
                    let out_tx_clone = out_tx.clone();
                    let id_clone = id.clone();
                    let handle = tokio::spawn(async move {
                        handle_subscribe_thread(id_clone, payload, state_clone, out_tx_clone).await;
                    });
                    let mut subs = subscriptions.lock().await;
                    if let Some(old) = subs.insert(req_id_key, handle) {
                        old.abort();
                    }
                }
                "orchestration.dispatchCommand" => {
                    match handle_dispatch_command(state, payload, process_mgr).await {
                        Ok(seq) => {
                            let val = serde_json::json!({ "sequence": seq });
                            let _ = out_tx.send(FromServerMessage::success(id, val)).await;
                        }
                        Err(e) => {
                            let _ = out_tx
                                .send(FromServerMessage::failure(
                                    id,
                                    serde_json::json!({ "message": e }),
                                ))
                                .await;
                        }
                    }
                }
                other => {
                    let _ = out_tx
                        .send(FromServerMessage::failure(
                            id,
                            serde_json::json!({ "message": format!("Unknown tag: {}", other) }),
                        ))
                        .await;
                }
            }
        }
    }
}
