use super::status::{parse_tailscale_status, TailscaleError, TailscaleStatus};
use std::time::Duration;
use tokio::process::Command;

pub fn tailscale_binary() -> &'static str {
    if cfg!(target_os = "windows") {
        "tailscale.exe"
    } else {
        "tailscale"
    }
}

pub async fn get_tailscale_status() -> Result<TailscaleStatus, TailscaleError> {
    let binary = tailscale_binary();
    let timeout = Duration::from_secs(5);

    let output = match tokio::time::timeout(
        timeout,
        Command::new(binary).args(["status", "--json"]).output(),
    )
    .await
    {
        Ok(Ok(output)) => output,
        Ok(Err(err)) => {
            return Err(TailscaleError::CommandSpawnFailed {
                command: format!("{} status --json", binary),
                source: err,
            })
        }
        Err(_) => {
            return Err(TailscaleError::Timeout(format!(
                "{} status --json timed out after {:?}",
                binary, timeout
            )))
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(TailscaleError::CommandFailed {
            command: format!("{} status --json", binary),
            code: output.status.code(),
            stderr,
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_tailscale_status(&stdout)
}

pub async fn tailscale_serve(port: u16) -> Result<(), TailscaleError> {
    let binary = tailscale_binary();
    let port_str = port.to_string();
    let timeout = Duration::from_secs(10);

    let output = match tokio::time::timeout(
        timeout,
        Command::new(binary)
            .args(["serve", "--bg", &port_str])
            .output(),
    )
    .await
    {
        Ok(Ok(output)) => output,
        Ok(Err(err)) => {
            return Err(TailscaleError::CommandSpawnFailed {
                command: format!("{} serve --bg {}", binary, port),
                source: err,
            })
        }
        Err(_) => {
            return Err(TailscaleError::Timeout(format!(
                "{} serve --bg {} timed out after {:?}",
                binary, port, timeout
            )))
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(TailscaleError::CommandFailed {
            command: format!("{} serve --bg {}", binary, port),
            code: output.status.code(),
            stderr,
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_live_get_tailscale_status() {
        // If tailscale is available and working on the machine, verify it parses properly.
        if let Ok(status) = get_tailscale_status().await {
            assert!(status.fqdn.is_some() || status.ipv4.is_some());
        }
    }
}
