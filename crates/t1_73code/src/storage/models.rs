use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

pub use crate::protocol::models::{
    ModelSelection, OrchestrationProjectShell, OrchestrationV2Run, OrchestrationV2ThreadProjection,
    OrchestrationV2ThreadShell, OrchestrationV2TurnItem,
};

#[derive(Debug)]
pub enum StorageError {
    Sqlite(rusqlite::Error),
    Serialization(serde_json::Error),
    InvalidData(String),
    NotFound(String),
    Io(std::io::Error),
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sqlite(e) => write!(f, "Database error: {e}"),
            Self::Serialization(e) => write!(f, "Serialization error: {e}"),
            Self::InvalidData(msg) => write!(f, "Invalid data: {msg}"),
            Self::NotFound(msg) => write!(f, "Not found: {msg}"),
            Self::Io(e) => write!(f, "IO error: {e}"),
        }
    }
}

impl std::error::Error for StorageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Sqlite(e) => Some(e),
            Self::Serialization(e) => Some(e),
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<rusqlite::Error> for StorageError {
    fn from(err: rusqlite::Error) -> Self {
        Self::Sqlite(err)
    }
}

impl From<serde_json::Error> for StorageError {
    fn from(err: serde_json::Error) -> Self {
        Self::Serialization(err)
    }
}

impl From<std::io::Error> for StorageError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthSession {
    pub session_id: String,
    pub subject: String,
    pub scopes: Vec<String>,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_ip_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_user_agent: Option<String>,
    pub client_device_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_os: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_browser: Option<String>,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_connected_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingLink {
    pub id: String,
    pub credential: String,
    pub method: String,
    pub scopes: Vec<String>,
    pub subject: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consumed_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestrationMessage {
    pub id: String,
    pub thread_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    pub role: String,
    pub text: String,
    #[serde(default)]
    pub streaming: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateThreadInput {
    pub id: Option<String>,
    pub project_id: String,
    pub title: String,
    #[serde(default = "default_provider")]
    pub provider_instance_id: String,
    pub model_selection: Option<ModelSelection>,
    #[serde(default = "default_runtime_mode")]
    pub runtime_mode: String,
    #[serde(default = "default_interaction_mode")]
    pub interaction_mode: String,
    pub branch: Option<String>,
    pub worktree_path: Option<String>,
}

fn default_provider() -> String {
    "codex".into()
}

fn default_runtime_mode() -> String {
    "full-access".into()
}

fn default_interaction_mode() -> String {
    "default".into()
}

impl From<CreateThreadInput> for OrchestrationV2ThreadShell {
    fn from(input: CreateThreadInput) -> Self {
        let now = Utc::now();
        let provider = if input.provider_instance_id.is_empty() {
            default_provider()
        } else {
            input.provider_instance_id
        };
        let model_selection = input.model_selection.unwrap_or_else(|| ModelSelection {
            instance_id: provider.clone(),
            model: "default".into(),
            options: None,
        });
        Self {
            id: input
                .id
                .unwrap_or_else(|| format!("thread:{}", uuid::Uuid::new_v4())),
            project_id: input.project_id,
            title: input.title,
            provider_instance_id: provider,
            model_selection,
            runtime_mode: if input.runtime_mode.is_empty() {
                default_runtime_mode()
            } else {
                input.runtime_mode
            },
            interaction_mode: if input.interaction_mode.is_empty() {
                default_interaction_mode()
            } else {
                input.interaction_mode
            },
            branch: input.branch,
            worktree_path: input.worktree_path,
            created_at: now,
            updated_at: now,
            archived_at: None,
            deleted_at: None,
            status: None,
            latest_run_id: None,
            item_count: 0,
        }
    }
}

impl From<&OrchestrationV2ThreadShell> for OrchestrationV2ThreadShell {
    fn from(s: &OrchestrationV2ThreadShell) -> Self {
        s.clone()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRunInput {
    pub id: Option<String>,
    pub thread_id: String,
    pub ordinal: Option<u64>,
    #[serde(default = "default_provider")]
    pub provider_instance_id: String,
    pub model_selection: Option<ModelSelection>,
    #[serde(default = "default_run_status")]
    pub status: String,
    pub requested_at: Option<DateTime<Utc>>,
}

fn default_run_status() -> String {
    "queued".into()
}

impl From<CreateRunInput> for OrchestrationV2Run {
    fn from(input: CreateRunInput) -> Self {
        let now = Utc::now();
        let provider = if input.provider_instance_id.is_empty() {
            default_provider()
        } else {
            input.provider_instance_id
        };
        let model_selection = input.model_selection.unwrap_or_else(|| ModelSelection {
            instance_id: provider.clone(),
            model: "default".into(),
            options: None,
        });
        let ordinal = input.ordinal.unwrap_or(0);
        let id = input.id.unwrap_or_default();
        Self {
            id,
            thread_id: input.thread_id,
            ordinal,
            provider_instance_id: provider,
            model_selection,
            status: if input.status.is_empty() {
                default_run_status()
            } else {
                input.status
            },
            requested_at: input.requested_at.unwrap_or(now),
            started_at: None,
            completed_at: None,
        }
    }
}

impl From<&OrchestrationV2Run> for OrchestrationV2Run {
    fn from(r: &OrchestrationV2Run) -> Self {
        r.clone()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTurnItemInput {
    pub id: Option<String>,
    pub thread_id: String,
    pub run_id: Option<String>,
    pub ordinal: Option<u64>,
    #[serde(default = "default_item_status")]
    pub status: String,
    #[serde(rename = "type")]
    pub item_type: String,
    pub title: Option<String>,
    pub text: Option<String>,
    pub input: Option<String>,
    pub output: Option<String>,
    pub exit_code: Option<i32>,
    pub created_at: Option<DateTime<Utc>>,
}

fn default_item_status() -> String {
    "completed".into()
}

impl From<CreateTurnItemInput> for OrchestrationV2TurnItem {
    fn from(input: CreateTurnItemInput) -> Self {
        let now = Utc::now();
        let ordinal = input.ordinal.unwrap_or(0);
        let id = input.id.unwrap_or_default();
        Self {
            id,
            thread_id: input.thread_id,
            run_id: input.run_id,
            ordinal,
            status: if input.status.is_empty() {
                default_item_status()
            } else {
                input.status
            },
            item_type: input.item_type,
            title: input.title,
            text: input.text,
            input: input.input,
            output: input.output,
            exit_code: input.exit_code,
            created_at: Some(input.created_at.unwrap_or(now)),
            updated_at: now,
        }
    }
}

impl From<&OrchestrationV2TurnItem> for OrchestrationV2TurnItem {
    fn from(item: &OrchestrationV2TurnItem) -> Self {
        item.clone()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateMessageInput {
    pub id: Option<String>,
    pub thread_id: String,
    pub run_id: Option<String>,
    pub node_id: Option<String>,
    pub role: String,
    pub text: String,
    #[serde(default)]
    pub streaming: bool,
    pub created_at: Option<DateTime<Utc>>,
}

impl From<CreateMessageInput> for OrchestrationMessage {
    fn from(input: CreateMessageInput) -> Self {
        let now = Utc::now();
        let id = input.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        Self {
            id,
            thread_id: input.thread_id,
            run_id: input.run_id,
            node_id: input.node_id,
            role: input.role,
            text: input.text,
            streaming: input.streaming,
            created_at: input.created_at.unwrap_or(now),
            updated_at: now,
        }
    }
}

impl From<&OrchestrationMessage> for OrchestrationMessage {
    fn from(m: &OrchestrationMessage) -> Self {
        m.clone()
    }
}

pub fn format_datetime(dt: DateTime<Utc>) -> String {
    dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub fn parse_datetime(s: &str) -> Result<DateTime<Utc>, StorageError> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
                .map(|naive| DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc))
        })
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.fZ")
                .map(|naive| DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc))
        })
        .map_err(|e| StorageError::InvalidData(format!("Invalid datetime string '{s}': {e}")))
}

pub fn parse_datetime_opt(s: Option<String>) -> Result<Option<DateTime<Utc>>, StorageError> {
    match s {
        Some(val) if !val.is_empty() => parse_datetime(&val).map(Some),
        _ => Ok(None),
    }
}
