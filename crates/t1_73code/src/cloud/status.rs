use super::error::CloudError;
use super::reachability::{format_reachability, ReachabilityStatus};
use super::storage::{
    get_desired_cloud_link, get_desired_link_mode, load_token, read_secret,
    CLOUD_LINKED_USER_ID_SECRET, CLOUD_LINK_REVOKED_SECRET, PUBLISH_AGENT_ACTIVITY_SECRET,
    RELAY_URL_SECRET,
};
use super::tunnel::resolve_cloudflared;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudStatus {
    pub desired: bool,
    pub authenticated: bool,
    pub linked: bool,
    pub revoked: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloud_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relay_url: Option<String>,
    pub publish_agent_activity: bool,
    pub link_mode: String,
    pub relay_client_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relay_client_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relay_client_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relay_client_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reachability: Option<ReachabilityStatus>,
}

pub async fn get_cloud_status(
    home_dir: &Path,
    check_reach: bool,
) -> Result<CloudStatus, CloudError> {
    let desired = get_desired_cloud_link(home_dir)?;
    let link_mode = get_desired_link_mode(home_dir)?;
    let token = load_token(home_dir)?;
    let authenticated = token.is_some();

    let cloud_user_id = read_secret(home_dir, CLOUD_LINKED_USER_ID_SECRET)?
        .map(|b| String::from_utf8_lossy(&b).trim().to_string());
    let linked = cloud_user_id.is_some();

    let revoked = read_secret(home_dir, CLOUD_LINK_REVOKED_SECRET)?.is_some();

    let relay_url = read_secret(home_dir, RELAY_URL_SECRET)?
        .map(|b| String::from_utf8_lossy(&b).trim().to_string());

    let publish_bytes = read_secret(home_dir, PUBLISH_AGENT_ACTIVITY_SECRET)?;
    let publish_agent_activity = publish_bytes
        .map(|b| String::from_utf8_lossy(&b).trim() == "true")
        .unwrap_or(false);

    let (relay_client_available, relay_client_version, relay_client_path, relay_client_source) =
        match resolve_cloudflared(home_dir).await {
            Ok(exe) => (
                true,
                Some(exe.version),
                Some(exe.path.to_string_lossy().to_string()),
                Some(format!("{:?}", exe.source)),
            ),
            Err(_) => (false, None, None, None),
        };

    let reachability = if check_reach && desired && link_mode == "managed" {
        Some(super::reachability::check_reachability(relay_client_available, None, None).await?)
    } else {
        None
    };

    Ok(CloudStatus {
        desired,
        authenticated,
        linked,
        revoked,
        cloud_user_id,
        relay_url,
        publish_agent_activity,
        link_mode,
        relay_client_available,
        relay_client_version,
        relay_client_path,
        relay_client_source,
        reachability,
    })
}

pub fn format_cloud_status(status: &CloudStatus, json: bool) -> String {
    if json {
        return serde_json::to_string_pretty(status).unwrap_or_default();
    }

    let provisioned = if status.revoked {
        "removed from T3 Connect"
    } else if status.linked {
        "provisioned"
    } else if status.desired && status.authenticated {
        "pending server startup"
    } else {
        "not provisioned"
    };

    let next_step = if !status.authenticated {
        Some("Run `t3 connect link` to authorize and enable T3 Connect.")
    } else if status.revoked {
        Some("Run `t3 connect link` to link this environment again.")
    } else if !status.desired {
        Some("Run `t3 connect link` to enable T3 Connect.")
    } else if !status.linked {
        Some("Start T3 to provision the environment link and launch its managed tunnel.")
    } else {
        None
    };

    let relay_client_lines = if status.relay_client_available {
        let src = status.relay_client_source.as_deref().unwrap_or("PATH");
        format!(
            "  Relay client: available via {}\n    Path: {}\n    Version: {}",
            src,
            status.relay_client_path.as_deref().unwrap_or("unknown"),
            status.relay_client_version.as_deref().unwrap_or("unknown")
        )
    } else {
        "  Relay client: not installed".to_string()
    };

    let mut lines = vec![
        "T3 Connect".to_string(),
        format!("  Exposure: {}", if status.desired { "enabled" } else { "disabled" }),
        format!(
            "  Authorization: {}",
            if status.authenticated {
                "stored credential"
            } else {
                "missing"
            }
        ),
        format!("  Environment link: {}", provisioned),
        format!(
            "  Relay: {}",
            status.relay_url.as_deref().unwrap_or("not provisioned")
        ),
        format!(
            "  Publish agent activity: {}",
            if status.publish_agent_activity {
                "enabled"
            } else {
                "disabled"
            }
        ),
        relay_client_lines,
    ];

    if let Some(ref reach) = status.reachability {
        lines.push(String::new());
        lines.push(format_reachability(reach));
    }

    if let Some(next) = next_step {
        lines.push(String::new());
        lines.push(format!("Next: {}", next));
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_cloud_status_human() {
        let status = CloudStatus {
            desired: true,
            authenticated: true,
            linked: true,
            revoked: false,
            cloud_user_id: Some("usr_123".to_string()),
            relay_url: Some("https://relay.t3.codes".to_string()),
            publish_agent_activity: false,
            link_mode: "managed".to_string(),
            relay_client_available: true,
            relay_client_version: Some("2026.10.4".to_string()),
            relay_client_path: Some("/usr/bin/cloudflared".to_string()),
            relay_client_source: Some("PATH".to_string()),
            reachability: None,
        };

        let formatted = format_cloud_status(&status, false);
        assert!(formatted.contains("T3 Connect"));
        assert!(formatted.contains("Exposure: enabled"));
        assert!(formatted.contains("Authorization: stored credential"));
        assert!(formatted.contains("Environment link: provisioned"));
        assert!(formatted.contains("Relay: https://relay.t3.codes"));
        assert!(formatted.contains("Version: 2026.10.4"));
    }

    #[test]
    fn test_format_cloud_status_unauthenticated() {
        let status = CloudStatus {
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
        };

        let formatted = format_cloud_status(&status, false);
        assert!(formatted.contains("Authorization: missing"));
        assert!(formatted.contains("Relay client: not installed"));
        assert!(formatted.contains("Next: Run `t3 connect link` to authorize and enable T3 Connect."));
    }

    #[test]
    fn test_format_cloud_status_json() {
        let status = CloudStatus {
            desired: true,
            authenticated: true,
            linked: true,
            revoked: false,
            cloud_user_id: Some("usr_abc".to_string()),
            relay_url: Some("https://relay.t3.codes".to_string()),
            publish_agent_activity: true,
            link_mode: "managed".to_string(),
            relay_client_available: true,
            relay_client_version: Some("2026.10.4".to_string()),
            relay_client_path: Some("/bin/cloudflared".to_string()),
            relay_client_source: Some("Path".to_string()),
            reachability: None,
        };

        let json = format_cloud_status(&status, true);
        assert!(json.contains("\"desired\": true"));
        assert!(json.contains("\"cloud_user_id\": \"usr_abc\""));
        assert!(json.contains("\"publish_agent_activity\": true"));
    }
}
