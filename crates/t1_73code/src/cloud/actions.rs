use super::auth::{
    format_device_authorization_prompt, poll_device_token, request_device_authorization,
    PersistedToken,
};
use super::config::CloudConfig;
use super::error::CloudError;
use super::status::{format_cloud_status, get_cloud_status, CloudStatus};
use super::storage::{
    clear_token, drop_revoked_or_stale_link, get_or_create_environment_id, load_token,
    remove_secret, resolve_t3_home_dir, save_token, set_desired_cloud_link, write_secret,
    CLOUD_LINKED_USER_ID_SECRET, CLOUD_LINK_REVOKED_SECRET, PUBLISH_AGENT_ACTIVITY_SECRET,
};
use super::tunnel::resolve_cloudflared;
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

const ENVIRONMENT_SIGNING_KEY_SECRET: &str = "cloud-link-ed25519-key-pair";
fn b64url(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity((bytes.len() * 4 + 2) / 3);
    for chunk in bytes.chunks(3) {
        let n = ((chunk[0] as u32) << 16)
            | ((chunk.get(1).copied().unwrap_or(0) as u32) << 8)
            | chunk.get(2).copied().unwrap_or(0) as u32;
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((n >> 6) & 63) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(TABLE[(n & 63) as usize] as char);
        }
    }
    out
}

fn environment_signing_key(home: &std::path::Path) -> Result<(Ed25519KeyPair, String), CloudError> {
    if let Some(bytes) = super::storage::read_secret(home, ENVIRONMENT_SIGNING_KEY_SECRET)? {
        let value: Value = serde_json::from_slice(&bytes)?;
        let private_pem = value
            .get("privateKey")
            .and_then(Value::as_str)
            .ok_or_else(|| CloudError::ConfigError("Invalid environment signing key".into()))?;
        let private_der = super::config::base64_decode(
            &private_pem
                .lines()
                .filter(|line| !line.starts_with("---"))
                .collect::<String>(),
        )?;
        let pair = if private_der.len() == 48
            && private_der[..16]
                == [
                    0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04,
                    0x22, 0x04, 0x20,
                ] {
            Ed25519KeyPair::from_seed_unchecked(&private_der[16..])
        } else {
            Ed25519KeyPair::from_pkcs8(&private_der)
        }
        .map_err(|_| CloudError::ConfigError("Invalid environment signing key".into()))?;
        let public_pem = value
            .get("publicKey")
            .and_then(Value::as_str)
            .ok_or_else(|| CloudError::ConfigError("Invalid environment signing key".into()))?
            .to_string();
        return Ok((pair, public_pem));
    }
    let rng = ring::rand::SystemRandom::new();
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).map_err(|_| {
        CloudError::ConfigError("Unable to generate environment signing key".into())
    })?;
    let pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref())
        .map_err(|_| CloudError::ConfigError("Unable to load generated signing key".into()))?;
    let public_key = pair.public_key().as_ref();
    let public_der = [
        vec![
            0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
        ],
        public_key.to_vec(),
    ]
    .concat();
    let public_pem = format!(
        "-----BEGIN PUBLIC KEY-----\n{}\n-----END PUBLIC KEY-----",
        super::config::base64_encode(&public_der)
    );
    let private_der = pkcs8.as_ref();
    let mut private_pem_bytes = [
        0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04,
        0x20,
    ]
    .to_vec();
    private_pem_bytes.extend_from_slice(&private_der[16..]);
    let private_pem = format!(
        "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----",
        super::config::base64_encode(&private_pem_bytes)
    );
    super::storage::write_secret(
        home,
        ENVIRONMENT_SIGNING_KEY_SECRET,
        &serde_json::to_vec(&json!({"privateKey": private_pem, "publicKey": public_pem}))?,
    )?;
    Ok((pair, public_pem))
}

pub async fn provision_connect_link(
    home: &std::path::Path,
    token: &PersistedToken,
    mode: &str,
    host: &str,
    port: u16,
) -> Result<(), CloudError> {
    let config = CloudConfig::from_env()?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()?;
    let managed = mode != "publish_only";
    let challenge: Value = client.post(format!("{}/v1/client/environment-link-challenges", config.relay_url.trim_end_matches('/')))
        .bearer_auth(&token.access_token)
        .json(&json!({"notificationsEnabled":true,"liveActivitiesEnabled":true,"managedTunnelsEnabled":managed}))
        .send().await?.error_for_status()?.json().await?;
    let challenge_value = challenge
        .get("challenge")
        .and_then(Value::as_str)
        .ok_or_else(|| CloudError::RelayError("Challenge response missing challenge".into()))?;
    let env_id = get_or_create_environment_id(home)?;
    let (key_pair, public_key) = environment_signing_key(home)?;
    let hostname = std::env::var("HOSTNAME").unwrap_or_else(|_| "localhost".into());
    let platform_os = match std::env::consts::OS {
        "macos" => "darwin",
        "linux" => "linux",
        "windows" => "windows",
        _ => "unknown",
    };
    let platform_arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        _ => "other",
    };
    let descriptor = json!({"environmentId":env_id,"label":hostname,"platform":{"os":platform_os,"arch":platform_arch,"machine":"server"},"serverVersion":"0.1.74","orchestrationProtocolVersion":1,"capabilities":{"repositoryIdentity":false}});
    let local_host = if host == "0.0.0.0" || host == "::" {
        "127.0.0.1"
    } else {
        host
    };
    let origin = json!({"localHttpHost":"127.0.0.1","localHttpPort":port});
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let payload = json!({"iss":format!("t3-env:{env_id}"),"aud":config.relay_url.trim_end_matches('/'),"sub":env_id,"jti":uuid::Uuid::new_v4().to_string(),"iat":now,"exp":now+300,"challenge":challenge_value,"descriptor":descriptor,"environmentId":env_id,"environmentPublicKey":public_key.trim(),"endpoint":{"httpBaseUrl":format!("http://{local_host}:{port}/"),"wsBaseUrl":format!("ws://{local_host}:{port}/ws"),"providerKind":if managed {"cloudflare_tunnel"} else {"manual"}},"origin":origin,"scopes":if managed {vec!["agent_activity_notifications","managed_tunnels"]} else {vec!["agent_activity_notifications"]}});
    let header = b64url(br#"{"alg":"EdDSA","typ":"t3-env-link+jwt"}"#);
    let body = b64url(&serde_json::to_vec(&payload)?);
    let signing_input = format!("{header}.{body}");
    let signature = key_pair.sign(signing_input.as_bytes());
    let proof = format!("{signing_input}.{}", b64url(signature.as_ref()));
    let response = client.post(format!("{}/v1/client/environment-links", config.relay_url.trim_end_matches('/')))
        .bearer_auth(&token.access_token)
        .json(&json!({"proof":proof,"notificationsEnabled":true,"liveActivitiesEnabled":true,"managedTunnelsEnabled":managed}))
        .send().await?;
    let status = response.status();
    let response_body = response.text().await?;
    if !status.is_success() {
        return Err(CloudError::RelayError(format!(
            "HTTP {status}: {response_body}"
        )));
    }
    let result: Value = serde_json::from_str(&response_body)?;
    if result.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(CloudError::RelayError(
            "Relay did not confirm environment link".into(),
        ));
    }
    for (key, secret) in [
        ("cloudUserId", CLOUD_LINKED_USER_ID_SECRET),
        (
            "environmentCredential",
            super::storage::RELAY_ENVIRONMENT_CREDENTIAL_SECRET,
        ),
        (
            "cloudMintPublicKey",
            super::storage::CLOUD_MINT_PUBLIC_KEY_SECRET,
        ),
    ] {
        let value = result
            .get(key)
            .and_then(Value::as_str)
            .ok_or_else(|| CloudError::RelayError(format!("Relay response missing {key}")))?;
        write_secret(home, secret, value.as_bytes())?;
    }
    if let Some(runtime) = result.get("endpointRuntime").filter(|v| !v.is_null()) {
        write_secret(
            home,
            super::storage::CLOUD_ENDPOINT_RUNTIME_CONFIG_SECRET,
            &serde_json::to_vec(runtime)?,
        )?;
    }
    if let Some(issuer) = result.get("relayIssuer").and_then(Value::as_str) {
        write_secret(home, super::storage::RELAY_URL_SECRET, issuer.as_bytes())?;
    }
    Ok(())
}

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

    // Drop revoked or stale credentials when re-establishing link
    if drop_revoked_or_stale_link(&home)? {
        println!("Replacing the link T3 Connect revoked.");
    }

    provision_connect_link(&home, &token, mode, "localhost", 3000).await?;

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

pub async fn connect_status(json: bool, home_dir: Option<&str>) -> Result<CloudStatus, CloudError> {
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
            let url = format!(
                "{}/v1/client/environment-links/{}",
                config.relay_url, env_id
            );
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
