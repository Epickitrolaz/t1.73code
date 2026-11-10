use super::config::{base64_decode_string, CloudConfig};
use super::error::CloudError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceAuthorizationResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_uri_complete: Option<String>,
    pub expires_in: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interval: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OAuthTokenResponse {
    pub access_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id_token: Option<String>,
    pub expires_in: u64,
    pub token_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OAuthErrorResponse {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedToken {
    #[serde(rename = "accessToken")]
    pub access_token: String,
    #[serde(rename = "refreshToken")]
    pub refresh_token: String,
    #[serde(rename = "expiresAtEpochMs")]
    pub expires_at_epoch_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
}

#[derive(Deserialize)]
struct OidcClaims {
    email: Option<String>,
    preferred_username: Option<String>,
    sub: Option<String>,
}

pub fn extract_identity_from_id_token(id_token: &str) -> Option<String> {
    let parts: Vec<&str> = id_token.split('.').collect();
    if parts.len() < 2 {
        return None;
    }

    let payload_str = base64_decode_string(parts[1]).ok()?;
    let claims: OidcClaims = serde_json::from_str(&payload_str).ok()?;

    claims
        .email
        .filter(|s| !s.is_empty())
        .or_else(|| claims.preferred_username.filter(|s| !s.is_empty()))
        .or_else(|| claims.sub.filter(|s| !s.is_empty()))
}

pub fn format_device_authorization_prompt(prompt: &DeviceAuthorizationResponse) -> String {
    let minutes = (prompt.expires_in + 59) / 60;
    let url = prompt
        .verification_uri_complete
        .as_deref()
        .unwrap_or(&prompt.verification_uri);

    format!(
        "Sign in to T3 Connect. Open on any device:\n  {}\nCode: {}\nWaiting for approval. Expires in {} min. Ctrl+C cancels.",
        url, prompt.user_code, minutes
    )
}

pub async fn request_device_authorization(
    client: &reqwest::Client,
    config: &CloudConfig,
) -> Result<DeviceAuthorizationResponse, CloudError> {
    let mut params = HashMap::new();
    params.insert("client_id", config.client_id.as_str());
    params.insert("scope", config.scopes.as_str());

    let response = client
        .post(&config.device_authorization_endpoint)
        .form(&params)
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(CloudError::DeviceAuthorizationFailed(format!(
            "HTTP {}: {}",
            status, text
        )));
    }

    let body = response.json::<DeviceAuthorizationResponse>().await?;
    Ok(body)
}

pub async fn poll_device_token(
    client: &reqwest::Client,
    config: &CloudConfig,
    device_code: &str,
    initial_interval_secs: u64,
    expires_in_secs: u64,
) -> Result<PersistedToken, CloudError> {
    let mut interval = Duration::from_secs(initial_interval_secs.max(1));
    let deadline = SystemTime::now() + Duration::from_secs(expires_in_secs);

    let grant_type = "urn:ietf:params:oauth:grant-type:device_code";

    while SystemTime::now() < deadline {
        tokio::time::sleep(interval).await;

        let mut params = HashMap::new();
        params.insert("grant_type", grant_type);
        params.insert("device_code", device_code);
        params.insert("client_id", config.client_id.as_str());

        let response = match client.post(&config.token_endpoint).form(&params).send().await {
            Ok(res) => res,
            Err(_) => {
                // Network error: back off slightly and retry
                interval += Duration::from_secs(5);
                continue;
            }
        };

        let status = response.status();
        if status.is_success() {
            let token_resp: OAuthTokenResponse = response.json().await?;
            let now_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);

            let identity = token_resp
                .id_token
                .as_deref()
                .and_then(extract_identity_from_id_token);

            return Ok(PersistedToken {
                access_token: token_resp.access_token,
                refresh_token: token_resp.refresh_token.unwrap_or_default(),
                expires_at_epoch_ms: now_ms + token_resp.expires_in * 1000,
                identity,
            });
        }

        // Check if known OAuth error response
        if let Ok(error_resp) = response.json::<OAuthErrorResponse>().await {
            match error_resp.error.as_str() {
                "authorization_pending" => {
                    continue;
                }
                "slow_down" => {
                    interval += Duration::from_secs(5);
                    continue;
                }
                "expired_token" => {
                    return Err(CloudError::AuthorizationExpired);
                }
                "access_denied" => {
                    return Err(CloudError::AuthorizationDenied);
                }
                other => {
                    return Err(CloudError::TokenExchangeFailed(format!(
                        "{}: {}",
                        other,
                        error_resp.error_description.unwrap_or_default()
                    )));
                }
            }
        }
    }

    Err(CloudError::AuthorizationExpired)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_device_authorization_response() {
        let json = r#"{
            "device_code": "dev_12345",
            "user_code": "WDJB-MJHT",
            "verification_uri": "https://clerk.t3.codes/oauth/verify",
            "verification_uri_complete": "https://clerk.t3.codes/oauth/verify?user_code=WDJB-MJHT",
            "expires_in": 900,
            "interval": 5
        }"#;

        let parsed: DeviceAuthorizationResponse = serde_json::from_str(json).expect("should parse");
        assert_eq!(parsed.device_code, "dev_12345");
        assert_eq!(parsed.user_code, "WDJB-MJHT");
        assert_eq!(parsed.verification_uri, "https://clerk.t3.codes/oauth/verify");
        assert_eq!(
            parsed.verification_uri_complete,
            Some("https://clerk.t3.codes/oauth/verify?user_code=WDJB-MJHT".to_string())
        );
        assert_eq!(parsed.expires_in, 900);
        assert_eq!(parsed.interval, Some(5));
    }

    #[test]
    fn test_format_device_authorization_prompt() {
        let resp = DeviceAuthorizationResponse {
            device_code: "dev_123".to_string(),
            user_code: "WDJB-MJHT".to_string(),
            verification_uri: "https://clerk.t3.codes/oauth/verify".to_string(),
            verification_uri_complete: Some(
                "https://clerk.t3.codes/oauth/verify?user_code=WDJB-MJHT".to_string(),
            ),
            expires_in: 900,
            interval: Some(5),
        };

        let prompt = format_device_authorization_prompt(&resp);
        assert!(prompt.contains("Sign in to T3 Connect"));
        assert!(prompt.contains("https://clerk.t3.codes/oauth/verify?user_code=WDJB-MJHT"));
        assert!(prompt.contains("Code: WDJB-MJHT"));
        assert!(prompt.contains("Expires in 15 min"));
    }

    #[test]
    fn test_parse_oauth_token_response() {
        let json = r#"{
            "access_token": "acc_xyz",
            "refresh_token": "ref_abc",
            "expires_in": 3600,
            "token_type": "Bearer"
        }"#;

        let token: OAuthTokenResponse = serde_json::from_str(json).expect("should parse");
        assert_eq!(token.access_token, "acc_xyz");
        assert_eq!(token.refresh_token, Some("ref_abc".to_string()));
        assert_eq!(token.expires_in, 3600);
        assert_eq!(token.token_type, "Bearer");
    }

    #[test]
    fn test_extract_identity_from_id_token() {
        // Base64url payload: {"sub":"user_42","email":"developer@example.com"}
        // {"sub":"user_42","email":"developer@example.com"} in base64:
        // eyJzdWIiOiJ1c2VyXzQyIiwiZW1haWwiOiJkZXZlbG9wZXJAZXhhbXBsZS5jb20ifQ==
        let id_token = "header.eyJzdWIiOiJ1c2VyXzQyIiwiZW1haWwiOiJkZXZlbG9wZXJAZXhhbXBsZS5jb20ifQ.signature";
        let identity = extract_identity_from_id_token(id_token);
        assert_eq!(identity, Some("developer@example.com".to_string()));

        let id_token_sub_only = "header.eyJzdWIiOiJ1c2VyXzQyIn0.sig";
        let identity_sub = extract_identity_from_id_token(id_token_sub_only);
        assert_eq!(identity_sub, Some("user_42".to_string()));
    }

    #[test]
    fn test_oauth_error_response_parsing() {
        let pending_json = r#"{"error":"authorization_pending"}"#;
        let err: OAuthErrorResponse = serde_json::from_str(pending_json).unwrap();
        assert_eq!(err.error, "authorization_pending");

        let slow_down_json = r#"{"error":"slow_down","error_description":"Slow down polling"}"#;
        let err_slow: OAuthErrorResponse = serde_json::from_str(slow_down_json).unwrap();
        assert_eq!(err_slow.error, "slow_down");
        assert_eq!(err_slow.error_description, Some("Slow down polling".to_string()));
    }

    #[test]
    fn test_persisted_token_serialization() {
        let token = PersistedToken {
            access_token: "tok_123".to_string(),
            refresh_token: "ref_456".to_string(),
            expires_at_epoch_ms: 1700000000000,
            identity: Some("user@t3.codes".to_string()),
        };

        let json = serde_json::to_string(&token).unwrap();
        assert!(json.contains("\"accessToken\":\"tok_123\""));
        assert!(json.contains("\"identity\":\"user@t3.codes\""));

        let deserialized: PersistedToken = serde_json::from_str(&json).unwrap();
        assert_eq!(token, deserialized);
    }
}
