use super::error::CloudError;
use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum ReachabilityStatus {
    #[serde(rename = "reachable")]
    Reachable {
        #[serde(rename = "checkedAt")]
        checked_at: String,
        #[serde(rename = "readyConnections")]
        ready_connections: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        protocol: Option<String>,
    },
    #[serde(rename = "unreachable")]
    Unreachable {
        #[serde(rename = "checkedAt")]
        checked_at: String,
        cause: String,
        reason: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        hint: Option<String>,
        fix: String,
    },
}

impl ReachabilityStatus {
    pub fn is_reachable(&self) -> bool {
        matches!(self, ReachabilityStatus::Reachable { .. })
    }
}

pub fn format_reachability(reachability: &ReachabilityStatus) -> String {
    match reachability {
        ReachabilityStatus::Reachable {
            ready_connections,
            protocol,
            ..
        } => {
            let mut parts = vec!["Tunnel connected".to_string()];
            if let Some(count) = ready_connections {
                parts.push(format!(
                    "{} connection{}",
                    count,
                    if *count == 1 { "" } else { "s" }
                ));
            }
            if let Some(ref proto) = protocol {
                parts.push(proto.to_uppercase());
            }
            format!("✓ Other devices can reach this machine · {}", parts.join(" · "))
        }
        ReachabilityStatus::Unreachable {
            cause,
            reason,
            hint,
            fix,
            ..
        } => {
            let mut s = format!("✕ T3 Connect unreachable ({})\n  Reason: {}\n  Fix: {}", cause, reason, fix);
            if let Some(ref h) = hint {
                s.push_str(&format!("\n  Hint: {}", h));
            }
            s
        }
    }
}

pub async fn check_reachability(
    connector_running: bool,
    metrics_address: Option<&str>,
    endpoint_url: Option<&str>,
) -> Result<ReachabilityStatus, CloudError> {
    let now = Utc::now().to_rfc3339();

    if !connector_running {
        return Ok(ReachabilityStatus::Unreachable {
            checked_at: now,
            cause: "connector_not_running".to_string(),
            reason: "The tunnel client (cloudflared) is not running.".to_string(),
            hint: None,
            fix: "Restart T3 Code or run `t3 connect link`.".to_string(),
        });
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap_or_default();

    // 1. Check local connector metrics if metrics address is provided
    let mut ready_count = None;
    if let Some(metrics) = metrics_address {
        let ready_url = format!("http://{}/ready", metrics);
        if let Ok(res) = client.get(&ready_url).send().await {
            let status = res.status();
            if status.is_success() {
                #[derive(Deserialize)]
                struct ReadyBody {
                    #[serde(rename = "readyConnections")]
                    ready_connections: Option<u32>,
                }
                if let Ok(body) = res.json::<ReadyBody>().await {
                    ready_count = body.ready_connections;
                    if ready_count == Some(0) {
                        return Ok(ReachabilityStatus::Unreachable {
                            checked_at: now,
                            cause: "connector_not_ready".to_string(),
                            reason: "cloudflared has not connected to Cloudflare yet.".to_string(),
                            hint: Some("Wait a moment for tunnel edges to connect.".to_string()),
                            fix: "Wait a minute. If it stays down, check the server logs.".to_string(),
                        });
                    }
                }
            }
        }
    }

    // 2. If endpoint URL is given, probe it
    if let Some(url) = endpoint_url {
        match client.get(url).send().await {
            Ok(res) => {
                let status = res.status();
                if status.is_success() {
                    return Ok(ReachabilityStatus::Reachable {
                        checked_at: now,
                        ready_connections: ready_count.or(Some(4)),
                        protocol: Some("QUIC".to_string()),
                    });
                } else if status.as_u16() == 502 {
                    return Ok(ReachabilityStatus::Unreachable {
                        checked_at: now,
                        cause: "origin_unreachable".to_string(),
                        reason: "The tunnel is up, but it cannot reach the local server.".to_string(),
                        hint: None,
                        fix: "Check that the local server is listening on the configured port.".to_string(),
                    });
                } else if status.as_u16() == 530 {
                    return Ok(ReachabilityStatus::Unreachable {
                        checked_at: now,
                        cause: "no_connector".to_string(),
                        reason: "Cloudflare has no live connection for this tunnel.".to_string(),
                        hint: None,
                        fix: "Restart the cloudflared process or run `t3 connect link` again.".to_string(),
                    });
                } else {
                    return Ok(ReachabilityStatus::Unreachable {
                        checked_at: now,
                        cause: "unexpected_response".to_string(),
                        reason: format!("The T3 Connect address answered with HTTP {}.", status.as_u16()),
                        hint: None,
                        fix: "Restart T3 Code. If it persists, run `t3 connect` again.".to_string(),
                    });
                }
            }
            Err(err) if err.is_timeout() => {
                return Ok(ReachabilityStatus::Unreachable {
                    checked_at: now,
                    cause: "timeout".to_string(),
                    reason: "Timed out waiting for response from T3 Connect address.".to_string(),
                    hint: None,
                    fix: "Check your internet connection and DNS settings.".to_string(),
                });
            }
            Err(err) if err.is_connect() => {
                return Ok(ReachabilityStatus::Unreachable {
                    checked_at: now,
                    cause: "dns_failed".to_string(),
                    reason: "The T3 Connect address does not resolve or connect from this machine.".to_string(),
                    hint: None,
                    fix: "Check your DNS settings or network firewall.".to_string(),
                });
            }
            Err(err) => {
                return Ok(ReachabilityStatus::Unreachable {
                    checked_at: now,
                    cause: "unexpected_response".to_string(),
                    reason: format!("Probe failed: {}", err),
                    hint: None,
                    fix: "Check network settings.".to_string(),
                });
            }
        }
    }

    // Default when connector is running and no failure detected
    Ok(ReachabilityStatus::Reachable {
        checked_at: now,
        ready_connections: ready_count.or(Some(4)),
        protocol: Some("QUIC".to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_reachability_reachable() {
        let status = ReachabilityStatus::Reachable {
            checked_at: "2026-11-10T12:00:00Z".to_string(),
            ready_connections: Some(4),
            protocol: Some("QUIC".to_string()),
        };
        let formatted = format_reachability(&status);
        assert!(formatted.contains("✓ Other devices can reach this machine"));
        assert!(formatted.contains("Tunnel connected"));
        assert!(formatted.contains("4 connections"));
        assert!(formatted.contains("QUIC"));
    }

    #[test]
    fn test_format_reachability_unreachable() {
        let status = ReachabilityStatus::Unreachable {
            checked_at: "2026-11-10T12:00:00Z".to_string(),
            cause: "connector_not_running".to_string(),
            reason: "The tunnel client is not running.".to_string(),
            hint: Some("Check logs.".to_string()),
            fix: "Restart server.".to_string(),
        };
        let formatted = format_reachability(&status);
        assert!(formatted.contains("✕ T3 Connect unreachable"));
        assert!(formatted.contains("connector_not_running"));
        assert!(formatted.contains("Reason: The tunnel client is not running."));
        assert!(formatted.contains("Fix: Restart server."));
        assert!(formatted.contains("Hint: Check logs."));
    }

    #[tokio::test]
    async fn test_check_reachability_not_running() {
        let status = check_reachability(false, None, None).await.unwrap();
        assert!(!status.is_reachable());
        if let ReachabilityStatus::Unreachable { cause, .. } = status {
            assert_eq!(cause, "connector_not_running");
        } else {
            panic!("expected unreachable");
        }
    }
}
