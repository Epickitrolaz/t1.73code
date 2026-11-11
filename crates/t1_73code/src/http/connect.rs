use axum::{
    extract::State,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use std::sync::Arc;
use crate::cloud::actions::connect_unlink;
use crate::cloud::reachability::check_reachability;
use crate::cloud::status::get_cloud_status;
use crate::cloud::storage::{
    set_desired_cloud_link, write_secret, CLOUD_LINKED_USER_ID_SECRET, RELAY_URL_SECRET,
};
use crate::http::auth::authenticate_session;
use crate::http::models::{
    ErrorResponse, LinkProofRequest, LinkStateResult, RelayConfigRequest, RelayConfigResult,
    RelayHealthRequest, RelayHealthResponse, RelayMintRequest, RelayMintResponse,
};
use crate::protocol::models::{
    ExecutionEnvironmentCapabilities, ExecutionEnvironmentDescriptor, ExecutionEnvironmentPlatform,
};
use crate::state::AppState;

fn make_descriptor(state: &AppState) -> ExecutionEnvironmentDescriptor {
    let hostname = std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("HOST"))
        .unwrap_or_else(|_| "localhost".to_string());

    ExecutionEnvironmentDescriptor {
        environment_id: state.environment_id.clone(),
        label: hostname.clone(),
        platform: ExecutionEnvironmentPlatform {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            machine: hostname,
        },
        server_version: state.server_version.clone(),
        orchestration_protocol_version: Some(1),
        database_migration: None,
        capabilities: ExecutionEnvironmentCapabilities::default(),
    }
}

pub async fn relay_health(
    State(state): State<Arc<AppState>>,
    body: Option<Json<RelayHealthRequest>>,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    let _request = body.map(|b| b.0).unwrap_or_default();
    let descriptor = make_descriptor(&state);
    let checked_at = Utc::now().to_rfc3339();
    let proof = format!("health_proof_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));

    let response = RelayHealthResponse {
        environment_id: state.environment_id.clone(),
        status: "online".to_string(),
        descriptor,
        checked_at,
        proof,
    };

    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));

    Ok((StatusCode::OK, headers, Json(response)).into_response())
}

pub async fn relay_mint_credential(
    State(state): State<Arc<AppState>>,
    body: Option<Json<RelayMintRequest>>,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    let _request = body.map(|b| b.0).unwrap_or_default();
    let credential = format!("pair_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
    let expires_at = Utc::now() + chrono::Duration::minutes(5);

    let scopes = vec![
        "orchestration:read".to_string(),
        "orchestration:operate".to_string(),
    ];
    let _ = state
        .storage
        .create_pairing_link(&credential, &scopes, expires_at);

    let proof = format!("mint_proof_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
    let response = RelayMintResponse {
        credential,
        expires_at: expires_at.to_rfc3339(),
        proof,
    };

    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));

    Ok((StatusCode::OK, headers, Json(response)).into_response())
}

pub async fn link_proof(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<LinkProofRequest>,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    authenticate_session(&headers, &state)?;

    let proof = format!(
        "link_proof_{}_{}",
        payload.challenge,
        uuid::Uuid::new_v4().to_string().replace('-', "")
    );

    let mut resp_headers = HeaderMap::new();
    resp_headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    resp_headers.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));

    Ok((StatusCode::OK, resp_headers, Json(proof)).into_response())
}

pub async fn relay_config(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RelayConfigRequest>,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    authenticate_session(&headers, &state)?;

    let home = &state.home_dir;
    if let Some(user_id) = &payload.cloud_user_id {
        let _ = write_secret(home, CLOUD_LINKED_USER_ID_SECRET, user_id.as_bytes());
    }
    let _ = write_secret(home, RELAY_URL_SECRET, payload.relay_url.as_bytes());
    let _ = set_desired_cloud_link(home, true, "managed");

    let result = RelayConfigResult {
        ok: true,
        endpoint_runtime_status: serde_json::json!({ "status": "ready" }),
    };

    let mut resp_headers = HeaderMap::new();
    resp_headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    resp_headers.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));

    Ok((StatusCode::OK, resp_headers, Json(result)).into_response())
}

pub async fn link_state(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    authenticate_session(&headers, &state)?;

    let status = get_cloud_status(&state.home_dir, false)
        .await
        .unwrap_or_else(|_| crate::cloud::status::CloudStatus {
            desired: false,
            authenticated: false,
            linked: false,
            revoked: false,
            cloud_user_id: None,
            relay_url: None,
            publish_agent_activity: false,
            link_mode: "managed".to_string(),
            relay_client_available: false,
            relay_client_version: None,
            relay_client_path: None,
            relay_client_source: None,
            reachability: None,
        });

    let reachability_json = status
        .reachability
        .as_ref()
        .and_then(|r| serde_json::to_value(r).ok());

    let response = LinkStateResult {
        linked: status.linked,
        cloud_user_id: status.cloud_user_id,
        relay_url: status.relay_url.clone(),
        relay_issuer: status.relay_url,
        managed_tunnel_active: Some(status.desired && status.link_mode == "managed"),
        publish_agent_activity: status.publish_agent_activity,
        hold_webhooks_while_offline: Some(false),
        link_revoked: Some(status.revoked),
        reachability: reachability_json,
        tunnel_hostname: None,
        linkable_here: Some(true),
    };

    Ok((StatusCode::OK, Json(response)).into_response())
}

pub async fn check_reachability_endpoint(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    authenticate_session(&headers, &state)?;

    let status = get_cloud_status(&state.home_dir, false).await.ok();
    let connector_available = status.map(|s| s.relay_client_available).unwrap_or(false);

    let reachability = check_reachability(connector_available, None, None)
        .await
        .unwrap_or_else(|_| crate::cloud::reachability::ReachabilityStatus::Unreachable {
            checked_at: Utc::now().to_rfc3339(),
            cause: "not_linked".to_string(),
            reason: "T3 Connect reachability check failed".to_string(),
            hint: None,
            fix: "Run `t3 connect link`".to_string(),
        });

    Ok((StatusCode::OK, Json(reachability)).into_response())
}

pub async fn unlink(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    authenticate_session(&headers, &state)?;

    let home_str = state.home_dir.to_string_lossy().to_string();
    let _ = connect_unlink(Some(&home_str)).await;

    let result = RelayConfigResult {
        ok: true,
        endpoint_runtime_status: serde_json::json!({ "status": "unlinked" }),
    };

    Ok((StatusCode::OK, Json(result)).into_response())
}
