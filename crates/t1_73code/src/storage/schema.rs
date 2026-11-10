use crate::storage::models::StorageError;
use rusqlite::Connection;

pub const INIT_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS projection_projects (
    project_id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    workspace_root TEXT NOT NULL,
    scripts_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT,
    default_model_selection_json TEXT,
    default_thread_env_mode TEXT,
    favicon_path TEXT,
    auto_pull INTEGER NOT NULL DEFAULT 0,
    project_icon_json TEXT,
    pinned_at TEXT,
    archived_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_projection_projects_updated_at
    ON projection_projects(updated_at);
CREATE INDEX IF NOT EXISTS idx_projection_projects_workspace_root_deleted_at
    ON projection_projects(workspace_root, deleted_at);

CREATE TABLE IF NOT EXISTS projection_threads (
    thread_id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    title TEXT NOT NULL,
    branch TEXT,
    worktree_path TEXT,
    latest_turn_id TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT,
    runtime_mode TEXT NOT NULL DEFAULT 'full-access',
    interaction_mode TEXT NOT NULL DEFAULT 'default',
    model_selection_json TEXT,
    archived_at TEXT,
    latest_user_message_at TEXT,
    pending_approval_count INTEGER NOT NULL DEFAULT 0,
    pending_user_input_count INTEGER NOT NULL DEFAULT 0,
    has_actionable_proposed_plan INTEGER NOT NULL DEFAULT 0,
    settled_override TEXT,
    settled_at TEXT,
    snoozed_until TEXT,
    snoozed_at TEXT,
    title_regeneration_request_id TEXT,
    title_regeneration_started_at TEXT,
    pinned_at TEXT,
    pin_order_key TEXT,
    linked_pull_request_json TEXT,
    unsettled_at TEXT,
    branch_pull_request_json TEXT,
    active_order_key TEXT,
    title_state_json TEXT,
    auto_settle_disabled_at TEXT
);

CREATE TABLE IF NOT EXISTS orchestration_v2_projection_threads (
    thread_id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    title TEXT NOT NULL,
    default_provider TEXT NOT NULL,
    runtime_mode TEXT NOT NULL,
    interaction_mode TEXT NOT NULL,
    active_provider_thread_id TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    archived_at TEXT,
    deleted_at TEXT,
    payload_json TEXT NOT NULL,
    provider_instance_id TEXT
);

CREATE INDEX IF NOT EXISTS orchestration_v2_projection_threads_project_updated_idx
    ON orchestration_v2_projection_threads(project_id, updated_at);

CREATE TABLE IF NOT EXISTS orchestration_v2_projection_runs (
    run_id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL,
    provider TEXT NOT NULL,
    provider_thread_id TEXT,
    status TEXT NOT NULL,
    requested_at TEXT NOT NULL,
    completed_at TEXT,
    payload_json TEXT NOT NULL,
    provider_instance_id TEXT
);

CREATE UNIQUE INDEX IF NOT EXISTS orchestration_v2_projection_runs_thread_ordinal_idx
    ON orchestration_v2_projection_runs(thread_id, ordinal);
CREATE INDEX IF NOT EXISTS orchestration_v2_projection_runs_provider_thread_idx
    ON orchestration_v2_projection_runs(provider_thread_id);
CREATE INDEX IF NOT EXISTS orchestration_v2_projection_runs_thread_status_idx
    ON orchestration_v2_projection_runs(thread_id, status);

CREATE TABLE IF NOT EXISTS orchestration_v2_projection_turn_items (
    turn_item_id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL,
    run_id TEXT,
    node_id TEXT,
    provider_thread_id TEXT,
    provider_turn_id TEXT,
    parent_item_id TEXT,
    ordinal INTEGER NOT NULL,
    type TEXT NOT NULL,
    status TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    payload_json TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS orchestration_v2_projection_turn_items_thread_ordinal_idx
    ON orchestration_v2_projection_turn_items(thread_id, ordinal, turn_item_id);
CREATE INDEX IF NOT EXISTS orchestration_v2_projection_turn_items_run_ordinal_idx
    ON orchestration_v2_projection_turn_items(run_id, ordinal);
CREATE INDEX IF NOT EXISTS orchestration_v2_projection_turn_items_thread_run_idx
    ON orchestration_v2_projection_turn_items(thread_id, run_id);

CREATE TABLE IF NOT EXISTS orchestration_v2_turn_item_positions (
    thread_id TEXT NOT NULL,
    turn_item_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL,
    PRIMARY KEY (thread_id, turn_item_id),
    UNIQUE (thread_id, ordinal)
);

CREATE TABLE IF NOT EXISTS orchestration_v2_projection_messages (
    message_id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL,
    run_id TEXT,
    node_id TEXT,
    role TEXT NOT NULL,
    streaming INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    payload_json TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS orchestration_v2_projection_messages_thread_created_idx
    ON orchestration_v2_projection_messages(thread_id, created_at, message_id);
CREATE INDEX IF NOT EXISTS orchestration_v2_projection_messages_run_idx
    ON orchestration_v2_projection_messages(run_id);

CREATE TABLE IF NOT EXISTS auth_pairing_links (
    id TEXT PRIMARY KEY,
    credential TEXT NOT NULL UNIQUE,
    method TEXT NOT NULL,
    scopes TEXT NOT NULL,
    subject TEXT NOT NULL,
    label TEXT,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    consumed_at TEXT,
    revoked_at TEXT,
    proof_key_thumbprint TEXT
);

CREATE INDEX IF NOT EXISTS idx_auth_pairing_links_active
    ON auth_pairing_links(revoked_at, consumed_at, expires_at);

CREATE TABLE IF NOT EXISTS auth_sessions (
    session_id TEXT PRIMARY KEY,
    subject TEXT NOT NULL,
    scopes TEXT NOT NULL,
    method TEXT NOT NULL,
    client_label TEXT,
    client_ip_address TEXT,
    client_user_agent TEXT,
    client_device_type TEXT NOT NULL DEFAULT 'unknown',
    client_os TEXT,
    client_browser TEXT,
    issued_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    last_connected_at TEXT,
    revoked_at TEXT,
    client_surface TEXT,
    client_app_version TEXT
);

CREATE INDEX IF NOT EXISTS idx_auth_sessions_active
    ON auth_sessions(revoked_at, expires_at, issued_at);

CREATE TABLE IF NOT EXISTS auth_tokens (
    token TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    expires_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_auth_tokens_token
    ON auth_tokens(token);

CREATE TABLE IF NOT EXISTS orchestration_events (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT NOT NULL UNIQUE,
    aggregate_kind TEXT NOT NULL,
    stream_id TEXT NOT NULL,
    stream_version INTEGER NOT NULL,
    event_type TEXT NOT NULL,
    occurred_at TEXT NOT NULL,
    command_id TEXT,
    causation_event_id TEXT,
    correlation_id TEXT,
    actor_kind TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    metadata_json TEXT NOT NULL,
    application_event_version INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS orchestration_v2_events (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT NOT NULL UNIQUE,
    command_id TEXT,
    thread_id TEXT NOT NULL,
    run_id TEXT,
    node_id TEXT,
    provider TEXT,
    raw_event_id TEXT,
    event_type TEXT NOT NULL,
    occurred_at TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    driver TEXT,
    provider_instance_id TEXT
);
"#;

pub fn init_database(conn: &Connection) -> Result<(), StorageError> {
    conn.execute_batch(INIT_SCHEMA)?;
    Ok(())
}
