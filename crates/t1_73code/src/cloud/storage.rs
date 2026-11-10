use super::auth::PersistedToken;
use super::error::CloudError;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub const CLOUD_CLI_OAUTH_TOKEN_SECRET: &str = "cloud-cli-oauth-token";
pub const CLOUD_CLI_DESIRED_LINK_SECRET: &str = "cloud-cli-desired-link";
pub const CLOUD_LINKED_USER_ID_SECRET: &str = "cloud-linked-user-id";
pub const RELAY_URL_SECRET: &str = "relay-url";
pub const PUBLISH_AGENT_ACTIVITY_SECRET: &str = "publish-agent-activity";
pub const CLOUD_LINK_REVOKED_SECRET: &str = "cloud-link-revoked";
pub const ENVIRONMENT_ID_SECRET: &str = "environment-id";

pub fn resolve_t3_home_dir(custom_home_dir: Option<&str>) -> PathBuf {
    if let Some(custom) = custom_home_dir {
        if !custom.trim().is_empty() {
            return PathBuf::from(custom.trim());
        }
    }

    if let Ok(env_home) = env::var("T3CODE_HOME") {
        if !env_home.trim().is_empty() {
            return PathBuf::from(env_home.trim());
        }
    }

    if let Ok(home) = env::var("HOME") {
        PathBuf::from(home).join(".t3")
    } else {
        PathBuf::from(".t3")
    }
}

pub fn resolve_secrets_dir(home_dir: &Path) -> PathBuf {
    home_dir.join("userdata").join("secrets")
}

pub fn resolve_sqlite_path(home_dir: &Path) -> PathBuf {
    home_dir.join("userdata").join("statev2.sqlite")
}

pub fn write_secret(home_dir: &Path, name: &str, data: &[u8]) -> Result<(), CloudError> {
    let secrets_dir = resolve_secrets_dir(home_dir);
    fs::create_dir_all(&secrets_dir)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&secrets_dir, fs::Permissions::from_mode(0o700));
    }

    let file_path = secrets_dir.join(format!("{}.bin", name));
    fs::write(&file_path, data)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&file_path, fs::Permissions::from_mode(0o600));
    }

    Ok(())
}

pub fn read_secret(home_dir: &Path, name: &str) -> Result<Option<Vec<u8>>, CloudError> {
    let file_path = resolve_secrets_dir(home_dir).join(format!("{}.bin", name));
    if !file_path.exists() {
        return Ok(None);
    }

    let data = fs::read(file_path)?;
    Ok(Some(data))
}

pub fn remove_secret(home_dir: &Path, name: &str) -> Result<(), CloudError> {
    let file_path = resolve_secrets_dir(home_dir).join(format!("{}.bin", name));
    if file_path.exists() {
        fs::remove_file(file_path)?;
    }
    Ok(())
}

pub fn save_token(home_dir: &Path, token: &PersistedToken) -> Result<(), CloudError> {
    let json = serde_json::to_vec(token)?;
    write_secret(home_dir, CLOUD_CLI_OAUTH_TOKEN_SECRET, &json)?;

    // Also persist in SQLite if database exists or can be accessed
    let _ = save_token_sqlite(home_dir, token);
    Ok(())
}

pub fn load_token(home_dir: &Path) -> Result<Option<PersistedToken>, CloudError> {
    if let Some(bytes) = read_secret(home_dir, CLOUD_CLI_OAUTH_TOKEN_SECRET)? {
        let token: PersistedToken = serde_json::from_slice(&bytes)?;
        return Ok(Some(token));
    }

    // Fallback to SQLite
    load_token_sqlite(home_dir)
}

pub fn clear_token(home_dir: &Path) -> Result<(), CloudError> {
    remove_secret(home_dir, CLOUD_CLI_OAUTH_TOKEN_SECRET)?;
    let _ = clear_token_sqlite(home_dir);
    Ok(())
}

pub fn set_desired_cloud_link(
    home_dir: &Path,
    desired: bool,
    mode: &str,
) -> Result<(), CloudError> {
    if desired {
        write_secret(home_dir, CLOUD_CLI_DESIRED_LINK_SECRET, mode.as_bytes())?;
    } else {
        remove_secret(home_dir, CLOUD_CLI_DESIRED_LINK_SECRET)?;
    }
    Ok(())
}

pub fn get_desired_cloud_link(home_dir: &Path) -> Result<bool, CloudError> {
    let secret = read_secret(home_dir, CLOUD_CLI_DESIRED_LINK_SECRET)?;
    Ok(secret.is_some())
}

pub fn get_desired_link_mode(home_dir: &Path) -> Result<String, CloudError> {
    match read_secret(home_dir, CLOUD_CLI_DESIRED_LINK_SECRET)? {
        Some(bytes) => {
            let s = String::from_utf8_lossy(&bytes).trim().to_string();
            if s == "publish_only" {
                Ok("publish_only".to_string())
            } else {
                Ok("managed".to_string())
            }
        }
        None => Ok("managed".to_string()),
    }
}

pub fn get_or_create_environment_id(home_dir: &Path) -> Result<String, CloudError> {
    if let Some(bytes) = read_secret(home_dir, ENVIRONMENT_ID_SECRET)? {
        let id = String::from_utf8_lossy(&bytes).trim().to_string();
        if !id.is_empty() {
            return Ok(id);
        }
    }

    let new_id = uuid::Uuid::new_v4().to_string();
    write_secret(home_dir, ENVIRONMENT_ID_SECRET, new_id.as_bytes())?;
    Ok(new_id)
}

// SQLite implementations
pub fn save_token_sqlite(home_dir: &Path, token: &PersistedToken) -> Result<(), CloudError> {
    let sqlite_path = resolve_sqlite_path(home_dir);
    if let Some(parent) = sqlite_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let conn = rusqlite::Connection::open(&sqlite_path)
        .map_err(|e| CloudError::StorageError(format!("SQLite open error: {}", e)))?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS secrets (key TEXT PRIMARY KEY, value BLOB)",
        [],
    )
    .map_err(|e| CloudError::StorageError(format!("SQLite create table error: {}", e)))?;

    let json = serde_json::to_vec(token)?;
    conn.execute(
        "INSERT INTO secrets (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2",
        rusqlite::params![CLOUD_CLI_OAUTH_TOKEN_SECRET, json],
    )
    .map_err(|e| CloudError::StorageError(format!("SQLite insert error: {}", e)))?;

    Ok(())
}

pub fn load_token_sqlite(home_dir: &Path) -> Result<Option<PersistedToken>, CloudError> {
    let sqlite_path = resolve_sqlite_path(home_dir);
    if !sqlite_path.exists() {
        return Ok(None);
    }

    let conn = rusqlite::Connection::open(&sqlite_path)
        .map_err(|e| CloudError::StorageError(format!("SQLite open error: {}", e)))?;

    let mut stmt = match conn.prepare("SELECT value FROM secrets WHERE key = ?1") {
        Ok(s) => s,
        Err(_) => return Ok(None),
    };

    let mut rows = stmt
        .query(rusqlite::params![CLOUD_CLI_OAUTH_TOKEN_SECRET])
        .map_err(|e| CloudError::StorageError(format!("SQLite query error: {}", e)))?;

    if let Some(row) = rows
        .next()
        .map_err(|e| CloudError::StorageError(format!("SQLite row error: {}", e)))?
    {
        let blob: Vec<u8> = row
            .get(0)
            .map_err(|e| CloudError::StorageError(format!("SQLite get blob error: {}", e)))?;
        let token: PersistedToken = serde_json::from_slice(&blob)?;
        Ok(Some(token))
    } else {
        Ok(None)
    }
}

pub fn clear_token_sqlite(home_dir: &Path) -> Result<(), CloudError> {
    let sqlite_path = resolve_sqlite_path(home_dir);
    if !sqlite_path.exists() {
        return Ok(());
    }

    if let Ok(conn) = rusqlite::Connection::open(&sqlite_path) {
        let _ = conn.execute(
            "DELETE FROM secrets WHERE key = ?1",
            rusqlite::params![CLOUD_CLI_OAUTH_TOKEN_SECRET],
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_secret_file_persistence() {
        let dir = tempdir().unwrap();
        let home = dir.path();

        assert_eq!(read_secret(home, "test_secret").unwrap(), None);

        write_secret(home, "test_secret", b"hello world").unwrap();
        assert_eq!(
            read_secret(home, "test_secret").unwrap(),
            Some(b"hello world".to_vec())
        );

        remove_secret(home, "test_secret").unwrap();
        assert_eq!(read_secret(home, "test_secret").unwrap(), None);
    }

    #[test]
    fn test_token_save_and_load() {
        let dir = tempdir().unwrap();
        let home = dir.path();

        let token = PersistedToken {
            access_token: "acc_123".to_string(),
            refresh_token: "ref_123".to_string(),
            expires_at_epoch_ms: 1700000000000,
            identity: Some("test@t3.codes".to_string()),
        };

        save_token(home, &token).unwrap();
        let loaded = load_token(home).unwrap().expect("token should be loaded");
        assert_eq!(loaded, token);

        clear_token(home).unwrap();
        assert_eq!(load_token(home).unwrap(), None);
    }

    #[test]
    fn test_sqlite_token_persistence() {
        let dir = tempdir().unwrap();
        let home = dir.path();

        let token = PersistedToken {
            access_token: "acc_sql".to_string(),
            refresh_token: "ref_sql".to_string(),
            expires_at_epoch_ms: 1800000000000,
            identity: Some("sqlite@t3.codes".to_string()),
        };

        save_token_sqlite(home, &token).unwrap();
        let loaded = load_token_sqlite(home).unwrap().expect("loaded from sqlite");
        assert_eq!(loaded, token);

        clear_token_sqlite(home).unwrap();
        assert_eq!(load_token_sqlite(home).unwrap(), None);
    }

    #[test]
    fn test_desired_link_state() {
        let dir = tempdir().unwrap();
        let home = dir.path();

        assert!(!get_desired_cloud_link(home).unwrap());
        assert_eq!(get_desired_link_mode(home).unwrap(), "managed");

        set_desired_cloud_link(home, true, "publish_only").unwrap();
        assert!(get_desired_cloud_link(home).unwrap());
        assert_eq!(get_desired_link_mode(home).unwrap(), "publish_only");

        set_desired_cloud_link(home, false, "managed").unwrap();
        assert!(!get_desired_cloud_link(home).unwrap());
    }

    #[test]
    fn test_environment_id() {
        let dir = tempdir().unwrap();
        let home = dir.path();

        let id1 = get_or_create_environment_id(home).unwrap();
        assert!(!id1.is_empty());
        let id2 = get_or_create_environment_id(home).unwrap();
        assert_eq!(id1, id2);
    }
}
