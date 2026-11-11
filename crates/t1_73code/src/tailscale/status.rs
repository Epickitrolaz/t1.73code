use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug)]
pub enum TailscaleError {
    CommandSpawnFailed {
        command: String,
        source: std::io::Error,
    },
    CommandFailed {
        command: String,
        code: Option<i32>,
        stderr: String,
    },
    JsonParseError(serde_json::Error),
    InvalidOutput(String),
    Timeout(String),
}

impl fmt::Display for TailscaleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TailscaleError::CommandSpawnFailed { command, source } => {
                write!(f, "Failed to spawn command '{}': {}", command, source)
            }
            TailscaleError::CommandFailed {
                command,
                code,
                stderr,
            } => {
                write!(
                    f,
                    "Command '{}' failed with exit code {:?}: {}",
                    command, code, stderr
                )
            }
            TailscaleError::JsonParseError(err) => {
                write!(f, "Failed to parse tailscale status JSON: {}", err)
            }
            TailscaleError::InvalidOutput(msg) => {
                write!(f, "Invalid tailscale output: {}", msg)
            }
            TailscaleError::Timeout(msg) => {
                write!(f, "Tailscale command timed out: {}", msg)
            }
        }
    }
}

impl std::error::Error for TailscaleError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TailscaleError::CommandSpawnFailed { source, .. } => Some(source),
            TailscaleError::JsonParseError(err) => Some(err),
            _ => None,
        }
    }
}

impl From<serde_json::Error> for TailscaleError {
    fn from(err: serde_json::Error) -> Self {
        TailscaleError::JsonParseError(err)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TailscaleStatus {
    pub fqdn: Option<String>,
    pub ips: Vec<String>,
    pub ipv4: Option<String>,
    pub https_enabled: Option<bool>,
}

impl TailscaleStatus {
    pub fn pairing_url(&self, port: Option<u16>, token: &str) -> Option<String> {
        let https = self.https_enabled.unwrap_or(false);
        if let Some(ref fqdn) = self.fqdn {
            Some(generate_pairing_url(fqdn, port, token, https))
        } else if let Some(ref ipv4) = self.ipv4 {
            Some(generate_pairing_url(ipv4, port, token, false))
        } else {
            None
        }
    }
}

#[derive(Deserialize)]
struct RawStatus {
    #[serde(rename = "Self")]
    self_node: Option<RawSelfNode>,
}

#[derive(Deserialize)]
struct RawSelfNode {
    #[serde(rename = "DNSName")]
    dns_name: Option<String>,
    #[serde(rename = "TailscaleIPs")]
    tailscale_ips: Option<Vec<String>>,
    #[serde(rename = "CapMap")]
    cap_map: Option<serde_json::Value>,
}

pub fn is_tailscale_ipv4(address: &str) -> bool {
    let parts: Vec<&str> = address.split('.').collect();
    if parts.len() != 4 {
        return false;
    }

    let mut octets = [0u8; 4];
    for (i, part) in parts.iter().enumerate() {
        match part.parse::<u8>() {
            Ok(val) => octets[i] = val,
            Err(_) => return false,
        }
    }

    octets[0] == 100 && (64..=127).contains(&octets[1])
}

pub fn normalize_fqdn(raw: &str) -> Option<String> {
    let trimmed = raw.trim().trim_end_matches('.');
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub fn parse_tailscale_status(json_str: &str) -> Result<TailscaleStatus, TailscaleError> {
    let raw: RawStatus = serde_json::from_str(json_str)?;

    let (fqdn, ips, ipv4, https_enabled) = match raw.self_node {
        Some(self_node) => {
            let fqdn = self_node
                .dns_name
                .as_deref()
                .and_then(normalize_fqdn);

            let raw_ips = self_node.tailscale_ips.unwrap_or_default();
            let mut tailnet_ipv4 = None;

            for ip in &raw_ips {
                if is_tailscale_ipv4(ip) {
                    tailnet_ipv4 = Some(ip.clone());
                    break;
                }
            }

            // If no 100.64.0.0/10 found, try any standard IPv4
            if tailnet_ipv4.is_none() {
                for ip in &raw_ips {
                    if ip.parse::<std::net::Ipv4Addr>().is_ok() {
                        tailnet_ipv4 = Some(ip.clone());
                        break;
                    }
                }
            }

            let https_enabled = match self_node.cap_map {
                Some(serde_json::Value::Object(map)) => Some(map.contains_key("https")),
                _ => None,
            };

            (fqdn, raw_ips, tailnet_ipv4, https_enabled)
        }
        None => (None, Vec::new(), None, None),
    };

    Ok(TailscaleStatus {
        fqdn,
        ips,
        ipv4,
        https_enabled,
    })
}

pub fn generate_pairing_url(host: &str, port: Option<u16>, token: &str, https: bool) -> String {
    let scheme = if https { "https" } else { "http" };
    let default_port = if https { 443 } else { 80 };

    let host_port = match port {
        Some(p) if p != default_port => {
            if host.contains(':') && !host.starts_with('[') {
                format!("[{}]:{}", host, p)
            } else {
                format!("{}:{}", host, p)
            }
        }
        _ => {
            if host.contains(':') && !host.starts_with('[') {
                format!("[{}]", host)
            } else {
                host.to_string()
            }
        }
    };

    format!("{}://{}/pair#token={}", scheme, host_port, token)
}

pub fn format_tailscale_status(status: &TailscaleStatus) -> String {
    let https_str = match status.https_enabled {
        Some(true) => "enabled",
        Some(false) => "disabled",
        None => "unknown",
    };

    format!(
        "Tailscale Status:\n  FQDN: {}\n  IPv4: {}\n  HTTPS: {}\n  IPs: [{}]",
        status.fqdn.as_deref().unwrap_or("none"),
        status.ipv4.as_deref().unwrap_or("none"),
        https_str,
        status.ips.join(", ")
    )
}

pub fn format_tailscale_status_json(status: &TailscaleStatus) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_tailscale_ipv4() {
        assert!(is_tailscale_ipv4("100.64.0.1"));
        assert!(is_tailscale_ipv4("100.127.219.6"));
        assert!(is_tailscale_ipv4("100.100.100.100"));
        assert!(!is_tailscale_ipv4("100.63.255.255"));
        assert!(!is_tailscale_ipv4("100.128.0.0"));
        assert!(!is_tailscale_ipv4("192.168.1.1"));
        assert!(!is_tailscale_ipv4("127.0.0.1"));
        assert!(!is_tailscale_ipv4("not-an-ip"));
    }

    #[test]
    fn test_normalize_fqdn() {
        assert_eq!(
            normalize_fqdn("my-machine.tailnet.ts.net."),
            Some("my-machine.tailnet.ts.net".to_string())
        );
        assert_eq!(
            normalize_fqdn("my-machine.tailnet.ts.net"),
            Some("my-machine.tailnet.ts.net".to_string())
        );
        assert_eq!(normalize_fqdn("  "), None);
        assert_eq!(normalize_fqdn("."), None);
    }

    #[test]
    fn test_parse_tailscale_status_full() {
        let json = r#"{
            "Self": {
                "DNSName": "coding-lxc.tail047f6e.ts.net.",
                "TailscaleIPs": ["100.127.219.6", "fd7a:115c:a1e0::d929:db07"],
                "CapMap": {
                    "https": true,
                    "ssh": null
                }
            }
        }"#;

        let status = parse_tailscale_status(json).expect("should parse");
        assert_eq!(
            status.fqdn,
            Some("coding-lxc.tail047f6e.ts.net".to_string())
        );
        assert_eq!(status.ipv4, Some("100.127.219.6".to_string()));
        assert_eq!(
            status.ips,
            vec![
                "100.127.219.6".to_string(),
                "fd7a:115c:a1e0::d929:db07".to_string()
            ]
        );
        assert_eq!(status.https_enabled, Some(true));
    }

    #[test]
    fn test_parse_tailscale_status_minimal() {
        let json = r#"{
            "Self": {
                "DNSName": "box.ts.net",
                "TailscaleIPs": ["100.80.1.2"]
            }
        }"#;

        let status = parse_tailscale_status(json).expect("should parse");
        assert_eq!(status.fqdn, Some("box.ts.net".to_string()));
        assert_eq!(status.ipv4, Some("100.80.1.2".to_string()));
        assert_eq!(status.https_enabled, None);
    }

    #[test]
    fn test_parse_tailscale_status_empty() {
        let json = r#"{}"#;
        let status = parse_tailscale_status(json).expect("should parse");
        assert_eq!(status.fqdn, None);
        assert_eq!(status.ipv4, None);
        assert!(status.ips.is_empty());
        assert_eq!(status.https_enabled, None);
    }

    #[test]
    fn test_generate_pairing_url() {
        let url1 = generate_pairing_url("my-host.ts.net", None, "secret123", true);
        assert_eq!(url1, "https://my-host.ts.net/pair#token=secret123");

        let url2 = generate_pairing_url("my-host.ts.net", Some(8443), "secret123", true);
        assert_eq!(url2, "https://my-host.ts.net:8443/pair#token=secret123");

        let url3 = generate_pairing_url("100.127.219.6", Some(3000), "tok_abc", false);
        assert_eq!(url3, "http://100.127.219.6:3000/pair#token=tok_abc");

        let url4 = generate_pairing_url("100.127.219.6", Some(80), "tok_abc", false);
        assert_eq!(url4, "http://100.127.219.6/pair#token=tok_abc");
    }

    #[test]
    fn test_status_pairing_url_method() {
        let status = TailscaleStatus {
            fqdn: Some("my-host.ts.net".to_string()),
            ips: vec!["100.64.1.2".to_string()],
            ipv4: Some("100.64.1.2".to_string()),
            https_enabled: Some(true),
        };
        let url = status.pairing_url(None, "my_token").unwrap();
        assert_eq!(url, "https://my-host.ts.net/pair#token=my_token");

        let status_http = TailscaleStatus {
            fqdn: None,
            ips: vec!["100.64.1.2".to_string()],
            ipv4: Some("100.64.1.2".to_string()),
            https_enabled: None,
        };
        let url_http = status_http.pairing_url(Some(3000), "my_token").unwrap();
        assert_eq!(url_http, "http://100.64.1.2:3000/pair#token=my_token");
    }

    #[test]
    fn test_format_tailscale_status() {
        let status = TailscaleStatus {
            fqdn: Some("my-host.ts.net".to_string()),
            ips: vec!["100.64.1.2".to_string()],
            ipv4: Some("100.64.1.2".to_string()),
            https_enabled: Some(true),
        };
        let formatted = format_tailscale_status(&status);
        assert!(formatted.contains("my-host.ts.net"));
        assert!(formatted.contains("100.64.1.2"));
        assert!(formatted.contains("HTTPS: enabled"));

        let json_formatted = format_tailscale_status_json(&status).unwrap();
        assert!(json_formatted.contains("\"fqdn\": \"my-host.ts.net\""));
    }
}
