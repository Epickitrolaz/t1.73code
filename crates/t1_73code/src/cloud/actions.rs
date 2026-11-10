use super::auth::{
    format_device_authorization_prompt, poll_device_token, request_device_authorization,
    PersistedToken,
};
use super::config::CloudConfig;
use super::error::CloudError;
use super::status::{format_cloud_status, get_cloud_status, CloudStatus};
use super::storage::{
    clear_token, get_or_create_environment_id, load_token, remove_secret, resolve_t3_home_dir,
    save_token, set_desired_cloud_link, write_secret, CLOUD_LINKED_USER_ID_SECRET,
    CLOUD_LINK_REVOKED_SECRET, PUBLISH_AGENT_ACTIVITY_SECRET,
};
use super::tunnel::resolve_cloudflared;

pub async fn connect_login(
    _headless: bool,
    home_dir: Option<&str>,
) -> Result<PersistedToken, CloudError> {
    let home = resolve_t3_home_dir(home_dir);
    let config = CloudConfig::from_env()?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    println!("T3 Connect\n");

    let auth_resp = request_device_authorization(&client, &config).await?;
    println!("{}", format_device_authorization_prompt(&auth_resp));

    let initial_interval = auth_resp.interval.unwrap_or(5);
    let token = poll_device_token(
        &client,
        &config,
        &auth_resp.device_code,
        initial_interval,
        auth_resp.expires_in,
    )
    .await?;

    save_token(&home, &token)?;

    let identity_str = token
        .identity
        .as_deref()
        .map(|id| format!(" as {}", id))
        .unwrap_or_default();
    println!("✓ Signed in{}", identity_str);

    Ok(token)
}

pub async fn connect_logout(home_dir: Option<&str>) -> Result<(), CloudError> {
    let home = resolve_t3_home_dir(home_dir);
    let _ = connect_unlink(home_dir).await;
    clear_token(&home)?;
    println!("Signed out of T3 Connect locally.");
    Ok(())
}

pub async fn connect_link(
    headless: bool,
    publish_only: bool,
    home_dir: Option<&str>,
) -> Result<(), CloudError> {
    let home = resolve_t3_home_dir(home_dir);

    println!("T3 Connect\n");

    // Check relay client if not publish_only
    if !publish_only {
        match resolve_cloudflared(&home).await {
            Ok(exe) => {
                println!("✓ Relay client ready · cloudflared {}", exe.version);
            }
            Err(_) => {
                println!("Notice: cloudflared relay client is not installed or not on PATH.");
            }
        }
    }

    // Ensure authorized
    let token = match load_token(&home)? {
        Some(t) => t,
        None => connect_login(headless, home_dir).await?,
    };

    let mode = if publish_only {
        "publish_only"
    } else {
        "managed"
    };
    set_desired_cloud_link(&home, true, mode)?;

    if publish_only {
        write_secret(&home, PUBLISH_AGENT_ACTIVITY_SECRET, b"true")?;
    }

    // Drop revoked marker if any
    let _ = remove_secret(&home, CLOUD_LINK_REVOKED_SECRET);

    let identity_str = token
        .identity
        .as_deref()
        .map(|id| format!(" as {}", id))
        .unwrap_or_default();

    println!("✓ Authorized{}", identity_str);
    if publish_only {
        println!("\nNext\n  Start T3 to publish agent activity (no managed tunnel).");
    } else {
        println!("\nNext\n  Start the server with `t1.73code up` to make this machine reachable.");
    }

    Ok(())
}

pub async fn connect_status(
    json: bool,
    home_dir: Option<&str>,
) -> Result<CloudStatus, CloudError> {
    let home = resolve_t3_home_dir(home_dir);
    let status = get_cloud_status(&home, true).await?;
    let formatted = format_cloud_status(&status, json);
    println!("{}", formatted);
    Ok(status)
}

pub async fn connect_unlink(home_dir: Option<&str>) -> Result<(), CloudError> {
    let home = resolve_t3_home_dir(home_dir);
    set_desired_cloud_link(&home, false, "managed")?;

    // Attempt to unlink from relay if token and environment_id exist
    if let Ok(Some(token)) = load_token(&home) {
        if let Ok(env_id) = get_or_create_environment_id(&home) {
            let config = CloudConfig::from_env()?;
            let url = format!("{}/v1/client/environment-links/{}", config.relay_url, env_id);
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap_or_default();

            let _ = client
                .delete(&url)
                .bearer_auth(&token.access_token)
                .send()
                .await;
        }
    }

    let _ = remove_secret(&home, CLOUD_LINKED_USER_ID_SECRET);
    let _ = remove_secret(&home, CLOUD_LINK_REVOKED_SECRET);

    println!("T3 Connect is disabled locally.");
    Ok(())
}
