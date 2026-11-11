use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::protocol::models::{ExecutionEnvironmentDescriptor, ServerAuthDescriptor};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthSessionState {
    pub authenticated: bool,
    pub auth: ServerAuthDescriptor,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scopes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenExchangeRequest {
    pub grant_type: Option<String>,
    pub subject_token: String,
    pub subject_token_type: Option<String>,
    pub requested_token_type: Option<String>,
    pub scope: Option<String>,
    pub client_label: Option<String>,
    pub client_device_type: Option<String>,
    pub client_os: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenExchangeResponse {
    pub access_token: String,
    pub issued_token_type: String,
    pub token_type: String,
    pub expires_in: i64,
    pub scope: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CreatePairingTokenInput {
    pub label: Option<String>,
    pub scopes: Option<Vec<String>>,
    pub access: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingCredentialResult {
    pub id: String,
    pub credential: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub expires_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSocketTicketResult {
    pub ticket: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RelayHealthRequest {
    #[serde(default)]
    pub proof: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayHealthResponse {
    pub environment_id: String,
    pub status: String,
    pub descriptor: ExecutionEnvironmentDescriptor,
    pub checked_at: String,
    pub proof: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RelayMintRequest {
    #[serde(default)]
    pub proof: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayMintResponse {
    pub credential: String,
    pub expires_at: String,
    pub proof: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LinkProofRequest {
    pub challenge: String,
    #[serde(default)]
    pub relay_issuer: Option<String>,
    #[serde(default)]
    pub endpoint: Option<serde_json::Value>,
    #[serde(default)]
    pub origin: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RelayConfigRequest {
    pub relay_url: String,
    pub relay_issuer: Option<String>,
    #[serde(default)]
    pub cloud_user_id: Option<String>,
    #[serde(default)]
    pub environment_credential: Option<String>,
    #[serde(default)]
    pub cloud_mint_public_key: Option<String>,
    pub endpoint_runtime: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayConfigResult {
    pub ok: bool,
    pub endpoint_runtime_status: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkStateResult {
    pub linked: bool,
    pub cloud_user_id: Option<String>,
    pub relay_url: Option<String>,
    pub relay_issuer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub managed_tunnel_active: Option<bool>,
    pub publish_agent_activity: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hold_webhooks_while_offline: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_revoked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reachability: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tunnel_hostname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linkable_here: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorResponse {
    pub code: String,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}
