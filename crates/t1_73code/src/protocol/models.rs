use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionEnvironmentDescriptor {
    pub environment_id: String,
    pub label: String,
    pub platform: ExecutionEnvironmentPlatform,
    pub server_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orchestration_protocol_version: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database_migration: Option<u32>,
    pub capabilities: ExecutionEnvironmentCapabilities,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionEnvironmentPlatform {
    pub os: String,
    pub arch: String,
    pub machine: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionEnvironmentCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice_transcription: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloud_voice_transcription: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerConfig {
    pub environment: ExecutionEnvironmentDescriptor,
    pub auth: ServerAuthDescriptor,
    pub cwd: String,
    pub keybindings_config_path: String,
    pub keybindings: ResolvedKeybindingsConfig,
    pub issues: Vec<String>,
    pub providers: Vec<ServerProvider>,
    pub available_editors: Vec<String>,
    pub observability: ServerObservability,
    pub settings: ServerSettings,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell_thread_families: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_link_metadata: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_snapshot_pagination: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ServerAuthDescriptor {
    pub active_methods: Vec<String>,
    pub token_exchange: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedKeybindingsConfig {
    pub bindings: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerProvider {
    pub instance_id: String,
    pub driver: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub models: Vec<ServerProviderModel>,
    pub enabled: bool,
    pub status: ServerProviderStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_modes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slash_commands: Option<Vec<ServerProviderSlashCommand>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<ServerProviderSkill>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerProviderModel {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerProviderStatus {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerProviderSlashCommand {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerProviderSkill {
    pub name: String,
    pub path: String,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ServerObservability {
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ServerSettings {
    #[serde(default)]
    pub default_provider: Option<String>,
    #[serde(default)]
    pub default_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestrationProjectShell {
    pub id: String,
    pub title: String,
    pub workspace_root: String,
    pub scripts: Vec<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestrationV2ThreadShell {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub provider_instance_id: String,
    pub model_selection: ModelSelection,
    pub runtime_mode: String,
    pub interaction_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree_path: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_run_id: Option<String>,
    #[serde(default)]
    pub item_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSelection {
    pub instance_id: String,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestrationV2ShellSnapshot {
    pub sequence: u64,
    pub projects: Vec<OrchestrationProjectShell>,
    pub threads: Vec<OrchestrationV2ThreadShell>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum OrchestrationV2ShellStreamItem {
    Synchronized,
    #[serde(rename = "snapshot")]
    Snapshot {
        snapshot: OrchestrationV2ShellSnapshot,
    },
    #[serde(rename = "thread.updated")]
    ThreadUpdated {
        sequence: u64,
        location: String,
        thread: OrchestrationV2ThreadShell,
    },
    #[serde(rename = "thread.removed")]
    ThreadRemoved {
        sequence: u64,
        location: String,
        thread_id: String,
    },
    #[serde(rename = "project.updated")]
    ProjectUpdated {
        sequence: u64,
        project: OrchestrationProjectShell,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestrationV2TurnItem {
    pub id: String,
    pub thread_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    pub ordinal: u64,
    pub status: String,
    #[serde(rename = "type")]
    pub item_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestrationV2ThreadProjection {
    pub thread: OrchestrationV2ThreadShell,
    pub runs: Vec<OrchestrationV2Run>,
    pub turn_items: Vec<OrchestrationV2TurnItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestrationV2Run {
    pub id: String,
    pub thread_id: String,
    pub ordinal: u64,
    pub provider_instance_id: String,
    pub model_selection: ModelSelection,
    pub status: String,
    pub requested_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum OrchestrationV2ThreadStreamItem {
    Synchronized,
    #[serde(rename = "snapshot")]
    Snapshot {
        snapshot_sequence: u64,
        projection: OrchestrationV2ThreadProjection,
    },
    #[serde(rename = "event")]
    Event {
        sequence: u64,
        event: serde_json::Value,
    },
}
