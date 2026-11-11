use super::error::CloudError;
use std::env;

pub const DEFAULT_RELAY_URL: &str = "https://relay.t3.codes";
pub const DEFAULT_CLERK_PUBLISHABLE_KEY: &str = "pk_live_Y2xlcmsudDMuY29kZXMk";
pub const DEFAULT_CLERK_CLI_OAUTH_CLIENT_ID: &str = "hzxSgY2cH10sDU2r";
pub const DEFAULT_SCOPES: &str = "openid profile email offline_access";

pub fn base64_decode(input: &str) -> Result<Vec<u8>, CloudError> {
    let mut clean = input.trim().replace('-', "+").replace('_', "/");
    let rem = clean.len() % 4;
    if rem != 0 {
        clean.push_str(&"=".repeat(4 - rem));
    }

    let mut out = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0u32;

    for byte in clean.bytes() {
        if byte == b'=' {
            break;
        }
        let val = match byte {
            b'A'..=b'Z' => (byte - b'A') as u32,
            b'a'..=b'z' => (byte - b'a' + 26) as u32,
            b'0'..=b'9' => (byte - b'0' + 52) as u32,
            b'+' => 62,
            b'/' => 63,
            b'\r' | b'\n' | b' ' | b'\t' => continue,
            _ => {
                return Err(CloudError::ConfigError(format!(
                    "Invalid base64 character: {}",
                    byte as char
                )))
            }
        };

        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }

    Ok(out)
}

pub fn base64_decode_string(input: &str) -> Result<String, CloudError> {
    let bytes = base64_decode(input)?;
    String::from_utf8(bytes).map_err(|e| {
        CloudError::ConfigError(format!("Failed to decode UTF-8 from base64: {}", e))
    })
}

pub fn clerk_frontend_api_url_from_publishable_key(key: &str) -> Result<String, CloudError> {
    let parts: Vec<&str> = key.split('_').collect();
    if parts.len() < 3 {
        return Err(CloudError::ConfigError(format!(
            "Invalid Clerk publishable key format: {}",
            key
        )));
    }

    let encoded_frontend_api = parts[2..].join("_");
    let decoded = base64_decode_string(&encoded_frontend_api)?;
    let frontend_api = decoded.trim_end_matches('$');

    if frontend_api.is_empty() {
        return Err(CloudError::ConfigError(
            "Decoded Clerk Frontend API hostname is empty".to_string(),
        ));
    }
    if frontend_api.contains('/') {
        return Err(CloudError::ConfigError(format!(
            "Clerk Frontend API contains invalid path: {}",
            frontend_api
        )));
    }

    Ok(format!("https://{}", frontend_api))
}

#[derive(Debug, Clone)]
pub struct CloudConfig {
    pub relay_url: String,
    pub clerk_publishable_key: String,
    pub client_id: String,
    pub device_authorization_endpoint: String,
    pub token_endpoint: String,
    pub scopes: String,
}

impl CloudConfig {
    pub fn from_env() -> Result<Self, CloudError> {
        let relay_url = env::var("T3CODE_RELAY_URL")
            .unwrap_or_else(|_| DEFAULT_RELAY_URL.to_string())
            .trim()
            .to_string();

        let clerk_publishable_key = env::var("T3CODE_CLERK_PUBLISHABLE_KEY")
            .unwrap_or_else(|_| DEFAULT_CLERK_PUBLISHABLE_KEY.to_string())
            .trim()
            .to_string();

        let client_id = env::var("T3CODE_CLERK_CLI_OAUTH_CLIENT_ID")
            .unwrap_or_else(|_| DEFAULT_CLERK_CLI_OAUTH_CLIENT_ID.to_string())
            .trim()
            .to_string();

        let frontend_api_url =
            clerk_frontend_api_url_from_publishable_key(&clerk_publishable_key)?;
        let device_authorization_endpoint =
            format!("{}/oauth/device_authorization", frontend_api_url);
        let token_endpoint = format!("{}/oauth/token", frontend_api_url);

        Ok(CloudConfig {
            relay_url,
            clerk_publishable_key,
            client_id,
            device_authorization_endpoint,
            token_endpoint,
            scopes: DEFAULT_SCOPES.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64_decode() {
        let decoded = base64_decode_string("Y2xlcmsudDMuY29kZXMk").expect("should decode");
        assert_eq!(decoded, "clerk.t3.codes$");
    }

    #[test]
    fn test_clerk_frontend_api_url_from_publishable_key() {
        let url = clerk_frontend_api_url_from_publishable_key(DEFAULT_CLERK_PUBLISHABLE_KEY)
            .expect("should derive url");
        assert_eq!(url, "https://clerk.t3.codes");
    }

    #[test]
    fn test_cloud_config_from_env() {
        let config = CloudConfig::from_env().expect("config should load");
        assert_eq!(config.relay_url, "https://relay.t3.codes");
        assert_eq!(config.client_id, "hzxSgY2cH10sDU2r");
        assert_eq!(
            config.device_authorization_endpoint,
            "https://clerk.t3.codes/oauth/device_authorization"
        );
        assert_eq!(config.token_endpoint, "https://clerk.t3.codes/oauth/token");
    }
}
