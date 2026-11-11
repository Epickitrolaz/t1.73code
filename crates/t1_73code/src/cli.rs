use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Parser, Debug, Clone)]
#[command(
    version = "0.1.73",
    about = "Ultra low-memory T3 Code CLI & server in Rust supporting the Pi agent"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[arg(short, long, default_value = "3000", help = "Port to listen on")]
    pub port: u16,

    #[arg(long, default_value = "0.0.0.0", help = "Host to listen on")]
    pub host: String,

    #[arg(long, help = "Share server over Tailscale")]
    pub share: bool,

    #[arg(long, help = "Connect server via T3 Connect tunnel")]
    pub connect: bool,

    #[arg(long, global = true, help = "Custom T3 home directory")]
    pub home_dir: Option<String>,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    #[command(about = "Start the t1.73code server")]
    Up {
        #[arg(short, long, default_value = "3000")]
        port: u16,
        #[arg(long, default_value = "0.0.0.0")]
        host: String,
        #[arg(long)]
        share: bool,
        #[arg(long)]
        connect: bool,
        #[arg(long)]
        home_dir: Option<String>,
    },
    #[command(about = "Start the t1.73code server")]
    Serve {
        #[arg(short, long, default_value = "3000")]
        port: u16,
        #[arg(long, default_value = "0.0.0.0")]
        host: String,
        #[arg(long)]
        share: bool,
        #[arg(long)]
        connect: bool,
        #[arg(long)]
        home_dir: Option<String>,
    },
    #[command(about = "Show pairing link and token")]
    Pair {
        #[arg(long)]
        home_dir: Option<String>,
        #[arg(short, long, default_value = "3000")]
        port: u16,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
    },
    #[command(about = "Check prerequisites and system health")]
    Doctor {
        #[arg(long)]
        home_dir: Option<String>,
    },
    #[command(about = "Manage T3 Connect")]
    Connect {
        #[command(subcommand)]
        action: ConnectAction,
    },
    #[command(about = "Install the server to start at boot (Linux systemd user service)")]
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum ServiceAction {
    Install,
    Uninstall,
    Status,
}

#[derive(Subcommand, Debug, Clone)]
pub enum ConnectAction {
    #[command(about = "Link environment to T3 Connect")]
    Link {
        #[arg(long)]
        headless: bool,
        #[arg(long)]
        publish_only: bool,
    },
    #[command(about = "Show T3 Connect status")]
    Status {
        #[arg(long)]
        json: bool,
    },
    #[command(about = "Unlink environment from T3 Connect")]
    Unlink,
    #[command(about = "Sign in to T3 Connect")]
    Login {
        #[arg(long)]
        headless: bool,
    },
    #[command(about = "Sign out of T3 Connect")]
    Logout,
}

pub fn resolve_t3_home_dir(custom_home_dir: Option<&str>) -> PathBuf {
    crate::cloud::storage::resolve_t3_home_dir(custom_home_dir)
}

pub fn resolve_connector_token(
    home_dir: &Path,
) -> Option<(String, Option<String>, Option<String>)> {
    // 1. Check CLOUD_ENDPOINT_RUNTIME_CONFIG_SECRET
    if let Ok(Some(bytes)) =
        crate::cloud::read_secret(home_dir, crate::cloud::CLOUD_ENDPOINT_RUNTIME_CONFIG_SECRET)
    {
        if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&bytes) {
            let token = val
                .get("connectorToken")
                .or_else(|| val.get("connector_token"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let tunnel_id = val
                .get("tunnelId")
                .or_else(|| val.get("tunnel_id"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let tunnel_name = val
                .get("tunnelName")
                .or_else(|| val.get("tunnel_name"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            if let Some(tok) = token {
                return Some((tok, tunnel_id, tunnel_name));
            }
        }
    }

    // 2. Fall back to RELAY_ENVIRONMENT_CREDENTIAL_SECRET
    if let Ok(Some(bytes)) =
        crate::cloud::read_secret(home_dir, crate::cloud::RELAY_ENVIRONMENT_CREDENTIAL_SECRET)
    {
        let text = String::from_utf8_lossy(&bytes).trim().to_string();
        if !text.is_empty() {
            return Some((text, None, None));
        }
    }

    None
}

pub fn generate_startup_pairing(
    storage: &crate::storage::SqliteStorage,
    host: &str,
    port: u16,
) -> Result<(String, String), Box<dyn std::error::Error>> {
    let token = format!("pair_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
    let expires_at = chrono::Utc::now() + chrono::Duration::hours(24);
    let scopes = [
        "orchestration:read",
        "orchestration:operate",
        "access:write",
        "relay:read",
        "relay:write",
    ];

    storage.create_pairing_link(&token, &scopes, expires_at)?;

    let url_host = if host == "0.0.0.0" || host == "::" {
        "127.0.0.1"
    } else {
        host
    };

    let pairing_url = crate::tailscale::generate_pairing_url(url_host, Some(port), &token, false);
    Ok((token, pairing_url))
}

pub async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(_) => {
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

pub async fn run_server(
    port: u16,
    host: String,
    share: bool,
    connect: bool,
    home_dir_opt: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let home_dir = resolve_t3_home_dir(home_dir_opt.as_deref());
    std::fs::create_dir_all(&home_dir)?;
    std::fs::create_dir_all(home_dir.join("userdata"))?;

    let storage = Arc::new(crate::storage::SqliteStorage::new_in_dir(&home_dir)?);
    let app_state = Arc::new(crate::state::AppState::new(
        home_dir.clone(),
        storage.clone(),
    ));

    // Generate administrative one-time pairing credential
    let (token, mut pairing_url) = generate_startup_pairing(&storage, &host, port)?;

    // Bind TCP listener
    let bind_addr = format!("{}:{}", host, port);
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    let local_addr = listener.local_addr()?;
    let bound_port = local_addr.port();

    if bound_port != port {
        let display_host = if host == "0.0.0.0" || host == "::" {
            "127.0.0.1"
        } else {
            &host
        };
        pairing_url =
            crate::tailscale::generate_pairing_url(display_host, Some(bound_port), &token, false);
    }

    // Share server over Tailscale if requested
    let mut tailscale_pairing_url: Option<String> = None;
    if share {
        match crate::tailscale::tailscale_serve(bound_port).await {
            Ok(()) => {
                tracing::info!("Tailscale serve started on port {}", bound_port);
            }
            Err(e) => {
                eprintln!("Warning: Failed to start Tailscale serve: {}", e);
            }
        }

        match crate::tailscale::get_tailscale_status().await {
            Ok(status) => {
                let ts_url = if status.https_enabled.unwrap_or(false) {
                    status.pairing_url(None, &token)
                } else {
                    status.pairing_url(Some(bound_port), &token)
                };
                if let Some(url) = ts_url {
                    tailscale_pairing_url = Some(url);
                }
            }
            Err(e) => {
                eprintln!("Warning: Failed to get Tailscale status: {}", e);
            }
        }
    }

    // Connect server via T3 Connect tunnel if requested
    let mut tunnel_manager: Option<crate::cloud::TunnelProcessManager> = None;
    if connect {
        match crate::cloud::get_cloud_status(&home_dir, false).await {
            Ok(mut cloud_status) => {
                if cloud_status.desired && !cloud_status.linked {
                    if let Some(token) = crate::cloud::load_token(&home_dir)? {
                        let mode = crate::cloud::get_desired_link_mode(&home_dir)?;
                        if let Err(error) = crate::cloud::actions::provision_connect_link(
                            &home_dir, &token, &mode, &host, bound_port,
                        )
                        .await
                        {
                            eprintln!("Warning: failed to register this environment with T3 Connect: {error}");
                        } else if let Ok(updated) =
                            crate::cloud::get_cloud_status(&home_dir, false).await
                        {
                            cloud_status = updated;
                        }
                    }
                }
                if cloud_status.linked {
                    match crate::cloud::resolve_cloudflared(&home_dir).await {
                        Ok(exe) => {
                            if let Some((connector_token, tunnel_id, tunnel_name)) =
                                resolve_connector_token(&home_dir)
                            {
                                let mut tunnel_config =
                                    crate::cloud::TunnelConfig::new(&connector_token, bound_port);
                                tunnel_config.tunnel_id = tunnel_id;
                                tunnel_config.tunnel_name = tunnel_name;

                                match crate::cloud::TunnelProcessManager::spawn(
                                    &exe.path,
                                    &tunnel_config,
                                ) {
                                    Ok(mgr) => {
                                        println!(
                                            "✓ T3 Connect tunnel launched (PID: {:?})",
                                            mgr.pid()
                                        );
                                        tunnel_manager = Some(mgr);
                                    }
                                    Err(e) => {
                                        eprintln!(
                                            "Warning: Failed to launch T3 Connect tunnel: {}",
                                            e
                                        );
                                    }
                                }
                            } else {
                                println!(
                                    "Notice: T3 Connect is linked but no connector token was found."
                                );
                            }
                        }
                        Err(e) => {
                            eprintln!("Warning: cloudflared relay client is not available: {}", e);
                        }
                    }
                } else {
                    println!(
                        "Notice: T3 Connect is not linked. Run `t1.73code connect link` first."
                    );
                }
            }
            Err(e) => {
                eprintln!("Warning: Failed to inspect cloud link status: {}", e);
            }
        }
    }

    // Build router
    let router = crate::http::build_router(app_state);

    // Print server banner and pairing links
    let display_host = if host == "0.0.0.0" || host == "::" {
        "127.0.0.1"
    } else {
        &host
    };
    println!("t1.73code v0.1.73");
    println!("Server listening on http://{}:{}", display_host, bound_port);
    println!("  pairingUrl: {}", pairing_url);
    if let Some(ref ts_url) = tailscale_pairing_url {
        println!("  tailscalePairingUrl: {}", ts_url);
    }

    // Serve Axum app with graceful shutdown signal
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    if let Some(mut mgr) = tunnel_manager {
        println!("Stopping T3 Connect tunnel...");
        let _ = mgr.stop().await;
    }

    Ok(())
}

// =========================================================================
// Doctor Diagnostic System
// =========================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Debug, Clone)]
pub struct DiagnosticCheck {
    pub name: String,
    pub status: CheckStatus,
    pub message: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DoctorReport {
    pub checks: Vec<DiagnosticCheck>,
}

impl DoctorReport {
    pub fn all_ok(&self) -> bool {
        !self.checks.iter().any(|c| c.status == CheckStatus::Fail)
    }

    pub fn format(&self) -> String {
        let mut out = String::new();
        out.push_str("============================================================\n");
        out.push_str("              t1.73code Doctor Diagnostic Report            \n");
        out.push_str("============================================================\n");

        for check in &self.checks {
            let status_tag = match check.status {
                CheckStatus::Pass => "[PASS]",
                CheckStatus::Warn => "[WARN]",
                CheckStatus::Fail => "[FAIL]",
            };
            out.push_str(&format!(
                "  {:<6} {:<22} {}\n",
                status_tag, check.name, check.message
            ));
            if let Some(ref detail) = check.detail {
                out.push_str(&format!("         -> {}\n", detail));
            }
        }

        out.push_str("============================================================\n");

        let passes = self
            .checks
            .iter()
            .filter(|c| c.status == CheckStatus::Pass)
            .count();
        let warns = self
            .checks
            .iter()
            .filter(|c| c.status == CheckStatus::Warn)
            .count();
        let fails = self
            .checks
            .iter()
            .filter(|c| c.status == CheckStatus::Fail)
            .count();

        out.push_str(&format!(
            "Summary: {} passed, {} warning(s), {} failed\n",
            passes, warns, fails
        ));

        if fails > 0 {
            out.push_str("Result: Action required to resolve failing checks.\n");
        } else if warns > 0 {
            out.push_str("Result: All essential systems operational (with warnings).\n");
        } else {
            out.push_str("Result: All systems operational. Ready to run t1.73code.\n");
        }

        out
    }
}

pub async fn check_pi_cli() -> DiagnosticCheck {
    let timeout = std::time::Duration::from_secs(3);
    match tokio::time::timeout(
        timeout,
        tokio::process::Command::new("pi").arg("--version").output(),
    )
    .await
    {
        Ok(Ok(output)) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let version = if stdout.is_empty() {
                String::from_utf8_lossy(&output.stderr).trim().to_string()
            } else {
                stdout
            };
            DiagnosticCheck {
                name: "Pi CLI".to_string(),
                status: CheckStatus::Pass,
                message: format!("Installed ({})", version),
                detail: None,
            }
        }
        Ok(Ok(output)) => DiagnosticCheck {
            name: "Pi CLI".to_string(),
            status: CheckStatus::Warn,
            message: format!("'pi --version' exited with code {:?}", output.status.code()),
            detail: Some("Verify Pi agent CLI installation".to_string()),
        },
        _ => {
            let which_cmd = if cfg!(target_os = "windows") {
                "where"
            } else {
                "which"
            };
            match tokio::time::timeout(
                timeout,
                tokio::process::Command::new(which_cmd).arg("pi").output(),
            )
            .await
            {
                Ok(Ok(output)) if output.status.success() => {
                    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    DiagnosticCheck {
                        name: "Pi CLI".to_string(),
                        status: CheckStatus::Pass,
                        message: format!("Available at {}", path),
                        detail: None,
                    }
                }
                _ => DiagnosticCheck {
                    name: "Pi CLI".to_string(),
                    status: CheckStatus::Warn,
                    message: "Not found on PATH".to_string(),
                    detail: Some(
                        "Pi CLI is required to drive Pi agent turns. Ensure 'pi' is in PATH."
                            .to_string(),
                    ),
                },
            }
        }
    }
}

pub async fn check_tailscale() -> DiagnosticCheck {
    let binary = crate::tailscale::tailscale_binary();
    let timeout = std::time::Duration::from_secs(3);
    let cli_available = match tokio::time::timeout(
        timeout,
        tokio::process::Command::new(binary).arg("version").output(),
    )
    .await
    {
        Ok(Ok(out)) => out.status.success(),
        _ => false,
    };

    match crate::tailscale::get_tailscale_status().await {
        Ok(status) => {
            let fqdn = status.fqdn.as_deref().unwrap_or("none");
            let ipv4 = status.ipv4.as_deref().unwrap_or("none");
            DiagnosticCheck {
                name: "Tailscale".to_string(),
                status: CheckStatus::Pass,
                message: format!("Daemon active (FQDN: {}, IPv4: {})", fqdn, ipv4),
                detail: None,
            }
        }
        Err(err) => {
            if cli_available {
                DiagnosticCheck {
                    name: "Tailscale".to_string(),
                    status: CheckStatus::Warn,
                    message: "CLI found, daemon not connected".to_string(),
                    detail: Some(err.to_string()),
                }
            } else {
                DiagnosticCheck {
                    name: "Tailscale".to_string(),
                    status: CheckStatus::Warn,
                    message: "Tailscale CLI not found on PATH".to_string(),
                    detail: Some("Tailscale is optional; required only for --share".to_string()),
                }
            }
        }
    }
}

pub async fn check_cloudflared(home_dir: &Path) -> DiagnosticCheck {
    match crate::cloud::resolve_cloudflared(home_dir).await {
        Ok(exe) => DiagnosticCheck {
            name: "cloudflared Relay".to_string(),
            status: CheckStatus::Pass,
            message: format!("Ready · version {} ({:?})", exe.version, exe.source),
            detail: Some(exe.path.display().to_string()),
        },
        Err(_) => DiagnosticCheck {
            name: "cloudflared Relay".to_string(),
            status: CheckStatus::Warn,
            message: "cloudflared not found on PATH or managed directory".to_string(),
            detail: Some(
                "cloudflared is required for T3 Connect managed tunnels. Run `t1.73code connect link` to configure."
                    .to_string(),
            ),
        },
    }
}

pub fn check_storage(home_dir: &Path) -> DiagnosticCheck {
    let userdata_dir = home_dir.join("userdata");
    if let Err(e) = std::fs::create_dir_all(&userdata_dir) {
        return DiagnosticCheck {
            name: "Storage & Home Dir".to_string(),
            status: CheckStatus::Fail,
            message: format!("Cannot create directory {}: {}", userdata_dir.display(), e),
            detail: None,
        };
    }

    let probe_file = userdata_dir.join(".doctor_probe.tmp");
    let probe_content = b"t1.73code_storage_probe";
    if let Err(e) = std::fs::write(&probe_file, probe_content) {
        return DiagnosticCheck {
            name: "Storage & Home Dir".to_string(),
            status: CheckStatus::Fail,
            message: format!(
                "Write permission check failed in {}: {}",
                userdata_dir.display(),
                e
            ),
            detail: None,
        };
    }
    let _ = std::fs::remove_file(&probe_file);

    match crate::storage::SqliteStorage::new_in_dir(home_dir) {
        Ok(_) => {
            let db_path = userdata_dir.join("statev2.sqlite");
            DiagnosticCheck {
                name: "Storage & Home Dir".to_string(),
                status: CheckStatus::Pass,
                message: format!("Read/write verified at {}", home_dir.display()),
                detail: Some(format!("Database: {}", db_path.display())),
            }
        }
        Err(e) => DiagnosticCheck {
            name: "Storage & Home Dir".to_string(),
            status: CheckStatus::Fail,
            message: format!("SQLite storage initialization failed: {}", e),
            detail: None,
        },
    }
}

pub fn read_system_memory() -> (Option<u64>, Option<u64>) {
    #[cfg(target_os = "linux")]
    {
        if let Ok(content) = std::fs::read_to_string("/proc/meminfo") {
            let mut total_kb: Option<u64> = None;
            let mut avail_kb: Option<u64> = None;
            for line in content.lines() {
                if line.starts_with("MemTotal:") {
                    total_kb = parse_meminfo_kb(line);
                } else if (line.starts_with("MemAvailable:") || line.starts_with("MemFree:"))
                    && avail_kb.is_none()
                {
                    avail_kb = parse_meminfo_kb(line);
                }
            }
            let total_mb = total_kb.map(|kb| kb / 1024);
            let avail_mb = avail_kb.map(|kb| kb / 1024);
            return (total_mb, avail_mb);
        }
    }
    (None, None)
}

fn parse_meminfo_kb(line: &str) -> Option<u64> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() >= 2 {
        parts[1].parse::<u64>().ok()
    } else {
        None
    }
}

pub fn read_process_rss() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        if let Ok(content) = std::fs::read_to_string("/proc/self/status") {
            for line in content.lines() {
                if line.starts_with("VmRSS:") {
                    if let Some(kb) = parse_meminfo_kb(line) {
                        return Some(kb / 1024);
                    }
                }
            }
        }
    }
    None
}

pub fn check_memory() -> DiagnosticCheck {
    let (total_mb, avail_mb) = read_system_memory();
    let process_rss_mb = read_process_rss();
    let budget_mb = 512;

    let mut detail_parts = Vec::new();
    if let Some(rss) = process_rss_mb {
        detail_parts.push(format!("Process RSS: ~{} MB", rss));
    }
    if let (Some(total), Some(avail)) = (total_mb, avail_mb) {
        detail_parts.push(format!(
            "System Total: {} MB, Available: {} MB",
            total, avail
        ));
    }

    let detail_str = if detail_parts.is_empty() {
        None
    } else {
        Some(detail_parts.join(" · "))
    };

    if let Some(rss) = process_rss_mb {
        if rss > budget_mb {
            return DiagnosticCheck {
                name: "Memory Budget".to_string(),
                status: CheckStatus::Warn,
                message: format!(
                    "Process RSS (~{} MB) exceeds {}MB budget limit",
                    rss, budget_mb
                ),
                detail: detail_str,
            };
        }
    }

    if let Some(avail) = avail_mb {
        if avail < 64 {
            return DiagnosticCheck {
                name: "Memory Budget".to_string(),
                status: CheckStatus::Warn,
                message: format!("Low available system RAM ({} MB available)", avail),
                detail: detail_str,
            };
        }
    }

    DiagnosticCheck {
        name: "Memory Budget".to_string(),
        status: CheckStatus::Pass,
        message: format!(
            "Verified under {}MB budget{}",
            budget_mb,
            if let Some(rss) = process_rss_mb {
                format!(" (current RSS: ~{} MB)", rss)
            } else {
                String::new()
            }
        ),
        detail: detail_str,
    }
}

pub async fn run_doctor(home_dir: &Path) -> DoctorReport {
    let mut checks = Vec::new();
    checks.push(check_pi_cli().await);
    checks.push(check_tailscale().await);
    checks.push(check_cloudflared(home_dir).await);
    checks.push(check_storage(home_dir));
    checks.push(check_memory());
    DoctorReport { checks }
}

// =========================================================================
// Main Entrypoint
// =========================================================================

pub async fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    let _ = tracing_subscriber::fmt::try_init();

    match cli.command {
        None => {
            run_server(cli.port, cli.host, cli.share, cli.connect, cli.home_dir).await?;
        }
        Some(Commands::Up {
            port,
            host,
            share,
            connect,
            home_dir,
        }) => {
            run_server(
                port,
                host,
                share || cli.share,
                connect || cli.connect,
                home_dir.or(cli.home_dir),
            )
            .await?;
        }
        Some(Commands::Serve {
            port,
            host,
            share,
            connect,
            home_dir,
        }) => {
            run_server(
                port,
                host,
                share || cli.share,
                connect || cli.connect,
                home_dir.or(cli.home_dir),
            )
            .await?;
        }
        Some(Commands::Pair {
            home_dir,
            port,
            host,
        }) => {
            let home = resolve_t3_home_dir(home_dir.as_deref().or(cli.home_dir.as_deref()));
            let storage = crate::storage::SqliteStorage::new_in_dir(&home)?;
            let (token, pairing_url) = generate_startup_pairing(&storage, &host, port)?;
            println!("Issued pairing token: {}", token);
            println!("Pairing code: {}", token);
            println!("pairingUrl: {}", pairing_url);
            println!("Pairing URL: {}", pairing_url);
        }
        Some(Commands::Doctor { home_dir }) => {
            let home = resolve_t3_home_dir(home_dir.as_deref().or(cli.home_dir.as_deref()));
            let report = run_doctor(&home).await;
            println!("{}", report.format());
            if !report.all_ok() {
                return Err("One or more doctor diagnostic checks failed".into());
            }
        }
        Some(Commands::Connect { action }) => {
            let home_dir_opt = cli.home_dir.as_deref();
            match action {
                ConnectAction::Link {
                    headless,
                    publish_only,
                } => {
                    crate::cloud::connect_link(headless, publish_only, home_dir_opt).await?;
                }
                ConnectAction::Status { json } => {
                    crate::cloud::connect_status(json, home_dir_opt).await?;
                }
                ConnectAction::Unlink => {
                    crate::cloud::connect_unlink(home_dir_opt).await?;
                }
                ConnectAction::Login { headless } => {
                    crate::cloud::connect_login(headless, home_dir_opt).await?;
                }
                ConnectAction::Logout => {
                    crate::cloud::connect_logout(home_dir_opt).await?;
                }
            }
        }
        Some(Commands::Service { action }) => {
            run_service(
                action,
                cli.port,
                cli.host,
                cli.share,
                cli.connect,
                cli.home_dir,
            )?;
        }
    }

    Ok(())
}

fn run_service(
    action: ServiceAction,
    port: u16,
    host: String,
    share: bool,
    connect: bool,
    home_dir: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "linux")]
    {
        use std::fs;
        use std::process::Command;
        let unit_dir =
            PathBuf::from(std::env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| {
                std::env::var("HOME").unwrap_or_else(|_| ".".into()) + "/.config"
            }))
            .join("systemd/user");
        let unit_path = unit_dir.join("t1_73code.service");
        match action {
            ServiceAction::Install => {
                fs::create_dir_all(&unit_dir)?;
                let executable = std::env::current_exe()?;
                let mut args = vec![
                    "up".to_string(),
                    "--port".into(),
                    port.to_string(),
                    "--host".into(),
                    host,
                ];
                if share {
                    args.push("--share".into());
                }
                if connect {
                    args.push("--connect".into());
                }
                if let Some(dir) = home_dir {
                    args.extend(["--home-dir".into(), dir]);
                }
                let quote = |value: &str| format!("\"{}\"", value.replace('"', "\\\""));
                let exec = std::iter::once(executable.to_string_lossy().to_string())
                    .chain(args)
                    .map(|s| quote(&s))
                    .collect::<Vec<_>>()
                    .join(" ");
                let unit = format!("[Unit]\nDescription=t1.73code server\nAfter=network-online.target\n\n[Service]\nType=simple\nExecStart={exec}\nRestart=on-failure\nRestartSec=5\n\n[Install]\nWantedBy=default.target\n");
                fs::write(&unit_path, unit)?;
                let status = Command::new("systemctl")
                    .args(["--user", "daemon-reload"])
                    .status()?;
                if !status.success() {
                    return Err("systemctl --user daemon-reload failed".into());
                }
                let status = Command::new("systemctl")
                    .args(["--user", "enable", "--now", "t1_73code.service"])
                    .status()?;
                if !status.success() {
                    return Err("systemd service install failed".into());
                }
                let user = std::env::var("USER").unwrap_or_default();
                let _ = Command::new("loginctl")
                    .args(["enable-linger", &user])
                    .status();
                println!("Installed and started {}", unit_path.display());
            }
            ServiceAction::Uninstall => {
                let status = Command::new("systemctl")
                    .args(["--user", "disable", "--now", "t1_73code.service"])
                    .status()?;
                if !status.success() {
                    eprintln!("Warning: systemctl disable reported a failure");
                }
                if unit_path.exists() {
                    fs::remove_file(&unit_path)?;
                }
                let _ = Command::new("systemctl")
                    .args(["--user", "daemon-reload"])
                    .status();
                println!("Removed t1_73code user service");
            }
            ServiceAction::Status => {
                let status = Command::new("systemctl")
                    .args(["--user", "status", "t1_73code.service"])
                    .status()?;
                if !status.success() {
                    return Err("Service is not active".into());
                }
            }
        }
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (action, port, host, share, connect, home_dir);
        Err("t1_73code service install currently supports Linux systemd user services".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parsing_defaults() {
        let cli = Cli::parse_from(["t1.73code"]);
        assert_eq!(cli.port, 3000);
        assert_eq!(cli.host, "0.0.0.0");
        assert!(!cli.share);
        assert!(!cli.connect);
        assert!(cli.command.is_none());
        assert!(cli.home_dir.is_none());
    }

    #[test]
    fn test_cli_parsing_up_and_serve() {
        let cli_up = Cli::parse_from([
            "t1.73code",
            "up",
            "--port",
            "8080",
            "--host",
            "0.0.0.0",
            "--share",
            "--connect",
            "--home-dir",
            "/tmp/custom_t3",
        ]);
        match cli_up.command {
            Some(Commands::Up {
                port,
                host,
                share,
                connect,
                home_dir,
            }) => {
                assert_eq!(port, 8080);
                assert_eq!(host, "0.0.0.0");
                assert!(share);
                assert!(connect);
                assert_eq!(home_dir.as_deref(), Some("/tmp/custom_t3"));
            }
            _ => panic!("Expected Commands::Up"),
        }

        let cli_serve =
            Cli::parse_from(["t1.73code", "serve", "--port", "4000", "--host", "0.0.0.0"]);
        match cli_serve.command {
            Some(Commands::Serve {
                port,
                host,
                share,
                connect,
                home_dir,
            }) => {
                assert_eq!(port, 4000);
                assert_eq!(host, "0.0.0.0");
                assert!(!share);
                assert!(!connect);
                assert!(home_dir.is_none());
            }
            _ => panic!("Expected Commands::Serve"),
        }
    }

    #[test]
    fn test_cli_parsing_pair_and_doctor() {
        let cli_pair = Cli::parse_from([
            "t1.73code",
            "pair",
            "--port",
            "3333",
            "--host",
            "10.0.0.1",
            "--home-dir",
            "/custom/home",
        ]);
        match cli_pair.command {
            Some(Commands::Pair {
                home_dir,
                port,
                host,
            }) => {
                assert_eq!(home_dir.as_deref(), Some("/custom/home"));
                assert_eq!(port, 3333);
                assert_eq!(host, "10.0.0.1");
            }
            _ => panic!("Expected Commands::Pair"),
        }

        let cli_doc = Cli::parse_from(["t1.73code", "doctor", "--home-dir", "/tmp/doc"]);
        match cli_doc.command {
            Some(Commands::Doctor { home_dir }) => {
                assert_eq!(home_dir.as_deref(), Some("/tmp/doc"));
            }
            _ => panic!("Expected Commands::Doctor"),
        }

        let cli_service = Cli::parse_from(["t1.73code", "service", "install"]);
        assert!(matches!(
            cli_service.command,
            Some(Commands::Service {
                action: ServiceAction::Install
            })
        ));
    }

    #[test]
    fn test_cli_parsing_connect_subcommands() {
        let cli_link = Cli::parse_from([
            "t1.73code",
            "connect",
            "link",
            "--headless",
            "--publish-only",
        ]);
        match cli_link.command {
            Some(Commands::Connect {
                action:
                    ConnectAction::Link {
                        headless,
                        publish_only,
                    },
            }) => {
                assert!(headless);
                assert!(publish_only);
            }
            _ => panic!("Expected ConnectAction::Link"),
        }

        let cli_status = Cli::parse_from(["t1.73code", "connect", "status", "--json"]);
        match cli_status.command {
            Some(Commands::Connect {
                action: ConnectAction::Status { json },
            }) => {
                assert!(json);
            }
            _ => panic!("Expected ConnectAction::Status"),
        }

        let cli_unlink = Cli::parse_from(["t1.73code", "connect", "unlink"]);
        match cli_unlink.command {
            Some(Commands::Connect {
                action: ConnectAction::Unlink,
            }) => {}
            _ => panic!("Expected ConnectAction::Unlink"),
        }

        let cli_login = Cli::parse_from(["t1.73code", "connect", "login", "--headless"]);
        match cli_login.command {
            Some(Commands::Connect {
                action: ConnectAction::Login { headless },
            }) => {
                assert!(headless);
            }
            _ => panic!("Expected ConnectAction::Login"),
        }

        let cli_logout = Cli::parse_from(["t1.73code", "connect", "logout"]);
        match cli_logout.command {
            Some(Commands::Connect {
                action: ConnectAction::Logout,
            }) => {}
            _ => panic!("Expected ConnectAction::Logout"),
        }
    }

    #[test]
    fn test_generate_startup_pairing() {
        let storage = crate::storage::SqliteStorage::open_in_memory().unwrap();
        let (token, pairing_url) = generate_startup_pairing(&storage, "127.0.0.1", 3000).unwrap();

        assert!(token.starts_with("pair_"));
        assert!(pairing_url.contains("http://127.0.0.1:3000/pair#token="));
        assert!(pairing_url.contains(&token));

        // Check storage has this token
        let link = storage.get_pairing_link(&token).unwrap().unwrap();
        assert_eq!(link.credential, token);
        assert_eq!(link.subject, "administrative-bootstrap");
        assert!(link.scopes.contains(&"orchestration:read".to_string()));
        assert!(link.scopes.contains(&"access:write".to_string()));
    }

    #[test]
    fn test_generate_startup_pairing_wildcard_host() {
        let storage = crate::storage::SqliteStorage::open_in_memory().unwrap();
        let (_token, pairing_url) = generate_startup_pairing(&storage, "0.0.0.0", 8080).unwrap();
        assert!(pairing_url.starts_with("http://127.0.0.1:8080/pair#token="));

        let (_token_v6, pairing_url_v6) = generate_startup_pairing(&storage, "::", 8080).unwrap();
        assert!(pairing_url_v6.starts_with("http://127.0.0.1:8080/pair#token="));
    }

    #[test]
    fn test_doctor_report_formatting() {
        let report = DoctorReport {
            checks: vec![
                DiagnosticCheck {
                    name: "Pi CLI".to_string(),
                    status: CheckStatus::Pass,
                    message: "Installed (pi v0.1.0)".to_string(),
                    detail: None,
                },
                DiagnosticCheck {
                    name: "Tailscale".to_string(),
                    status: CheckStatus::Warn,
                    message: "CLI found, daemon not connected".to_string(),
                    detail: Some("Optional for --share".to_string()),
                },
                DiagnosticCheck {
                    name: "Storage & Home Dir".to_string(),
                    status: CheckStatus::Pass,
                    message: "Read/write verified".to_string(),
                    detail: Some("DB: statev2.sqlite".to_string()),
                },
                DiagnosticCheck {
                    name: "Memory Budget".to_string(),
                    status: CheckStatus::Pass,
                    message: "Verified under 512MB budget".to_string(),
                    detail: None,
                },
            ],
        };

        assert!(report.all_ok());
        let formatted = report.format();
        assert!(formatted.contains("[PASS] Pi CLI"));
        assert!(formatted.contains("[WARN] Tailscale"));
        assert!(formatted.contains("Summary: 3 passed, 1 warning(s), 0 failed"));
        assert!(formatted.contains("Result: All essential systems operational"));

        let report_with_failure = DoctorReport {
            checks: vec![DiagnosticCheck {
                name: "Storage".to_string(),
                status: CheckStatus::Fail,
                message: "Permission denied".to_string(),
                detail: None,
            }],
        };
        assert!(!report_with_failure.all_ok());
        let formatted_fail = report_with_failure.format();
        assert!(formatted_fail.contains("[FAIL] Storage"));
        assert!(formatted_fail.contains("Action required"));
    }

    #[test]
    fn test_check_storage() {
        let tmp = tempfile::tempdir().unwrap();
        let check = check_storage(tmp.path());
        assert_eq!(check.status, CheckStatus::Pass);
        assert!(check.message.contains("Read/write verified"));
        assert!(tmp.path().join("userdata").join("statev2.sqlite").exists());
    }

    #[test]
    fn test_check_memory() {
        let check = check_memory();
        assert!(check.status == CheckStatus::Pass || check.status == CheckStatus::Warn);
        assert!(check.message.contains("512MB budget"));
    }

    #[test]
    fn test_resolve_connector_token() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();

        // Initially none
        assert!(resolve_connector_token(home).is_none());

        // 1. From CLOUD_ENDPOINT_RUNTIME_CONFIG_SECRET
        let runtime_cfg = serde_json::json!({
            "connectorToken": "tok_runtime_123",
            "tunnelId": "tun_abc",
            "tunnelName": "t3-tunnel"
        });
        crate::cloud::write_secret(
            home,
            crate::cloud::CLOUD_ENDPOINT_RUNTIME_CONFIG_SECRET,
            &serde_json::to_vec(&runtime_cfg).unwrap(),
        )
        .unwrap();

        let (tok, id, name) = resolve_connector_token(home).unwrap();
        assert_eq!(tok, "tok_runtime_123");
        assert_eq!(id.as_deref(), Some("tun_abc"));
        assert_eq!(name.as_deref(), Some("t3-tunnel"));

        // 2. From RELAY_ENVIRONMENT_CREDENTIAL_SECRET when runtime config absent
        crate::cloud::remove_secret(home, crate::cloud::CLOUD_ENDPOINT_RUNTIME_CONFIG_SECRET)
            .unwrap();
        crate::cloud::write_secret(
            home,
            crate::cloud::RELAY_ENVIRONMENT_CREDENTIAL_SECRET,
            b"cred_relay_456",
        )
        .unwrap();

        let (tok2, id2, name2) = resolve_connector_token(home).unwrap();
        assert_eq!(tok2, "cred_relay_456");
        assert!(id2.is_none());
        assert!(name2.is_none());
    }

    #[tokio::test]
    async fn test_server_http_stack_and_shutdown() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().to_path_buf();
        let storage = Arc::new(crate::storage::SqliteStorage::new_in_dir(&home).unwrap());
        let app_state = Arc::new(crate::state::AppState::new(home.clone(), storage.clone()));
        let router = crate::http::build_router(app_state);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        let server_handle = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.await;
                })
                .await
                .unwrap();
        });

        // Test HTTP call to discovery endpoint
        let client = reqwest::Client::new();
        let resp = client
            .get(format!(
                "http://127.0.0.1:{}/.well-known/t3/environment",
                port
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        let json: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(json["serverVersion"], "0.1.73");

        // Trigger shutdown and wait for server to exit
        let _ = shutdown_tx.send(());
        let _ = server_handle.await;
    }
}
