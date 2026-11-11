pub mod auth;
pub mod connect;
pub mod environment;
pub mod models;

#[cfg(test)]
mod tests;

use axum::{
    extract::{ws::WebSocketUpgrade, State},
    response::Response,
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use crate::state::AppState;

pub fn build_router(state: Arc<AppState>) -> Router {
    let cors = CorsLayer::permissive();

    Router::new()
        // Discovery
        .route("/.well-known/t3/environment", get(environment::get_environment))
        // Auth & session
        .route("/api/auth/session", get(auth::get_session))
        .route("/oauth/token", post(auth::oauth_token))
        .route("/api/auth/pairing-token", post(auth::create_pairing_token))
        .route("/api/auth/websocket-ticket", post(auth::create_websocket_ticket))
        .route("/api/auth/websocket-ticket/{use_param}", post(auth::create_websocket_ticket_for))
        // T3 Connect / relay
        .route("/api/t3-connect/health", post(connect::relay_health))
        .route("/api/t3-connect/mint-credential", post(connect::relay_mint_credential))
        .route("/api/connect/link-proof", post(connect::link_proof))
        .route("/api/connect/relay-config", post(connect::relay_config))
        .route("/api/connect/link-state", get(connect::link_state))
        .route("/api/connect/reachability", post(connect::check_reachability_endpoint))
        .route("/api/connect/unlink", post(connect::unlink))
        // WebSocket
        .route("/ws", get(ws_route_handler))
        .layer(cors)
        .with_state(state)
}

async fn ws_route_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> Response {
    crate::ws::ws_handler(ws, state).await
}
