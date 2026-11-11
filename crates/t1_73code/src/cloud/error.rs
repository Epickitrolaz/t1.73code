use std::fmt;

#[derive(Debug)]
pub enum CloudError {
    DeviceAuthorizationFailed(String),
    AuthorizationPending,
    AuthorizationSlowDown,
    AuthorizationExpired,
    AuthorizationDenied,
    TokenExchangeFailed(String),
    StorageError(String),
    CloudflaredNotFound(String),
    CloudflaredFailed(String),
    ReachabilityFailed(String),
    RelayError(String),
    ConfigError(String),
    HttpError(reqwest::Error),
    IoError(std::io::Error),
    JsonError(serde_json::Error),
}

impl fmt::Display for CloudError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CloudError::DeviceAuthorizationFailed(msg) => {
                write!(f, "Device authorization failed: {}", msg)
            }
            CloudError::AuthorizationPending => write!(f, "Authorization is still pending"),
            CloudError::AuthorizationSlowDown => write!(f, "Authorization polling slow down"),
            CloudError::AuthorizationExpired => write!(f, "Device authorization code has expired"),
            CloudError::AuthorizationDenied => {
                write!(f, "T3 Connect authorization was denied in browser")
            }
            CloudError::TokenExchangeFailed(msg) => write!(f, "OAuth token exchange failed: {}", msg),
            CloudError::StorageError(msg) => write!(f, "Credential storage error: {}", msg),
            CloudError::CloudflaredNotFound(msg) => write!(f, "cloudflared binary not found: {}", msg),
            CloudError::CloudflaredFailed(msg) => write!(f, "cloudflared execution failed: {}", msg),
            CloudError::ReachabilityFailed(msg) => write!(f, "Reachability check failed: {}", msg),
            CloudError::RelayError(msg) => write!(f, "Relay API error: {}", msg),
            CloudError::ConfigError(msg) => write!(f, "Configuration error: {}", msg),
            CloudError::HttpError(err) => write!(f, "HTTP request failed: {}", err),
            CloudError::IoError(err) => write!(f, "I/O error: {}", err),
            CloudError::JsonError(err) => write!(f, "JSON error: {}", err),
        }
    }
}

impl std::error::Error for CloudError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CloudError::HttpError(err) => Some(err),
            CloudError::IoError(err) => Some(err),
            CloudError::JsonError(err) => Some(err),
            _ => None,
        }
    }
}

impl From<reqwest::Error> for CloudError {
    fn from(err: reqwest::Error) -> Self {
        CloudError::HttpError(err)
    }
}

impl From<std::io::Error> for CloudError {
    fn from(err: std::io::Error) -> Self {
        CloudError::IoError(err)
    }
}

impl From<serde_json::Error> for CloudError {
    fn from(err: serde_json::Error) -> Self {
        CloudError::JsonError(err)
    }
}
