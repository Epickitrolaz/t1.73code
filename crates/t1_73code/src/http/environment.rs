use axum::{
    extract::State,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use std::sync::Arc;
use crate::protocol::models::{
    ExecutionEnvironmentCapabilities, ExecutionEnvironmentDescriptor, ExecutionEnvironmentPlatform,
};
use crate::state::AppState;

pub async fn get_environment(
    State(state): State<Arc<AppState>>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let hostname = std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("HOST"))
        .unwrap_or_else(|_| "localhost".to_string());

    let descriptor = ExecutionEnvironmentDescriptor {
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
    };

    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));

    Ok((StatusCode::OK, headers, Json(descriptor)).into_response())
}
