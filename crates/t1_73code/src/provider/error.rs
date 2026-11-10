use std::fmt;

#[derive(Debug)]
pub enum PiError {
    Spawn(std::io::Error),
    Io(std::io::Error),
    Json(serde_json::Error),
    Timeout {
        operation: String,
        timeout_ms: u64,
    },
    ProcessExited {
        exit_code: Option<i32>,
        detail: String,
    },
    StdoutClosed,
    RpcError {
        operation: String,
        detail: String,
    },
    BufferLimitExceeded {
        size: usize,
        limit: usize,
    },
    Cancelled,
    InvalidResponse(String),
}

impl fmt::Display for PiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn(err) => write!(f, "Failed to spawn Pi process: {err}"),
            Self::Io(err) => write!(f, "Pi IO error: {err}"),
            Self::Json(err) => write!(f, "Pi JSON error: {err}"),
            Self::Timeout {
                operation,
                timeout_ms,
            } => {
                write!(f, "Pi RPC {operation} timed out after {timeout_ms}ms")
            }
            Self::ProcessExited { exit_code, detail } => match exit_code {
                Some(code) => write!(f, "Pi process exited with code {code}: {detail}"),
                None => write!(f, "Pi process exited unexpectedly: {detail}"),
            },
            Self::StdoutClosed => write!(f, "Pi process closed stdout"),
            Self::RpcError { operation, detail } => {
                write!(f, "Pi RPC {operation} failed: {detail}")
            }
            Self::BufferLimitExceeded { size, limit } => {
                write!(f, "Pi record exceeded buffer limit: {size} > {limit}")
            }
            Self::Cancelled => write!(f, "Pi operation was cancelled"),
            Self::InvalidResponse(detail) => write!(f, "Invalid Pi response: {detail}"),
        }
    }
}

impl std::error::Error for PiError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Spawn(err) | Self::Io(err) => Some(err),
            Self::Json(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for PiError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<serde_json::Error> for PiError {
    fn from(err: serde_json::Error) -> Self {
        Self::Json(err)
    }
}
