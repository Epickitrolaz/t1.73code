use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use std::sync::Arc;
use tower::ServiceExt;
use crate::http::build_router;
use crate::state::AppState;
use crate::storage::Storage;

async fn setup_test_app() -> (axum::Router, Arc<AppState>, tempfile::TempDir) {
    let temp_dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(Storage::open_in_memory().unwrap());
    let state = Arc::new(AppState::new(temp_dir.path().to_path_buf(), storage));
    let router = build_router(state.clone());
    (router, state, temp_dir)
}

async fn create_authenticated_session(state: &AppState) -> (String, String) {
    let session_id = format!("ses_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
    let token = format!("tok_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
    let expires_at = chrono::Utc::now() + chrono::Duration::days(30);
    state
        .storage
        .create_session(&session_id, &token, expires_at)
        .unwrap();
    (session_id, token)
}

#[tokio::test]
async fn test_get_environment_descriptor() {
    let (app, state, _dir) = setup_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/.well-known/t3/environment")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(json["environmentId"], state.environment_id);
    assert_eq!(json["serverVersion"], state.server_version);
    assert!(json["platform"]["os"].is_string());
    assert!(json["platform"]["arch"].is_string());
    assert!(json["platform"]["machine"].is_string());
    assert!(json["capabilities"].is_object());
}

#[tokio::test]
async fn test_auth_session_unauthenticated() {
    let (app, _state, _dir) = setup_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/auth/session")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(json["authenticated"], false);
    assert_eq!(json["auth"]["tokenExchange"], true);
    assert!(json["scopes"].is_null());
}

#[tokio::test]
async fn test_auth_session_authenticated_bearer() {
    let (app, state, _dir) = setup_test_app().await;
    let (_session_id, token) = create_authenticated_session(&state).await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/auth/session")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(json["authenticated"], true);
    assert!(json["scopes"].is_array());
    assert_eq!(json["sessionMethod"], "bearer-access-token");
    assert!(json["expiresAt"].is_string());
}

#[tokio::test]
async fn test_auth_session_authenticated_cookie() {
    let (app, state, _dir) = setup_test_app().await;
    let (_session_id, token) = create_authenticated_session(&state).await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/auth/session")
                .header(header::COOKIE, format!("t3_session={token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(json["authenticated"], true);
}

#[tokio::test]
async fn test_oauth_token_exchange_success_json() {
    let (app, state, _dir) = setup_test_app().await;

    let pair_token = "pair-token-json-test";
    let scopes = vec!["orchestration:read".to_string(), "orchestration:operate".to_string()];
    state
        .storage
        .create_pairing_link(pair_token, &scopes, chrono::Utc::now() + chrono::Duration::hours(1))
        .unwrap();

    let payload = serde_json::json!({
        "grant_type": "urn:ietf:params:oauth:grant-type:token-exchange",
        "subject_token": pair_token,
        "subject_token_type": "urn:ietf:params:oauth:token-type:access_token",
        "client_label": "my-client"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/oauth/token")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let cookie = response.headers().get(header::SET_COOKIE).unwrap().to_str().unwrap();
    assert!(cookie.starts_with("t3_session="));

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    assert!(json["access_token"].as_str().unwrap().starts_with("tok_"));
    assert_eq!(json["token_type"], "Bearer");
    assert_eq!(json["issued_token_type"], "urn:ietf:params:oauth:token-type:access_token");
    assert_eq!(json["scope"], "orchestration:read orchestration:operate");

    // Check that the pairing token was consumed
    assert!(state.storage.get_pairing_link(pair_token).unwrap().unwrap().consumed_at.is_some());
}

#[tokio::test]
async fn test_oauth_token_exchange_success_form_urlencoded() {
    let (app, state, _dir) = setup_test_app().await;

    let pair_token = "pair-token-form-test";
    let scopes = vec!["relay:read".to_string()];
    state
        .storage
        .create_pairing_link(pair_token, &scopes, chrono::Utc::now() + chrono::Duration::hours(1))
        .unwrap();

    let form_body = format!("grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Atoken-exchange&subject_token={pair_token}&client_label=cli");

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/oauth/token")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(form_body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(json["access_token"].as_str().unwrap().starts_with("tok_"));
}

#[tokio::test]
async fn test_oauth_token_exchange_invalid_token() {
    let (app, _state, _dir) = setup_test_app().await;

    let payload = serde_json::json!({
        "subject_token": "non-existent-token"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/oauth/token")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["error"], "invalid_grant");
}

#[tokio::test]
async fn test_create_pairing_token_auth_guard() {
    let (app, _state, _dir) = setup_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/pairing-token")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&serde_json::json!({})).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_create_pairing_token_authenticated() {
    let (app, state, _dir) = setup_test_app().await;
    let (_session_id, token) = create_authenticated_session(&state).await;

    let payload = serde_json::json!({
        "label": "test-label",
        "scopes": ["orchestration:read", "access:write"]
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/pairing-token")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    assert!(json["credential"].as_str().unwrap().starts_with("pair_"));
    assert_eq!(json["label"], "test-label");
    assert!(json["expiresAt"].is_string());

    // Verify stored in storage
    let stored = state
        .storage
        .get_pairing_link(json["credential"].as_str().unwrap())
        .unwrap();
    assert!(stored.is_some());
}

#[tokio::test]
async fn test_create_websocket_ticket() {
    let (app, state, _dir) = setup_test_app().await;
    let (_session_id, token) = create_authenticated_session(&state).await;

    // Bare path
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/websocket-ticket")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let ticket = json["ticket"].as_str().unwrap();
    assert!(ticket.starts_with("wst_"));
    assert!(json["expiresAt"].is_string());

    // Verify the ticket can validate as a session
    assert!(state.storage.validate_session(ticket).unwrap().is_some());

    // Path with :use segment
    let response_for = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/websocket-ticket/rpc")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response_for.status(), StatusCode::OK);
    let bytes_for = axum::body::to_bytes(response_for.into_body(), usize::MAX)
        .await
        .unwrap();
    let json_for: serde_json::Value = serde_json::from_slice(&bytes_for).unwrap();
    assert!(json_for["ticket"].as_str().unwrap().starts_with("wst_"));
}

#[tokio::test]
async fn test_t3_connect_health() {
    let (app, state, _dir) = setup_test_app().await;

    let payload = serde_json::json!({
        "proof": "test-proof"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/t3-connect/health")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(json["environmentId"], state.environment_id);
    assert_eq!(json["status"], "online");
    assert!(json["descriptor"].is_object());
    assert!(json["checkedAt"].is_string());
    assert!(json["proof"].is_string());
}

#[tokio::test]
async fn test_t3_connect_mint_credential() {
    let (app, state, _dir) = setup_test_app().await;

    let payload = serde_json::json!({
        "proof": "test-mint-proof"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/t3-connect/mint-credential")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    let credential = json["credential"].as_str().unwrap();
    assert!(credential.starts_with("pair_"));
    assert!(json["expiresAt"].is_string());
    assert!(json["proof"].is_string());

    // Stored in sqlite
    assert!(state.storage.get_pairing_link(credential).unwrap().is_some());
}

#[tokio::test]
async fn test_connect_link_proof() {
    let (app, state, _dir) = setup_test_app().await;
    let (_session_id, token) = create_authenticated_session(&state).await;

    let payload = serde_json::json!({
        "challenge": "challenge-123",
        "relayIssuer": "https://relay.t3.codes",
        "endpoint": { "providerKind": "cloudflare_tunnel" },
        "origin": { "localHttpHost": "127.0.0.1", "localHttpPort": 3000 }
    });

    // Unauthenticated -> 401
    let unauth_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/connect/link-proof")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauth_resp.status(), StatusCode::UNAUTHORIZED);

    // Authenticated -> 200 with proof string
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/connect/link-proof")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let proof: String = serde_json::from_slice(&bytes).unwrap();
    assert!(proof.contains("challenge-123"));
}

#[tokio::test]
async fn test_connect_relay_config_and_link_state() {
    let (app, state, _dir) = setup_test_app().await;
    let (_session_id, token) = create_authenticated_session(&state).await;

    let config_payload = serde_json::json!({
        "relayUrl": "https://custom-relay.example.com",
        "cloudUserId": "user-456",
        "environmentCredential": "env-cred-secret",
        "cloudMintPublicKey": "pk-123"
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/connect/relay-config")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&config_payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["ok"], true);

    // Now query link-state
    let state_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/connect/link-state")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(state_resp.status(), StatusCode::OK);
    let state_bytes = axum::body::to_bytes(state_resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let state_json: serde_json::Value = serde_json::from_slice(&state_bytes).unwrap();
    assert_eq!(state_json["linked"], true);
    assert_eq!(state_json["cloudUserId"], "user-456");
    assert_eq!(state_json["relayUrl"], "https://custom-relay.example.com");

    // Unlink
    let unlink_resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/connect/unlink")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(unlink_resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_connect_reachability() {
    let (app, state, _dir) = setup_test_app().await;
    let (_session_id, token) = create_authenticated_session(&state).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/connect/reachability")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(json["status"] == "reachable" || json["status"] == "unreachable");
    assert!(json["checkedAt"].is_string());
}

#[tokio::test]
async fn test_ws_endpoint_mounted() {
    let (app, _state, _dir) = setup_test_app().await;

    // Normal GET without WebSocket headers receives Bad Request or 426 Upgrade Required
    let response = app
        .oneshot(
            Request::builder()
                .uri("/ws")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Axum WebSocketUpgrade extractor rejects non-websocket requests with 400 or 426
    assert!(
        response.status() == StatusCode::BAD_REQUEST
            || response.status() == StatusCode::UPGRADE_REQUIRED,
        "Expected websocket upgrade response, got {}",
        response.status()
    );
}
