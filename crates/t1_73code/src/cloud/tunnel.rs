use super::error::CloudError;
use serde::{Deserialize, Serialize};
use std::env;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::process::{Child, Command};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CloudflaredSource {
    Override,
    Managed,
    Path,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudflaredExecutable {
    pub path: PathBuf,
    pub version: String,
    pub source: CloudflaredSource,
}

pub fn cloudflared_filename() -> &'static str {
    if cfg!(target_os = "windows") {
        "cloudflared.exe"
    } else {
        "cloudflared"
    }
}

pub fn parse_cloudflared_version(output: &str) -> Option<String> {
    for word in output.split_whitespace() {
        let parts: Vec<&str> = word.split('.').collect();
        if parts.len() >= 3 {
            if let (Ok(year), Ok(month), Ok(patch)) = (
                parts[0].parse::<u32>(),
                parts[1].parse::<u32>(),
                parts[2].parse::<u32>(),
            ) {
                if year >= 2020 && month <= 12 {
                    return Some(format!("{}.{}.{}", year, month, patch));
                }
            }
        }
    }
    None
}

pub async fn probe_cloudflared_version(path: &Path) -> Result<String, CloudError> {
    let output = match tokio::time::timeout(
        Duration::from_secs(5),
        Command::new(path).arg("--version").output(),
    )
    .await
    {
        Ok(Ok(out)) => out,
        Ok(Err(err)) => {
            return Err(CloudError::CloudflaredFailed(format!(
                "Failed to probe {}: {}",
                path.display(),
                err
            )))
        }
        Err(_) => {
            return Err(CloudError::CloudflaredFailed(format!(
                "Probe {} timed out",
                path.display()
            )))
        }
    };

    let text = String::from_utf8_lossy(&output.stdout);
    if let Some(v) = parse_cloudflared_version(&text) {
        return Ok(v);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if let Some(v) = parse_cloudflared_version(&stderr) {
        return Ok(v);
    }

    Ok("unknown".to_string())
}

pub fn find_cloudflared_in_dir(dir: &Path) -> Option<PathBuf> {
    if !dir.exists() || !dir.is_dir() {
        return None;
    }

    let target_name = cloudflared_filename();

    // Check direct child first: <dir>/cloudflared
    let direct = dir.join(target_name);
    if direct.is_file() {
        return Some(direct);
    }

    // Search subdirectories recursively (e.g. <dir>/<version>/<platform>-<arch>/cloudflared)
    let mut dirs_to_visit = vec![(dir.to_path_buf(), 0usize)];
    let mut candidates = Vec::new();

    while let Some((current_dir, depth)) = dirs_to_visit.pop() {
        if depth > 4 {
            continue;
        }

        if let Ok(entries) = std::fs::read_dir(&current_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some(file_name) = path.file_name() {
                        if file_name == target_name {
                            candidates.push(path);
                        }
                    }
                } else if path.is_dir() {
                    dirs_to_visit.push((path, depth + 1));
                }
            }
        }
    }

    // Sort descending so newer versions/paths are tried first
    candidates.sort_by(|a, b| b.cmp(a));
    candidates.into_iter().next()
}

pub async fn resolve_cloudflared(home_dir: &Path) -> Result<CloudflaredExecutable, CloudError> {
    // 1. T3CODE_CLOUDFLARED_PATH then CLOUDFLARED_PATH override
    let override_var = env::var("T3CODE_CLOUDFLARED_PATH")
        .or_else(|_| env::var("CLOUDFLARED_PATH"));

    if let Ok(override_path) = override_var {
        let p = PathBuf::from(override_path.trim());
        if p.exists() {
            let version = probe_cloudflared_version(&p).await.unwrap_or_else(|_| "unknown".to_string());
            return Ok(CloudflaredExecutable {
                path: p,
                version,
                source: CloudflaredSource::Override,
            });
        }
    }

    // 2. Managed path: <home_dir>/tools/cloudflared and subdirectories
    let managed_root = home_dir.join("tools").join("cloudflared");
    if let Some(managed_bin) = find_cloudflared_in_dir(&managed_root) {
        let version = probe_cloudflared_version(&managed_bin).await.unwrap_or_else(|_| "unknown".to_string());
        return Ok(CloudflaredExecutable {
            path: managed_bin,
            version,
            source: CloudflaredSource::Managed,
        });
    }

    // 3. Search PATH
    if let Ok(path_var) = env::var("PATH") {
        let delimiter = if cfg!(target_os = "windows") { ';' } else { ':' };
        for dir in path_var.split(delimiter) {
            let candidate = Path::new(dir.trim()).join(cloudflared_filename());
            if candidate.is_file() {
                let version = probe_cloudflared_version(&candidate).await.unwrap_or_else(|_| "unknown".to_string());
                return Ok(CloudflaredExecutable {
                    path: candidate,
                    version,
                    source: CloudflaredSource::Path,
                });
            }
        }
    }

    Err(CloudError::CloudflaredNotFound(
        "cloudflared binary not found on PATH or in managed directory".to_string(),
    ))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelConfig {
    pub tunnel_id: Option<String>,
    pub tunnel_name: Option<String>,
    pub connector_token: String,
    pub local_port: u16,
    pub local_host: String,
    pub protocol: Option<String>,
    pub metrics_address: String,
}

impl TunnelConfig {
    pub fn new(connector_token: &str, local_port: u16) -> Self {
        TunnelConfig {
            tunnel_id: None,
            tunnel_name: None,
            connector_token: connector_token.to_string(),
            local_port,
            local_host: "127.0.0.1".to_string(),
            protocol: None,
            metrics_address: "127.0.0.1:0".to_string(),
        }
    }

    pub fn build_args(&self) -> Vec<String> {
        let mut args = vec![
            "tunnel".to_string(),
            "--no-autoupdate".to_string(),
            "--loglevel".to_string(),
            "info".to_string(),
            "--output".to_string(),
            "default".to_string(),
            "--metrics".to_string(),
            self.metrics_address.clone(),
        ];

        if let Some(ref proto) = self.protocol {
            args.push("--protocol".to_string());
            args.push(proto.clone());
        }

        args.push("run".to_string());
        args
    }

    pub fn generate_ingress_yaml(&self, hostname: &str) -> String {
        format!(
            "tunnel: {}\ningress:\n  - hostname: {}\n    service: http://{}:{}\n  - service: http_status:404\nmetrics: {}\n",
            self.tunnel_id.as_deref().unwrap_or("t3-tunnel"),
            hostname,
            self.local_host,
            self.local_port,
            self.metrics_address
        )
    }
}

pub fn spawn_tunnel_process(
    executable: &Path,
    config: &TunnelConfig,
) -> Result<Child, CloudError> {
    let args = config.build_args();

    let child = Command::new(executable)
        .args(&args)
        .env("TUNNEL_TOKEN", &config.connector_token)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            CloudError::CloudflaredFailed(format!(
                "Failed to spawn cloudflared at {}: {}",
                executable.display(),
                e
            ))
        })?;

    Ok(child)
}

pub struct TunnelProcessManager {
    child: Mutex<Option<Child>>,
    pid: Option<u32>,
    running: Arc<AtomicBool>,
}

impl TunnelProcessManager {
    pub fn spawn(executable: &Path, config: &TunnelConfig) -> Result<Self, CloudError> {
        let child = spawn_tunnel_process(executable, config)?;
        let pid = child.id();
        Ok(TunnelProcessManager {
            child: Mutex::new(Some(child)),
            pid,
            running: Arc::new(AtomicBool::new(true)),
        })
    }

    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    pub fn is_alive(&self) -> bool {
        if !self.running.load(Ordering::SeqCst) {
            return false;
        }

        if let Ok(mut guard) = self.child.lock() {
            if let Some(ref mut child) = *guard {
                match child.try_wait() {
                    Ok(None) => true,
                    Ok(Some(_status)) => {
                        self.running.store(false, Ordering::SeqCst);
                        false
                    }
                    Err(_) => {
                        self.running.store(false, Ordering::SeqCst);
                        false
                    }
                }
            } else {
                false
            }
        } else {
            self.running.load(Ordering::SeqCst)
        }
    }

    pub async fn stop(&mut self) -> Result<(), CloudError> {
        self.running.store(false, Ordering::SeqCst);
        let maybe_child = if let Ok(mut guard) = self.child.lock() {
            guard.take()
        } else {
            None
        };
        if let Some(mut child) = maybe_child {
            let _ = child.kill().await;
        }
        Ok(())
    }
}

impl Drop for TunnelProcessManager {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.child.lock() {
            if let Some(mut child) = guard.take() {
                let _ = child.start_kill();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cloudflared_version() {
        let output = "cloudflared version 2026.10.4 (built 2026-10-14-1234 UTC)";
        let version = parse_cloudflared_version(output);
        assert_eq!(version, Some("2026.10.4".to_string()));

        let output2 = "Version 2024.8.3";
        let version2 = parse_cloudflared_version(output2);
        assert_eq!(version2, Some("2024.8.3".to_string()));

        let output3 = "invalid version output";
        assert_eq!(parse_cloudflared_version(output3), None);
    }

    #[test]
    fn test_tunnel_config_build_args() {
        let mut config = TunnelConfig::new("ey_token123", 3000);
        let args = config.build_args();
        assert_eq!(
            args,
            vec![
                "tunnel",
                "--no-autoupdate",
                "--loglevel",
                "info",
                "--output",
                "default",
                "--metrics",
                "127.0.0.1:0",
                "run"
            ]
        );

        config.protocol = Some("http2".to_string());
        let args2 = config.build_args();
        assert!(args2.contains(&"--protocol".to_string()));
        assert!(args2.contains(&"http2".to_string()));
    }

    #[test]
    fn test_generate_ingress_yaml() {
        let mut config = TunnelConfig::new("tok", 3773);
        config.tunnel_id = Some("tun_abc".to_string());
        let yaml = config.generate_ingress_yaml("app.t3.codes");
        assert!(yaml.contains("tunnel: tun_abc"));
        assert!(yaml.contains("hostname: app.t3.codes"));
        assert!(yaml.contains("service: http://127.0.0.1:3773"));
        assert!(yaml.contains("service: http_status:404"));
    }

    #[test]
    fn test_find_cloudflared_in_dir() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        assert_eq!(find_cloudflared_in_dir(root), None);

        // Nested directory: <root>/2026.10.4/linux-amd64/cloudflared
        let nested = root.join("2026.10.4").join("linux-amd64");
        std::fs::create_dir_all(&nested).unwrap();
        let binary_path = nested.join(cloudflared_filename());
        std::fs::write(&binary_path, b"dummy binary").unwrap();

        let found = find_cloudflared_in_dir(root);
        assert_eq!(found, Some(binary_path));
    }
}
