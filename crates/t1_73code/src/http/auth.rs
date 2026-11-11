use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use std::sync::Arc;
use crate::http::models::{
    AuthSessionState, CreatePairingTokenInput, ErrorResponse, PairingCredentialResult,
    TokenExchangeRequest, TokenExchangeResponse, WebSocketTicketResult,
};
use crate::protocol::models::ServerAuthDescriptor;
use crate::state::AppState;
use crate::storage::models::AuthSession;

pub fn default_server_auth_descriptor() -> ServerAuthDescriptor {
    ServerAuthDescriptor {
        active_methods: vec![
            "bearer-access-token".to_string(),
            "browser-session-cookie".to_string(),
            "one-time-token".to_string(),
        ],
        token_exchange: true,
    }
}

pub fn extract_token(headers: &HeaderMap) -> Option<String> {
    if let Some(auth) = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()) {
        let auth = auth.trim();
        if let Some(token) = auth.strip_prefix("Bearer ").or_else(|| auth.strip_prefix("bearer ")) {
            let t = token.trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        } else if !auth.is_empty() {
            return Some(auth.to_string());
        }
    }

    if let Some(cookie_header) = headers.get(header::COOKIE).and_then(|v| v.to_str().ok()) {
        for cookie in cookie_header.split(';') {
            let cookie = cookie.trim();
            if let Some((name, val)) = cookie.split_once('=') {
                let name = name.trim();
                let val = val.trim();
                if (name.starts_with("t3_session") || name.starts_with("t3_dev_session") || name == "session")
                    && !val.is_empty()
                {
                    return Some(val.to_string());
                }
            }
        }
    }

    None
}

pub fn authenticate_session(
    headers: &HeaderMap,
    state: &AppState,
) -> Result<AuthSession, (StatusCode, Json<ErrorResponse>)> {
    let token = extract_token(headers).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                code: "auth_invalid".to_string(),
                reason: "missing_credential".to_string(),
                message: Some("Authentication credential required".to_string()),
            }),
        )
    })?;

    if let Ok(dev_token) = std::env::var("T3CODE_DEV_AUTH_TOKEN") {
        if !dev_token.is_empty() && token == dev_token {
            let now = Utc::now();
            return Ok(AuthSession {
                session_id: "dev-session".to_string(),
                subject: "developer".to_string(),
                scopes: vec![
                    "orchestration:read".to_string(),
                    "orchestration:operate".to_string(),
                    "access:write".to_string(),
                    "access:read".to_string(),
                    "relay:read".to_string(),
                    "relay:write".to_string(),
                ],
                method: "bearer-access-token".to_string(),
                client_label: Some("dev-token".to_string()),
                client_ip_address: None,
                client_user_agent: None,
                client_device_type: "dev".to_string(),
                client_os: None,
                client_browser: None,
                issued_at: now,
                expires_at: now + chrono::Duration::days(365),
                last_connected_at: Some(now),
                revoked_at: None,
            });
        }
    }

    match state.storage.validate_session(&token) {
        Ok(Some(session)) => Ok(session),
        _ => Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                code: "auth_invalid".to_string(),
                reason: "invalid_credential".to_string(),
                message: Some("Invalid, expired, or revoked session".to_string()),
            }),
        )),
    }
}

pub async fn get_session(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let descriptor = default_server_auth_descriptor();

    let session = match authenticate_session(&headers, &state) {
        Ok(s) => Some(s),
        Err(_) => None,
    };

    let response = match session {
        Some(s) => AuthSessionState {
            authenticated: true,
            auth: descriptor,
            scopes: Some(s.scopes.clone()),
            permissions: Some(s.scopes),
            session_method: Some(s.method),
            expires_at: Some(s.expires_at),
        },
        None => AuthSessionState {
            authenticated: false,
            auth: descriptor,
            scopes: None,
            permissions: None,
            session_method: None,
            expires_at: None,
        },
    };

    (StatusCode::OK, Json(response))
}

fn urlencoding_decode(input: &str) -> String {
    let input = input.replace('+', " ");
    let mut result = Vec::new();
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&input[i + 1..i + 3], 16) {
                result.push(b);
                i += 3;
                continue;
            }
        }
        result.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(result).unwrap_or_else(|_| input.to_string())
}

fn parse_form_urlencoded(bytes: &[u8]) -> Option<TokenExchangeRequest> {
    let s = std::str::from_utf8(bytes).ok()?;
    let mut req = TokenExchangeRequest::default();
    let mut found_any = false;
    for part in s.split('&') {
        if let Some((k, v)) = part.split_once('=') {
            found_any = true;
            let k = urlencoding_decode(k);
            let v = urlencoding_decode(v);
            match k.as_str() {
                "grant_type" => req.grant_type = Some(v),
                "subject_token" => req.subject_token = v,
                "subject_token_type" => req.subject_token_type = Some(v),
                "requested_token_type" => req.requested_token_type = Some(v),
                "scope" => req.scope = Some(v),
                "client_label" => req.client_label = Some(v),
                "client_device_type" => req.client_device_type = Some(v),
                "client_os" => req.client_os = Some(v),
                _ => {}
            }
        }
    }
    if found_any && !req.subject_token.is_empty() {
        Some(req)
    } else {
        None
    }
}

pub async fn oauth_token(
    _headers: HeaderMap,
    State(state): State<Arc<AppState>>,
    body: Bytes,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let request: TokenExchangeRequest = if let Ok(parsed) = serde_json::from_slice::<TokenExchangeRequest>(&body) {
        parsed
    } else if let Some(parsed) = parse_form_urlencoded(&body) {
        parsed
    } else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "invalid_request",
                "error_description": "Failed to parse token exchange request",
            })),
        ));
    };

    let subject_token = request.subject_token.trim();
    if subject_token.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "invalid_request",
                "error_description": "Missing subject_token",
            })),
        ));
    }

    let dev_match = std::env::var("T3CODE_DEV_AUTH_TOKEN")
        .map(|dt| !dt.is_empty() && dt == subject_token)
        .unwrap_or(false);

    let (scopes, label) = if dev_match {
        (
            vec![
                "orchestration:read".to_string(),
                "orchestration:operate".to_string(),
                "access:write".to_string(),
                "access:read".to_string(),
                "relay:read".to_string(),
                "relay:write".to_string(),
            ],
            Some("dev-pairing".to_string()),
        )
    } else {
        match state.storage.consume_pairing_link(subject_token) {
            Ok(Some(link)) => (link.scopes, link.label),
            _ => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": "invalid_grant",
                        "error_description": "Invalid, expired, or already consumed pairing token",
                    })),
                ));
            }
        }
    };

    let session_id = uuid::Uuid::new_v4().to_string();
    let access_token = format!("tok_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
    let expires_in: i64 = 30 * 24 * 3600;
    let expires_at = Utc::now() + chrono::Duration::days(30);

    let mut session = state
        .storage
        .create_session(&session_id, &access_token, expires_at)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "internal_error",
                    "error_description": format!("Failed to create auth session: {e}"),
                })),
            )
        })?;

    if !scopes.is_empty() {
        session.scopes = scopes.clone();
    }
    if let Some(l) = label.or(request.client_label) {
        session.client_label = Some(l);
    }
    if let Some(dev_type) = request.client_device_type {
        session.client_device_type = dev_type;
    }
    if let Some(os) = request.client_os {
        session.client_os = Some(os);
    }

    let cookie_val = format!(
        "t3_session={}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}",
        access_token, expires_in
    );

    let token_resp = TokenExchangeResponse {
        access_token,
        issued_token_type: "urn:ietf:params:oauth:token-type:access_token".to_string(),
        token_type: "Bearer".to_string(),
        expires_in,
        scope: scopes.join(" "),
    };

    let mut resp_headers = HeaderMap::new();
    resp_headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    resp_headers.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    if let Ok(cookie_hdr) = HeaderValue::from_str(&cookie_val) {
        resp_headers.insert(header::SET_COOKIE, cookie_hdr);
    }

    Ok((StatusCode::OK, resp_headers, Json(token_resp)).into_response())
}

pub async fn create_pairing_token(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
    body: Option<Json<CreatePairingTokenInput>>,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    authenticate_session(&headers, &state)?;

    let input = body.map(|b| b.0).unwrap_or_default();
    let token = format!("pair_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
    let expires_at = Utc::now() + chrono::Duration::minutes(15);

    let scopes = input.scopes.clone().unwrap_or_else(|| {
        vec![
            "orchestration:read".to_string(),
            "orchestration:operate".to_string(),
            "access:write".to_string(),
            "relay:read".to_string(),
            "relay:write".to_string(),
        ]
    });

    let pairing = state
        .storage
        .create_pairing_link(&token, &scopes, expires_at)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    code: "internal_error".to_string(),
                    reason: "storage_error".to_string(),
                    message: Some(e.to_string()),
                }),
            )
        })?;

    let result = PairingCredentialResult {
        id: pairing.id,
        credential: token,
        label: input.label,
        expires_at,
        access: input.access,
    };

    let mut resp_headers = HeaderMap::new();
    resp_headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    resp_headers.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));

    Ok((StatusCode::OK, resp_headers, Json(result)).into_response())
}

pub async fn create_websocket_ticket(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    authenticate_session(&headers, &state)?;

    let ticket = format!("wst_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
    let expires_at = Utc::now() + chrono::Duration::minutes(5);

    let sid = format!("ticket_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
    let _ = state.storage.create_session(&sid, &ticket, expires_at);

    let result = WebSocketTicketResult { ticket, expires_at };

    let mut resp_headers = HeaderMap::new();
    resp_headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    resp_headers.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));

    Ok((StatusCode::OK, resp_headers, Json(result)).into_response())
}

pub async fn create_websocket_ticket_for(
    Path(_use_param): Path<String>,
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    create_websocket_ticket(headers, State(state)).await
}
