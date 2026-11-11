use axum::extract::ws::{WebSocket, WebSocketUpgrade};
use axum::response::Response;
use std::sync::Arc;
use crate::state::AppState;

pub async fn ws_handler(ws: WebSocketUpgrade, state: Arc<AppState>) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(_socket: WebSocket, _state: Arc<AppState>) {}

