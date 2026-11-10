use crate::storage::models::*;
use crate::storage::schema::init_database;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Storage {
    conn: Arc<Mutex<Connection>>,
}

pub type SqliteStorage = Storage;

impl Storage {
    pub fn new(conn: Connection) -> Result<Self, StorageError> {
        let storage = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        storage.init_schema()?;
        Ok(storage)
    }

    pub fn init_schema(&self) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        init_database(&conn)
    }

    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, StorageError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;",
        )?;
        Self::new(conn)
    }

    pub fn open_in_memory() -> Result<Self, StorageError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;",
        )?;
        Self::new(conn)
    }

    pub fn new_in_dir<P: AsRef<Path>>(home_dir: P) -> Result<Self, StorageError> {
        let db_path = home_dir.as_ref().join("userdata").join("statev2.sqlite");
        Self::open(db_path)
    }

    // ==========================================
    // Projects CRUD
    // ==========================================

    pub fn get_or_create_project(
        &self,
        workspace_root: &str,
        title: &str,
    ) -> Result<OrchestrationProjectShell, StorageError> {
        let conn = self.conn.lock().unwrap();
        let existing = conn
            .query_row(
                "SELECT project_id, title, workspace_root, scripts_json, created_at, updated_at, deleted_at
                 FROM projection_projects
                 WHERE workspace_root = ?1 AND deleted_at IS NULL
                 ORDER BY created_at ASC
                 LIMIT 1;",
                params![workspace_root],
                |row| {
                    let id: String = row.get(0)?;
                    let title: String = row.get(1)?;
                    let root: String = row.get(2)?;
                    let scripts_raw: String = row.get(3)?;
                    let created_str: String = row.get(4)?;
                    let updated_str: String = row.get(5)?;
                    let deleted_str: Option<String> = row.get(6)?;
                    Ok((id, title, root, scripts_raw, created_str, updated_str, deleted_str))
                },
            )
            .optional()?;

        if let Some((id, title, root, scripts_raw, created_str, updated_str, deleted_str)) =
            existing
        {
            let scripts = serde_json::from_str(&scripts_raw).unwrap_or_default();
            return Ok(OrchestrationProjectShell {
                id,
                title,
                workspace_root: root,
                scripts,
                created_at: parse_datetime(&created_str)?,
                updated_at: parse_datetime(&updated_str)?,
                deleted_at: parse_datetime_opt(deleted_str)?,
            });
        }

        let now = Utc::now();
        let now_str = format_datetime(now);
        let id = uuid::Uuid::new_v4().to_string();

        conn.execute(
            "INSERT INTO projection_projects (
                project_id, title, workspace_root, scripts_json, created_at, updated_at, deleted_at
            ) VALUES (?1, ?2, ?3, '[]', ?4, ?4, NULL);",
            params![id, title, workspace_root, now_str],
        )?;

        Ok(OrchestrationProjectShell {
            id,
            title: title.to_string(),
            workspace_root: workspace_root.to_string(),
            scripts: Vec::new(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
        })
    }

    pub fn list_projects(&self) -> Result<Vec<OrchestrationProjectShell>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT project_id, title, workspace_root, scripts_json, created_at, updated_at, deleted_at
             FROM projection_projects
             WHERE deleted_at IS NULL
             ORDER BY created_at ASC, project_id ASC;",
        )?;

        let rows = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let title: String = row.get(1)?;
            let workspace_root: String = row.get(2)?;
            let scripts_raw: String = row.get(3)?;
            let created_str: String = row.get(4)?;
            let updated_str: String = row.get(5)?;
            let deleted_str: Option<String> = row.get(6)?;
            Ok((
                id,
                title,
                workspace_root,
                scripts_raw,
                created_str,
                updated_str,
                deleted_str,
            ))
        })?;

        let mut projects = Vec::new();
        for row in rows {
            let (id, title, workspace_root, scripts_raw, created_str, updated_str, deleted_str) =
                row?;
            let scripts = serde_json::from_str(&scripts_raw).unwrap_or_default();
            projects.push(OrchestrationProjectShell {
                id,
                title,
                workspace_root,
                scripts,
                created_at: parse_datetime(&created_str)?,
                updated_at: parse_datetime(&updated_str)?,
                deleted_at: parse_datetime_opt(deleted_str)?,
            });
        }
        Ok(projects)
    }

    pub fn get_project(&self, id: &str) -> Result<Option<OrchestrationProjectShell>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let result = conn
            .query_row(
                "SELECT project_id, title, workspace_root, scripts_json, created_at, updated_at, deleted_at
                 FROM projection_projects
                 WHERE project_id = ?1;",
                params![id],
                |row| {
                    let id: String = row.get(0)?;
                    let title: String = row.get(1)?;
                    let workspace_root: String = row.get(2)?;
                    let scripts_raw: String = row.get(3)?;
                    let created_str: String = row.get(4)?;
                    let updated_str: String = row.get(5)?;
                    let deleted_str: Option<String> = row.get(6)?;
                    Ok((id, title, workspace_root, scripts_raw, created_str, updated_str, deleted_str))
                },
            )
            .optional()?;

        match result {
            Some((
                id,
                title,
                workspace_root,
                scripts_raw,
                created_str,
                updated_str,
                deleted_str,
            )) => {
                let scripts = serde_json::from_str(&scripts_raw).unwrap_or_default();
                Ok(Some(OrchestrationProjectShell {
                    id,
                    title,
                    workspace_root,
                    scripts,
                    created_at: parse_datetime(&created_str)?,
                    updated_at: parse_datetime(&updated_str)?,
                    deleted_at: parse_datetime_opt(deleted_str)?,
                }))
            }
            None => Ok(None),
        }
    }

    pub fn update_project(
        &self,
        project: &OrchestrationProjectShell,
    ) -> Result<OrchestrationProjectShell, StorageError> {
        let conn = self.conn.lock().unwrap();
        let scripts_json = serde_json::to_string(&project.scripts)?;
        let updated_at_str = format_datetime(project.updated_at);
        let deleted_at_str = project.deleted_at.map(format_datetime);

        let count = conn.execute(
            "UPDATE projection_projects
             SET title = ?2,
                 workspace_root = ?3,
                 scripts_json = ?4,
                 updated_at = ?5,
                 deleted_at = ?6
             WHERE project_id = ?1;",
            params![
                project.id,
                project.title,
                project.workspace_root,
                scripts_json,
                updated_at_str,
                deleted_at_str
            ],
        )?;

        if count == 0 {
            return Err(StorageError::NotFound(format!(
                "Project {} not found",
                project.id
            )));
        }

        Ok(project.clone())
    }

    pub fn delete_project(&self, id: &str) -> Result<bool, StorageError> {
        let conn = self.conn.lock().unwrap();
        let now_str = format_datetime(Utc::now());
        let count = conn.execute(
            "UPDATE projection_projects
             SET deleted_at = ?2, updated_at = ?2
             WHERE project_id = ?1 AND deleted_at IS NULL;",
            params![id, now_str],
        )?;
        Ok(count > 0)
    }

    // ==========================================
    // Threads CRUD
    // ==========================================

    pub fn create_thread(
        &self,
        thread: impl Into<OrchestrationV2ThreadShell>,
    ) -> Result<OrchestrationV2ThreadShell, StorageError> {
        let thread = thread.into();
        let conn = self.conn.lock().unwrap();

        let created_at_str = format_datetime(thread.created_at);
        let updated_at_str = format_datetime(thread.updated_at);
        let archived_at_str = thread.archived_at.map(format_datetime);
        let deleted_at_str = thread.deleted_at.map(format_datetime);

        let payload = serde_json::json!({
            "id": thread.id,
            "projectId": thread.project_id,
            "title": thread.title,
            "providerInstanceId": thread.provider_instance_id,
            "modelSelection": thread.model_selection,
            "runtimeMode": thread.runtime_mode,
            "interactionMode": thread.interaction_mode,
            "branch": thread.branch,
            "worktreePath": thread.worktree_path,
            "pullRequests": [],
            "activeProviderThreadId": null,
            "lineage": {
                "parentThreadId": null,
                "relationshipToParent": null,
                "rootThreadId": thread.id
            },
            "forkedFrom": null,
            "createdAt": created_at_str,
            "updatedAt": updated_at_str,
            "archivedAt": archived_at_str,
            "deletedAt": deleted_at_str,
            "status": thread.status,
            "settledOverride": null,
            "settledAt": null
        });
        let payload_json = serde_json::to_string(&payload)?;
        let model_selection_json = serde_json::to_string(&thread.model_selection)?;

        conn.execute(
            "INSERT INTO orchestration_v2_projection_threads (
                thread_id,
                project_id,
                title,
                default_provider,
                provider_instance_id,
                runtime_mode,
                interaction_mode,
                active_provider_thread_id,
                created_at,
                updated_at,
                archived_at,
                deleted_at,
                payload_json
            ) VALUES (?1, ?2, ?3, ?4, ?4, ?5, ?6, NULL, ?7, ?8, ?9, ?10, ?11);",
            params![
                thread.id,
                thread.project_id,
                thread.title,
                thread.provider_instance_id,
                thread.runtime_mode,
                thread.interaction_mode,
                created_at_str,
                updated_at_str,
                archived_at_str,
                deleted_at_str,
                payload_json
            ],
        )?;

        let _ = conn.execute(
            "INSERT OR REPLACE INTO projection_threads (
                thread_id,
                project_id,
                title,
                branch,
                worktree_path,
                created_at,
                updated_at,
                deleted_at,
                runtime_mode,
                interaction_mode,
                model_selection_json,
                archived_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12);",
            params![
                thread.id,
                thread.project_id,
                thread.title,
                thread.branch,
                thread.worktree_path,
                created_at_str,
                updated_at_str,
                deleted_at_str,
                thread.runtime_mode,
                thread.interaction_mode,
                model_selection_json,
                archived_at_str
            ],
        );

        Ok(thread)
    }

    pub fn get_thread(&self, id: &str) -> Result<Option<OrchestrationV2ThreadShell>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let query = "SELECT
                t.thread_id,
                t.project_id,
                t.title,
                COALESCE(t.provider_instance_id, t.default_provider) AS provider_instance_id,
                t.runtime_mode,
                t.interaction_mode,
                t.created_at,
                t.updated_at,
                t.archived_at,
                t.deleted_at,
                t.payload_json,
                (SELECT r.run_id FROM orchestration_v2_projection_runs r WHERE r.thread_id = t.thread_id ORDER BY r.ordinal DESC LIMIT 1) AS latest_run_id,
                (SELECT COUNT(*) FROM orchestration_v2_projection_turn_items i WHERE i.thread_id = t.thread_id) AS item_count
            FROM orchestration_v2_projection_threads t
            WHERE t.thread_id = ?1;";

        let result = conn
            .query_row(query, params![id], |row| {
                let thread_id: String = row.get(0)?;
                let project_id: String = row.get(1)?;
                let title: String = row.get(2)?;
                let provider_instance_id: String = row.get(3)?;
                let runtime_mode: String = row.get(4)?;
                let interaction_mode: String = row.get(5)?;
                let created_str: String = row.get(6)?;
                let updated_str: String = row.get(7)?;
                let archived_str: Option<String> = row.get(8)?;
                let deleted_str: Option<String> = row.get(9)?;
                let payload_json: String = row.get(10)?;
                let latest_run_id: Option<String> = row.get(11)?;
                let item_count: i64 = row.get(12)?;
                Ok((
                    thread_id,
                    project_id,
                    title,
                    provider_instance_id,
                    runtime_mode,
                    interaction_mode,
                    created_str,
                    updated_str,
                    archived_str,
                    deleted_str,
                    payload_json,
                    latest_run_id,
                    item_count,
                ))
            })
            .optional()?;

        match result {
            Some((
                thread_id,
                project_id,
                title,
                provider_instance_id,
                runtime_mode,
                interaction_mode,
                created_str,
                updated_str,
                archived_str,
                deleted_str,
                payload_json,
                latest_run_id,
                item_count,
            )) => {
                let payload: serde_json::Value =
                    serde_json::from_str(&payload_json).unwrap_or(serde_json::Value::Null);

                let model_selection = payload
                    .get("modelSelection")
                    .and_then(|v| serde_json::from_value::<ModelSelection>(v.clone()).ok())
                    .unwrap_or_else(|| ModelSelection {
                        instance_id: provider_instance_id.clone(),
                        model: "default".into(),
                        options: None,
                    });

                let branch = payload
                    .get("branch")
                    .and_then(|v| v.as_str())
                    .map(ToString::to_string);
                let worktree_path = payload
                    .get("worktreePath")
                    .and_then(|v| v.as_str())
                    .map(ToString::to_string);
                let status = payload
                    .get("status")
                    .and_then(|v| v.as_str())
                    .map(ToString::to_string);

                let archived_at = parse_datetime_opt(archived_str)?;
                let deleted_at = parse_datetime_opt(deleted_str)?;

                Ok(Some(OrchestrationV2ThreadShell {
                    id: thread_id,
                    project_id,
                    title,
                    provider_instance_id,
                    model_selection,
                    runtime_mode,
                    interaction_mode,
                    branch,
                    worktree_path,
                    created_at: parse_datetime(&created_str)?,
                    updated_at: parse_datetime(&updated_str)?,
                    archived_at,
                    deleted_at,
                    status,
                    latest_run_id,
                    item_count: item_count as usize,
                }))
            }
            None => Ok(None),
        }
    }

    pub fn list_threads(
        &self,
        project_id: &str,
    ) -> Result<Vec<OrchestrationV2ThreadShell>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let query = "SELECT
                t.thread_id,
                t.project_id,
                t.title,
                COALESCE(t.provider_instance_id, t.default_provider) AS provider_instance_id,
                t.runtime_mode,
                t.interaction_mode,
                t.created_at,
                t.updated_at,
                t.archived_at,
                t.deleted_at,
                t.payload_json,
                (SELECT r.run_id FROM orchestration_v2_projection_runs r WHERE r.thread_id = t.thread_id ORDER BY r.ordinal DESC LIMIT 1) AS latest_run_id,
                (SELECT COUNT(*) FROM orchestration_v2_projection_turn_items i WHERE i.thread_id = t.thread_id) AS item_count
            FROM orchestration_v2_projection_threads t
            WHERE t.project_id = ?1 AND t.deleted_at IS NULL
            ORDER BY t.created_at DESC;";

        let mut stmt = conn.prepare(query)?;
        let rows = stmt.query_map(params![project_id], |row| {
            let thread_id: String = row.get(0)?;
            let project_id: String = row.get(1)?;
            let title: String = row.get(2)?;
            let provider_instance_id: String = row.get(3)?;
            let runtime_mode: String = row.get(4)?;
            let interaction_mode: String = row.get(5)?;
            let created_str: String = row.get(6)?;
            let updated_str: String = row.get(7)?;
            let archived_str: Option<String> = row.get(8)?;
            let deleted_str: Option<String> = row.get(9)?;
            let payload_json: String = row.get(10)?;
            let latest_run_id: Option<String> = row.get(11)?;
            let item_count: i64 = row.get(12)?;
            Ok((
                thread_id,
                project_id,
                title,
                provider_instance_id,
                runtime_mode,
                interaction_mode,
                created_str,
                updated_str,
                archived_str,
                deleted_str,
                payload_json,
                latest_run_id,
                item_count,
            ))
        })?;

        let mut threads = Vec::new();
        for row in rows {
            let (
                thread_id,
                project_id,
                title,
                provider_instance_id,
                runtime_mode,
                interaction_mode,
                created_str,
                updated_str,
                archived_str,
                deleted_str,
                payload_json,
                latest_run_id,
                item_count,
            ) = row?;

            let payload: serde_json::Value =
                serde_json::from_str(&payload_json).unwrap_or(serde_json::Value::Null);

            let model_selection = payload
                .get("modelSelection")
                .and_then(|v| serde_json::from_value::<ModelSelection>(v.clone()).ok())
                .unwrap_or_else(|| ModelSelection {
                    instance_id: provider_instance_id.clone(),
                    model: "default".into(),
                    options: None,
                });

            let branch = payload
                .get("branch")
                .and_then(|v| v.as_str())
                .map(ToString::to_string);
            let worktree_path = payload
                .get("worktreePath")
                .and_then(|v| v.as_str())
                .map(ToString::to_string);
            let status = payload
                .get("status")
                .and_then(|v| v.as_str())
                .map(ToString::to_string);

            let archived_at = parse_datetime_opt(archived_str)?;
            let deleted_at = parse_datetime_opt(deleted_str)?;

            threads.push(OrchestrationV2ThreadShell {
                id: thread_id,
                project_id,
                title,
                provider_instance_id,
                model_selection,
                runtime_mode,
                interaction_mode,
                branch,
                worktree_path,
                created_at: parse_datetime(&created_str)?,
                updated_at: parse_datetime(&updated_str)?,
                archived_at,
                deleted_at,
                status,
                latest_run_id,
                item_count: item_count as usize,
            });
        }

        Ok(threads)
    }

    pub fn update_thread(
        &self,
        thread: &OrchestrationV2ThreadShell,
    ) -> Result<OrchestrationV2ThreadShell, StorageError> {
        let conn = self.conn.lock().unwrap();
        let existing_payload: Option<String> = conn
            .query_row(
                "SELECT payload_json FROM orchestration_v2_projection_threads WHERE thread_id = ?1;",
                params![thread.id],
                |r| r.get(0),
            )
            .optional()?;

        let mut payload: serde_json::Value = existing_payload
            .as_deref()
            .and_then(|p| serde_json::from_str(p).ok())
            .unwrap_or_else(|| serde_json::json!({}));

        let updated_at_str = format_datetime(thread.updated_at);
        let archived_at_str = thread.archived_at.map(format_datetime);
        let deleted_at_str = thread.deleted_at.map(format_datetime);

        payload["title"] = serde_json::json!(thread.title);
        payload["providerInstanceId"] = serde_json::json!(thread.provider_instance_id);
        payload["modelSelection"] = serde_json::to_value(&thread.model_selection)?;
        payload["runtimeMode"] = serde_json::json!(thread.runtime_mode);
        payload["interactionMode"] = serde_json::json!(thread.interaction_mode);
        payload["branch"] = serde_json::to_value(&thread.branch)?;
        payload["worktreePath"] = serde_json::to_value(&thread.worktree_path)?;
        payload["status"] = serde_json::to_value(&thread.status)?;
        payload["updatedAt"] = serde_json::json!(updated_at_str);
        payload["archivedAt"] = serde_json::to_value(&archived_at_str)?;
        payload["deletedAt"] = serde_json::to_value(&deleted_at_str)?;

        let payload_json = serde_json::to_string(&payload)?;
        let model_selection_json = serde_json::to_string(&thread.model_selection)?;

        conn.execute(
            "UPDATE orchestration_v2_projection_threads
             SET title = ?2,
                 provider_instance_id = ?3,
                 default_provider = ?3,
                 runtime_mode = ?4,
                 interaction_mode = ?5,
                 updated_at = ?6,
                 archived_at = ?7,
                 deleted_at = ?8,
                 payload_json = ?9
             WHERE thread_id = ?1;",
            params![
                thread.id,
                thread.title,
                thread.provider_instance_id,
                thread.runtime_mode,
                thread.interaction_mode,
                updated_at_str,
                archived_at_str,
                deleted_at_str,
                payload_json
            ],
        )?;

        let _ = conn.execute(
            "UPDATE projection_threads
             SET title = ?2,
                 runtime_mode = ?3,
                 interaction_mode = ?4,
                 branch = ?5,
                 worktree_path = ?6,
                 model_selection_json = ?7,
                 updated_at = ?8,
                 archived_at = ?9,
                 deleted_at = ?10
             WHERE thread_id = ?1;",
            params![
                thread.id,
                thread.title,
                thread.runtime_mode,
                thread.interaction_mode,
                thread.branch,
                thread.worktree_path,
                model_selection_json,
                updated_at_str,
                archived_at_str,
                deleted_at_str
            ],
        );

        drop(conn);
        self.get_thread(&thread.id)?
            .ok_or_else(|| StorageError::NotFound(format!("Thread {} not found", thread.id)))
    }

    pub fn delete_thread(&self, id: &str) -> Result<bool, StorageError> {
        let conn = self.conn.lock().unwrap();
        let existing_payload: Option<String> = conn
            .query_row(
                "SELECT payload_json FROM orchestration_v2_projection_threads WHERE thread_id = ?1 AND deleted_at IS NULL;",
                params![id],
                |r| r.get(0),
            )
            .optional()?;

        let Some(payload_str) = existing_payload else {
            return Ok(false);
        };

        let now_str = format_datetime(Utc::now());
        let mut payload: serde_json::Value =
            serde_json::from_str(&payload_str).unwrap_or(serde_json::Value::Null);
        payload["deletedAt"] = serde_json::json!(now_str);
        payload["updatedAt"] = serde_json::json!(now_str);
        let updated_payload_json = serde_json::to_string(&payload)?;

        conn.execute(
            "UPDATE orchestration_v2_projection_threads
             SET deleted_at = ?2, updated_at = ?2, payload_json = ?3
             WHERE thread_id = ?1;",
            params![id, now_str, updated_payload_json],
        )?;

        let _ = conn.execute(
            "UPDATE projection_threads SET deleted_at = ?2, updated_at = ?2 WHERE thread_id = ?1;",
            params![id, now_str],
        );

        Ok(true)
    }

    pub fn settle_thread(&self, id: &str) -> Result<bool, StorageError> {
        let conn = self.conn.lock().unwrap();
        let existing_payload: Option<String> = conn
            .query_row(
                "SELECT payload_json FROM orchestration_v2_projection_threads WHERE thread_id = ?1 AND deleted_at IS NULL;",
                params![id],
                |r| r.get(0),
            )
            .optional()?;

        let Some(payload_str) = existing_payload else {
            return Ok(false);
        };

        let now_str = format_datetime(Utc::now());
        let mut payload: serde_json::Value =
            serde_json::from_str(&payload_str).unwrap_or(serde_json::Value::Null);
        payload["settledOverride"] = serde_json::json!("settled");
        payload["settledAt"] = serde_json::json!(now_str);
        payload["updatedAt"] = serde_json::json!(now_str);
        let updated_payload_json = serde_json::to_string(&payload)?;

        conn.execute(
            "UPDATE orchestration_v2_projection_threads
             SET updated_at = ?2, payload_json = ?3
             WHERE thread_id = ?1;",
            params![id, now_str, updated_payload_json],
        )?;

        let _ = conn.execute(
            "UPDATE projection_threads
             SET settled_override = 'settled', settled_at = ?2, updated_at = ?2
             WHERE thread_id = ?1;",
            params![id, now_str],
        );

        Ok(true)
    }

    pub fn archive_thread(&self, id: &str) -> Result<bool, StorageError> {
        let conn = self.conn.lock().unwrap();
        let existing_payload: Option<String> = conn
            .query_row(
                "SELECT payload_json FROM orchestration_v2_projection_threads WHERE thread_id = ?1 AND archived_at IS NULL AND deleted_at IS NULL;",
                params![id],
                |r| r.get(0),
            )
            .optional()?;

        let Some(payload_str) = existing_payload else {
            return Ok(false);
        };

        let now_str = format_datetime(Utc::now());
        let mut payload: serde_json::Value =
            serde_json::from_str(&payload_str).unwrap_or(serde_json::Value::Null);
        payload["archivedAt"] = serde_json::json!(now_str);
        payload["updatedAt"] = serde_json::json!(now_str);
        let updated_payload_json = serde_json::to_string(&payload)?;

        conn.execute(
            "UPDATE orchestration_v2_projection_threads
             SET archived_at = ?2, updated_at = ?2, payload_json = ?3
             WHERE thread_id = ?1;",
            params![id, now_str, updated_payload_json],
        )?;

        let _ = conn.execute(
            "UPDATE projection_threads SET archived_at = ?2, updated_at = ?2 WHERE thread_id = ?1;",
            params![id, now_str],
        );

        Ok(true)
    }

    // ==========================================
    // Turn Items & Runs
    // ==========================================

    pub fn create_run(
        &self,
        run: impl Into<OrchestrationV2Run>,
    ) -> Result<OrchestrationV2Run, StorageError> {
        let mut run = run.into();
        let conn = self.conn.lock().unwrap();

        if run.ordinal == 0 {
            let max_ordinal: i64 = conn.query_row(
                "SELECT COALESCE(MAX(ordinal), 0) FROM orchestration_v2_projection_runs WHERE thread_id = ?1;",
                params![run.thread_id],
                |r| r.get(0),
            )?;
            run.ordinal = (max_ordinal + 1) as u64;
        }

        if run.id.is_empty() {
            run.id = format!("run:thread:{}:ordinal:{}", run.thread_id, run.ordinal);
        }

        let requested_at_str = format_datetime(run.requested_at);
        let started_at_str = run.started_at.map(format_datetime);
        let completed_at_str = run.completed_at.map(format_datetime);

        let payload = serde_json::json!({
            "id": run.id,
            "threadId": run.thread_id,
            "ordinal": run.ordinal,
            "providerInstanceId": run.provider_instance_id,
            "modelSelection": run.model_selection,
            "status": run.status,
            "requestedAt": requested_at_str,
            "startedAt": started_at_str,
            "completedAt": completed_at_str,
        });
        let payload_json = serde_json::to_string(&payload)?;

        conn.execute(
            "INSERT INTO orchestration_v2_projection_runs (
                run_id,
                thread_id,
                ordinal,
                provider,
                provider_instance_id,
                provider_thread_id,
                status,
                requested_at,
                completed_at,
                payload_json
            ) VALUES (?1, ?2, ?3, ?4, ?4, NULL, ?5, ?6, ?7, ?8);",
            params![
                run.id,
                run.thread_id,
                run.ordinal as i64,
                run.provider_instance_id,
                run.status,
                requested_at_str,
                completed_at_str,
                payload_json
            ],
        )?;

        Ok(run)
    }

    pub fn update_run_status(&self, run_id: &str, status: &str) -> Result<bool, StorageError> {
        let conn = self.conn.lock().unwrap();
        let existing: Option<(String, Option<String>)> = conn
            .query_row(
                "SELECT payload_json, completed_at FROM orchestration_v2_projection_runs WHERE run_id = ?1;",
                params![run_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;

        let Some((payload_str, existing_completed_at)) = existing else {
            return Ok(false);
        };

        let mut payload: serde_json::Value =
            serde_json::from_str(&payload_str).unwrap_or(serde_json::Value::Null);
        payload["status"] = serde_json::json!(status);

        let now = format_datetime(Utc::now());
        if status == "running" && payload.get("startedAt").is_none_or(|v| v.is_null()) {
            payload["startedAt"] = serde_json::json!(now);
        }

        let completed_at_str = if matches!(status, "completed" | "failed" | "cancelled") {
            existing_completed_at.or_else(|| {
                payload["completedAt"] = serde_json::json!(now);
                Some(now)
            })
        } else {
            existing_completed_at
        };

        let updated_payload_json = serde_json::to_string(&payload)?;

        conn.execute(
            "UPDATE orchestration_v2_projection_runs
             SET status = ?2, completed_at = ?3, payload_json = ?4
             WHERE run_id = ?1;",
            params![run_id, status, completed_at_str, updated_payload_json],
        )?;

        Ok(true)
    }

    pub fn get_run(&self, run_id: &str) -> Result<Option<OrchestrationV2Run>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let query = "SELECT
                run_id,
                thread_id,
                ordinal,
                COALESCE(provider_instance_id, provider) AS provider_instance_id,
                status,
                requested_at,
                completed_at,
                payload_json
            FROM orchestration_v2_projection_runs
            WHERE run_id = ?1;";

        let result = conn
            .query_row(query, params![run_id], |row| {
                let id: String = row.get(0)?;
                let thread_id: String = row.get(1)?;
                let ordinal: i64 = row.get(2)?;
                let provider_instance_id: String = row.get(3)?;
                let status: String = row.get(4)?;
                let requested_at_str: String = row.get(5)?;
                let completed_at_str: Option<String> = row.get(6)?;
                let payload_json: String = row.get(7)?;
                Ok((
                    id,
                    thread_id,
                    ordinal,
                    provider_instance_id,
                    status,
                    requested_at_str,
                    completed_at_str,
                    payload_json,
                ))
            })
            .optional()?;

        match result {
            Some((
                id,
                thread_id,
                ordinal,
                provider_instance_id,
                status,
                requested_at_str,
                completed_at_str,
                payload_json,
            )) => {
                let payload: serde_json::Value =
                    serde_json::from_str(&payload_json).unwrap_or(serde_json::Value::Null);

                let model_selection = payload
                    .get("modelSelection")
                    .and_then(|v| serde_json::from_value::<ModelSelection>(v.clone()).ok())
                    .unwrap_or_else(|| ModelSelection {
                        instance_id: provider_instance_id.clone(),
                        model: "default".into(),
                        options: None,
                    });

                let started_at = payload
                    .get("startedAt")
                    .and_then(|v| v.as_str())
                    .and_then(|s| parse_datetime(s).ok());

                let completed_at = completed_at_str
                    .and_then(|s| parse_datetime(&s).ok())
                    .or_else(|| {
                        payload
                            .get("completedAt")
                            .and_then(|v| v.as_str())
                            .and_then(|s| parse_datetime(s).ok())
                    });

                Ok(Some(OrchestrationV2Run {
                    id,
                    thread_id,
                    ordinal: ordinal as u64,
                    provider_instance_id,
                    model_selection,
                    status,
                    requested_at: parse_datetime(&requested_at_str)?,
                    started_at,
                    completed_at,
                }))
            }
            None => Ok(None),
        }
    }

    pub fn list_runs(&self, thread_id: &str) -> Result<Vec<OrchestrationV2Run>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let query = "SELECT
                run_id,
                thread_id,
                ordinal,
                COALESCE(provider_instance_id, provider) AS provider_instance_id,
                status,
                requested_at,
                completed_at,
                payload_json
            FROM orchestration_v2_projection_runs
            WHERE thread_id = ?1
            ORDER BY ordinal ASC, run_id ASC;";

        let mut stmt = conn.prepare(query)?;
        let rows = stmt.query_map(params![thread_id], |row| {
            let id: String = row.get(0)?;
            let thread_id: String = row.get(1)?;
            let ordinal: i64 = row.get(2)?;
            let provider_instance_id: String = row.get(3)?;
            let status: String = row.get(4)?;
            let requested_at_str: String = row.get(5)?;
            let completed_at_str: Option<String> = row.get(6)?;
            let payload_json: String = row.get(7)?;
            Ok((
                id,
                thread_id,
                ordinal,
                provider_instance_id,
                status,
                requested_at_str,
                completed_at_str,
                payload_json,
            ))
        })?;

        let mut runs = Vec::new();
        for row in rows {
            let (
                id,
                thread_id,
                ordinal,
                provider_instance_id,
                status,
                requested_at_str,
                completed_at_str,
                payload_json,
            ) = row?;

            let payload: serde_json::Value =
                serde_json::from_str(&payload_json).unwrap_or(serde_json::Value::Null);

            let model_selection = payload
                .get("modelSelection")
                .and_then(|v| serde_json::from_value::<ModelSelection>(v.clone()).ok())
                .unwrap_or_else(|| ModelSelection {
                    instance_id: provider_instance_id.clone(),
                    model: "default".into(),
                    options: None,
                });

            let started_at = payload
                .get("startedAt")
                .and_then(|v| v.as_str())
                .and_then(|s| parse_datetime(s).ok());

            let completed_at = completed_at_str
                .and_then(|s| parse_datetime(&s).ok())
                .or_else(|| {
                    payload
                        .get("completedAt")
                        .and_then(|v| v.as_str())
                        .and_then(|s| parse_datetime(s).ok())
                });

            runs.push(OrchestrationV2Run {
                id,
                thread_id,
                ordinal: ordinal as u64,
                provider_instance_id,
                model_selection,
                status,
                requested_at: parse_datetime(&requested_at_str)?,
                started_at,
                completed_at,
            });
        }

        Ok(runs)
    }

    pub fn add_turn_item(
        &self,
        item: impl Into<OrchestrationV2TurnItem>,
    ) -> Result<OrchestrationV2TurnItem, StorageError> {
        let mut item = item.into();
        let conn = self.conn.lock().unwrap();

        if item.ordinal == 0 {
            let max_ordinal: i64 = conn.query_row(
                "SELECT COALESCE(MAX(ordinal), 0) FROM orchestration_v2_projection_turn_items WHERE thread_id = ?1;",
                params![item.thread_id],
                |r| r.get(0),
            )?;
            item.ordinal = (max_ordinal + 1) as u64;
        }

        if item.id.is_empty() {
            item.id = format!(
                "turn-item:thread:{}:{}",
                item.thread_id,
                uuid::Uuid::new_v4()
            );
        }

        let created_at_str = item.created_at.map(format_datetime);
        let updated_at_str = format_datetime(item.updated_at);

        let payload = serde_json::json!({
            "id": item.id,
            "threadId": item.thread_id,
            "runId": item.run_id,
            "ordinal": item.ordinal,
            "status": item.status,
            "type": item.item_type,
            "title": item.title,
            "text": item.text,
            "input": item.input,
            "output": item.output,
            "exitCode": item.exit_code,
            "createdAt": created_at_str,
            "updatedAt": updated_at_str,
        });
        let payload_json = serde_json::to_string(&payload)?;

        conn.execute(
            "INSERT INTO orchestration_v2_projection_turn_items (
                turn_item_id,
                thread_id,
                run_id,
                node_id,
                provider_thread_id,
                provider_turn_id,
                parent_item_id,
                ordinal,
                type,
                status,
                updated_at,
                payload_json
            ) VALUES (?1, ?2, ?3, NULL, NULL, NULL, NULL, ?4, ?5, ?6, ?7, ?8);",
            params![
                item.id,
                item.thread_id,
                item.run_id,
                item.ordinal as i64,
                item.item_type,
                item.status,
                updated_at_str,
                payload_json
            ],
        )?;

        let _ = conn.execute(
            "INSERT OR REPLACE INTO orchestration_v2_turn_item_positions (
                thread_id, turn_item_id, ordinal
            ) VALUES (?1, ?2, ?3);",
            params![item.thread_id, item.id, item.ordinal as i64],
        );

        let _ = conn.execute(
            "UPDATE orchestration_v2_projection_threads SET updated_at = ?2 WHERE thread_id = ?1;",
            params![item.thread_id, updated_at_str],
        );

        Ok(item)
    }

    pub fn update_turn_item(
        &self,
        item: impl Into<OrchestrationV2TurnItem>,
    ) -> Result<OrchestrationV2TurnItem, StorageError> {
        let mut item = item.into();
        item.updated_at = Utc::now();
        let conn = self.conn.lock().unwrap();

        let existing_payload: Option<String> = conn
            .query_row(
                "SELECT payload_json FROM orchestration_v2_projection_turn_items WHERE turn_item_id = ?1;",
                params![item.id],
                |r| r.get(0),
            )
            .optional()?;

        let Some(payload_str) = existing_payload else {
            return Err(StorageError::NotFound(format!(
                "Turn item {} not found",
                item.id
            )));
        };

        let mut payload: serde_json::Value =
            serde_json::from_str(&payload_str).unwrap_or_else(|_| serde_json::json!({}));

        let updated_at_str = format_datetime(item.updated_at);

        payload["status"] = serde_json::json!(item.status);
        payload["title"] = serde_json::to_value(&item.title)?;
        payload["text"] = serde_json::to_value(&item.text)?;
        payload["input"] = serde_json::to_value(&item.input)?;
        payload["output"] = serde_json::to_value(&item.output)?;
        payload["exitCode"] = serde_json::to_value(item.exit_code)?;
        payload["updatedAt"] = serde_json::json!(updated_at_str);

        let payload_json = serde_json::to_string(&payload)?;

        conn.execute(
            "UPDATE orchestration_v2_projection_turn_items
             SET status = ?2, updated_at = ?3, payload_json = ?4
             WHERE turn_item_id = ?1;",
            params![item.id, item.status, updated_at_str, payload_json],
        )?;

        Ok(item)
    }

    pub fn get_turn_item(
        &self,
        turn_item_id: &str,
    ) -> Result<Option<OrchestrationV2TurnItem>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let query = "SELECT
                turn_item_id,
                thread_id,
                run_id,
                ordinal,
                type,
                status,
                updated_at,
                payload_json
            FROM orchestration_v2_projection_turn_items
            WHERE turn_item_id = ?1;";

        let result = conn
            .query_row(query, params![turn_item_id], |row| {
                let id: String = row.get(0)?;
                let thread_id: String = row.get(1)?;
                let run_id: Option<String> = row.get(2)?;
                let ordinal: i64 = row.get(3)?;
                let item_type: String = row.get(4)?;
                let status: String = row.get(5)?;
                let updated_at_str: String = row.get(6)?;
                let payload_json: String = row.get(7)?;
                Ok((
                    id,
                    thread_id,
                    run_id,
                    ordinal,
                    item_type,
                    status,
                    updated_at_str,
                    payload_json,
                ))
            })
            .optional()?;

        match result {
            Some((
                id,
                thread_id,
                run_id,
                ordinal,
                item_type,
                status,
                updated_at_str,
                payload_json,
            )) => {
                let payload: serde_json::Value =
                    serde_json::from_str(&payload_json).unwrap_or(serde_json::Value::Null);

                let title = payload
                    .get("title")
                    .and_then(|v| v.as_str())
                    .map(ToString::to_string);
                let text = payload
                    .get("text")
                    .and_then(|v| v.as_str())
                    .map(ToString::to_string);
                let input = extract_json_or_string(payload.get("input"));
                let output = extract_json_or_string(payload.get("output"));
                let exit_code = payload
                    .get("exitCode")
                    .and_then(|v| v.as_i64())
                    .map(|c| c as i32);
                let created_at = payload
                    .get("createdAt")
                    .or_else(|| payload.get("startedAt"))
                    .and_then(|v| v.as_str())
                    .and_then(|s| parse_datetime(s).ok());

                Ok(Some(OrchestrationV2TurnItem {
                    id,
                    thread_id,
                    run_id,
                    ordinal: ordinal as u64,
                    status,
                    item_type,
                    title,
                    text,
                    input,
                    output,
                    exit_code,
                    created_at,
                    updated_at: parse_datetime(&updated_at_str)?,
                }))
            }
            None => Ok(None),
        }
    }

    pub fn list_turn_items(
        &self,
        thread_id: &str,
    ) -> Result<Vec<OrchestrationV2TurnItem>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let query = "SELECT
                turn_item_id,
                thread_id,
                run_id,
                ordinal,
                type,
                status,
                updated_at,
                payload_json
            FROM orchestration_v2_projection_turn_items
            WHERE thread_id = ?1
            ORDER BY ordinal ASC, turn_item_id ASC;";

        let mut stmt = conn.prepare(query)?;
        let rows = stmt.query_map(params![thread_id], |row| {
            let id: String = row.get(0)?;
            let thread_id: String = row.get(1)?;
            let run_id: Option<String> = row.get(2)?;
            let ordinal: i64 = row.get(3)?;
            let item_type: String = row.get(4)?;
            let status: String = row.get(5)?;
            let updated_at_str: String = row.get(6)?;
            let payload_json: String = row.get(7)?;
            Ok((
                id,
                thread_id,
                run_id,
                ordinal,
                item_type,
                status,
                updated_at_str,
                payload_json,
            ))
        })?;

        let mut items = Vec::new();
        for row in rows {
            let (id, thread_id, run_id, ordinal, item_type, status, updated_at_str, payload_json) =
                row?;

            let payload: serde_json::Value =
                serde_json::from_str(&payload_json).unwrap_or(serde_json::Value::Null);

            let title = payload
                .get("title")
                .and_then(|v| v.as_str())
                .map(ToString::to_string);
            let text = payload
                .get("text")
                .and_then(|v| v.as_str())
                .map(ToString::to_string);
            let input = extract_json_or_string(payload.get("input"));
            let output = extract_json_or_string(payload.get("output"));
            let exit_code = payload
                .get("exitCode")
                .and_then(|v| v.as_i64())
                .map(|c| c as i32);
            let created_at = payload
                .get("createdAt")
                .or_else(|| payload.get("startedAt"))
                .and_then(|v| v.as_str())
                .and_then(|s| parse_datetime(s).ok());

            items.push(OrchestrationV2TurnItem {
                id,
                thread_id,
                run_id,
                ordinal: ordinal as u64,
                status,
                item_type,
                title,
                text,
                input,
                output,
                exit_code,
                created_at,
                updated_at: parse_datetime(&updated_at_str)?,
            });
        }

        Ok(items)
    }

    pub fn get_thread_projection(
        &self,
        thread_id: &str,
    ) -> Result<Option<OrchestrationV2ThreadProjection>, StorageError> {
        let thread = match self.get_thread(thread_id)? {
            Some(t) => t,
            None => return Ok(None),
        };

        let runs = self.list_runs(thread_id)?;
        let turn_items = self.list_turn_items(thread_id)?;

        Ok(Some(OrchestrationV2ThreadProjection {
            thread,
            runs,
            turn_items,
        }))
    }

    // ==========================================
    // Messages CRUD
    // ==========================================

    pub fn add_message(
        &self,
        message: impl Into<OrchestrationMessage>,
    ) -> Result<OrchestrationMessage, StorageError> {
        let mut message = message.into();
        let conn = self.conn.lock().unwrap();

        if message.id.is_empty() {
            message.id = uuid::Uuid::new_v4().to_string();
        }

        let created_at_str = format_datetime(message.created_at);
        let updated_at_str = format_datetime(message.updated_at);

        let payload = serde_json::json!({
            "id": message.id,
            "threadId": message.thread_id,
            "runId": message.run_id,
            "nodeId": message.node_id,
            "role": message.role,
            "text": message.text,
            "streaming": message.streaming,
            "createdAt": created_at_str,
            "updatedAt": updated_at_str,
        });
        let payload_json = serde_json::to_string(&payload)?;

        conn.execute(
            "INSERT INTO orchestration_v2_projection_messages (
                message_id,
                thread_id,
                run_id,
                node_id,
                role,
                streaming,
                created_at,
                updated_at,
                payload_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9);",
            params![
                message.id,
                message.thread_id,
                message.run_id,
                message.node_id,
                message.role,
                if message.streaming { 1 } else { 0 },
                created_at_str,
                updated_at_str,
                payload_json
            ],
        )?;

        Ok(message)
    }

    pub fn get_message(
        &self,
        message_id: &str,
    ) -> Result<Option<OrchestrationMessage>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let query = "SELECT
                message_id,
                thread_id,
                run_id,
                node_id,
                role,
                streaming,
                created_at,
                updated_at,
                payload_json
            FROM orchestration_v2_projection_messages
            WHERE message_id = ?1;";

        let result = conn
            .query_row(query, params![message_id], |row| {
                let id: String = row.get(0)?;
                let thread_id: String = row.get(1)?;
                let run_id: Option<String> = row.get(2)?;
                let node_id: Option<String> = row.get(3)?;
                let role: String = row.get(4)?;
                let streaming: i64 = row.get(5)?;
                let created_str: String = row.get(6)?;
                let updated_str: String = row.get(7)?;
                let payload_json: String = row.get(8)?;
                Ok((
                    id,
                    thread_id,
                    run_id,
                    node_id,
                    role,
                    streaming != 0,
                    created_str,
                    updated_str,
                    payload_json,
                ))
            })
            .optional()?;

        match result {
            Some((
                id,
                thread_id,
                run_id,
                node_id,
                role,
                streaming,
                created_str,
                updated_str,
                payload_json,
            )) => {
                let payload: serde_json::Value =
                    serde_json::from_str(&payload_json).unwrap_or(serde_json::Value::Null);
                let text = payload
                    .get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                Ok(Some(OrchestrationMessage {
                    id,
                    thread_id,
                    run_id,
                    node_id,
                    role,
                    text,
                    streaming,
                    created_at: parse_datetime(&created_str)?,
                    updated_at: parse_datetime(&updated_str)?,
                }))
            }
            None => Ok(None),
        }
    }

    pub fn list_messages(
        &self,
        thread_id: &str,
    ) -> Result<Vec<OrchestrationMessage>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let query = "SELECT
                message_id,
                thread_id,
                run_id,
                node_id,
                role,
                streaming,
                created_at,
                updated_at,
                payload_json
            FROM orchestration_v2_projection_messages
            WHERE thread_id = ?1
            ORDER BY created_at ASC, message_id ASC;";

        let mut stmt = conn.prepare(query)?;
        let rows = stmt.query_map(params![thread_id], |row| {
            let id: String = row.get(0)?;
            let thread_id: String = row.get(1)?;
            let run_id: Option<String> = row.get(2)?;
            let node_id: Option<String> = row.get(3)?;
            let role: String = row.get(4)?;
            let streaming: i64 = row.get(5)?;
            let created_str: String = row.get(6)?;
            let updated_str: String = row.get(7)?;
            let payload_json: String = row.get(8)?;
            Ok((
                id,
                thread_id,
                run_id,
                node_id,
                role,
                streaming != 0,
                created_str,
                updated_str,
                payload_json,
            ))
        })?;

        let mut messages = Vec::new();
        for row in rows {
            let (
                id,
                thread_id,
                run_id,
                node_id,
                role,
                streaming,
                created_str,
                updated_str,
                payload_json,
            ) = row?;

            let payload: serde_json::Value =
                serde_json::from_str(&payload_json).unwrap_or(serde_json::Value::Null);
            let text = payload
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            messages.push(OrchestrationMessage {
                id,
                thread_id,
                run_id,
                node_id,
                role,
                text,
                streaming,
                created_at: parse_datetime(&created_str)?,
                updated_at: parse_datetime(&updated_str)?,
            });
        }

        Ok(messages)
    }

    pub fn update_message(
        &self,
        message: impl Into<OrchestrationMessage>,
    ) -> Result<OrchestrationMessage, StorageError> {
        let mut message = message.into();
        message.updated_at = Utc::now();
        let conn = self.conn.lock().unwrap();

        let existing_payload: Option<String> = conn
            .query_row(
                "SELECT payload_json FROM orchestration_v2_projection_messages WHERE message_id = ?1;",
                params![message.id],
                |r| r.get(0),
            )
            .optional()?;

        let Some(payload_str) = existing_payload else {
            return Err(StorageError::NotFound(format!(
                "Message {} not found",
                message.id
            )));
        };

        let mut payload: serde_json::Value =
            serde_json::from_str(&payload_str).unwrap_or_else(|_| serde_json::json!({}));

        let updated_at_str = format_datetime(message.updated_at);
        payload["role"] = serde_json::json!(message.role);
        payload["text"] = serde_json::json!(message.text);
        payload["streaming"] = serde_json::json!(message.streaming);
        payload["updatedAt"] = serde_json::json!(updated_at_str);

        let payload_json = serde_json::to_string(&payload)?;

        conn.execute(
            "UPDATE orchestration_v2_projection_messages
             SET role = ?2, streaming = ?3, updated_at = ?4, payload_json = ?5
             WHERE message_id = ?1;",
            params![
                message.id,
                message.role,
                if message.streaming { 1 } else { 0 },
                updated_at_str,
                payload_json
            ],
        )?;

        Ok(message)
    }

    pub fn delete_message(&self, message_id: &str) -> Result<bool, StorageError> {
        let conn = self.conn.lock().unwrap();
        let count = conn.execute(
            "DELETE FROM orchestration_v2_projection_messages WHERE message_id = ?1;",
            params![message_id],
        )?;
        Ok(count > 0)
    }

    // ==========================================
    // Auth & Pairing
    // ==========================================

    pub fn create_pairing_link<S: AsRef<str>>(
        &self,
        token: &str,
        scopes: &[S],
        expires_at: chrono::DateTime<Utc>,
    ) -> Result<PairingLink, StorageError> {
        let conn = self.conn.lock().unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now();
        let created_at_str = format_datetime(now);
        let expires_at_str = format_datetime(expires_at);

        let scopes_vec: Vec<String> = scopes.iter().map(|s| s.as_ref().to_string()).collect();
        let scopes_json = serde_json::to_string(&scopes_vec)?;

        conn.execute(
            "INSERT INTO auth_pairing_links (
                id,
                credential,
                method,
                scopes,
                subject,
                label,
                created_at,
                expires_at,
                consumed_at,
                revoked_at
            ) VALUES (?1, ?2, 'one-time-token', ?3, 'administrative-bootstrap', NULL, ?4, ?5, NULL, NULL);",
            params![id, token, scopes_json, created_at_str, expires_at_str],
        )?;

        Ok(PairingLink {
            id,
            credential: token.to_string(),
            method: "one-time-token".to_string(),
            scopes: scopes_vec,
            subject: "administrative-bootstrap".to_string(),
            label: None,
            created_at: now,
            expires_at,
            consumed_at: None,
            revoked_at: None,
        })
    }

    pub fn consume_pairing_link(&self, token: &str) -> Result<Option<PairingLink>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now();
        let now_str = format_datetime(now);

        let row = conn
            .query_row(
                "SELECT id, credential, method, scopes, subject, label, created_at, expires_at, revoked_at
                 FROM auth_pairing_links
                 WHERE credential = ?1 AND consumed_at IS NULL AND revoked_at IS NULL AND expires_at > ?2;",
                params![token, now_str],
                |r| {
                    let id: String = r.get(0)?;
                    let credential: String = r.get(1)?;
                    let method: String = r.get(2)?;
                    let scopes_json: String = r.get(3)?;
                    let subject: String = r.get(4)?;
                    let label: Option<String> = r.get(5)?;
                    let created_str: String = r.get(6)?;
                    let expires_str: String = r.get(7)?;
                    let revoked_str: Option<String> = r.get(8)?;
                    Ok((
                        id,
                        credential,
                        method,
                        scopes_json,
                        subject,
                        label,
                        created_str,
                        expires_str,
                        revoked_str,
                    ))
                },
            )
            .optional()?;

        match row {
            Some((
                id,
                credential,
                method,
                scopes_json,
                subject,
                label,
                created_str,
                expires_str,
                revoked_str,
            )) => {
                let count = conn.execute(
                    "UPDATE auth_pairing_links SET consumed_at = ?2 WHERE id = ?1 AND consumed_at IS NULL;",
                    params![id, now_str],
                )?;

                if count == 0 {
                    return Ok(None);
                }

                let scopes = serde_json::from_str(&scopes_json).unwrap_or_default();
                Ok(Some(PairingLink {
                    id,
                    credential,
                    method,
                    scopes,
                    subject,
                    label,
                    created_at: parse_datetime(&created_str)?,
                    expires_at: parse_datetime(&expires_str)?,
                    consumed_at: Some(now),
                    revoked_at: parse_datetime_opt(revoked_str)?,
                }))
            }
            None => Ok(None),
        }
    }

    pub fn get_pairing_link(&self, token: &str) -> Result<Option<PairingLink>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let row = conn
            .query_row(
                "SELECT id, credential, method, scopes, subject, label, created_at, expires_at, consumed_at, revoked_at
                 FROM auth_pairing_links
                 WHERE credential = ?1;",
                params![token],
                |r| {
                    let id: String = r.get(0)?;
                    let credential: String = r.get(1)?;
                    let method: String = r.get(2)?;
                    let scopes_json: String = r.get(3)?;
                    let subject: String = r.get(4)?;
                    let label: Option<String> = r.get(5)?;
                    let created_str: String = r.get(6)?;
                    let expires_str: String = r.get(7)?;
                    let consumed_str: Option<String> = r.get(8)?;
                    let revoked_str: Option<String> = r.get(9)?;
                    Ok((
                        id,
                        credential,
                        method,
                        scopes_json,
                        subject,
                        label,
                        created_str,
                        expires_str,
                        consumed_str,
                        revoked_str,
                    ))
                },
            )
            .optional()?;

        match row {
            Some((
                id,
                credential,
                method,
                scopes_json,
                subject,
                label,
                created_str,
                expires_str,
                consumed_str,
                revoked_str,
            )) => {
                let scopes = serde_json::from_str(&scopes_json).unwrap_or_default();
                Ok(Some(PairingLink {
                    id,
                    credential,
                    method,
                    scopes,
                    subject,
                    label,
                    created_at: parse_datetime(&created_str)?,
                    expires_at: parse_datetime(&expires_str)?,
                    consumed_at: parse_datetime_opt(consumed_str)?,
                    revoked_at: parse_datetime_opt(revoked_str)?,
                }))
            }
            None => Ok(None),
        }
    }

    pub fn create_session(
        &self,
        session_id: &str,
        token: &str,
        expires_at: chrono::DateTime<Utc>,
    ) -> Result<AuthSession, StorageError> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now();
        let issued_at_str = format_datetime(now);
        let expires_at_str = format_datetime(expires_at);

        let scopes = vec![
            "orchestration:read".to_string(),
            "orchestration:operate".to_string(),
        ];
        let scopes_json = serde_json::to_string(&scopes)?;

        conn.execute(
            "INSERT INTO auth_sessions (
                session_id,
                subject,
                scopes,
                method,
                client_label,
                client_ip_address,
                client_user_agent,
                client_device_type,
                client_os,
                client_browser,
                issued_at,
                expires_at,
                last_connected_at,
                revoked_at
            ) VALUES (?1, 'user', ?2, 'bearer-access-token', ?3, NULL, NULL, 'unknown', NULL, NULL, ?4, ?5, NULL, NULL)
            ON CONFLICT(session_id) DO UPDATE SET
                client_label = excluded.client_label,
                expires_at = excluded.expires_at,
                revoked_at = NULL;",
            params![session_id, scopes_json, token, issued_at_str, expires_at_str],
        )?;

        let _ = conn.execute(
            "INSERT OR REPLACE INTO auth_tokens (token, session_id, expires_at)
             VALUES (?1, ?2, ?3);",
            params![token, session_id, expires_at_str],
        );

        Ok(AuthSession {
            session_id: session_id.to_string(),
            subject: "user".to_string(),
            scopes,
            method: "bearer-access-token".to_string(),
            client_label: Some(token.to_string()),
            client_ip_address: None,
            client_user_agent: None,
            client_device_type: "unknown".to_string(),
            client_os: None,
            client_browser: None,
            issued_at: now,
            expires_at,
            last_connected_at: None,
            revoked_at: None,
        })
    }

    pub fn validate_session(&self, token: &str) -> Result<Option<AuthSession>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now();
        let now_str = format_datetime(now);

        // 1. Check auth_tokens
        let token_session_id: Option<String> = conn
            .query_row(
                "SELECT session_id FROM auth_tokens WHERE token = ?1 AND expires_at > ?2;",
                params![token, now_str],
                |r| r.get(0),
            )
            .optional()?;

        // 2. Check directly in auth_sessions by session_id or client_label
        let direct_session_id: Option<String> = if token_session_id.is_none() {
            conn.query_row(
                "SELECT session_id FROM auth_sessions
                 WHERE (session_id = ?1 OR client_label = ?1) AND revoked_at IS NULL AND expires_at > ?2;",
                params![token, now_str],
                |r| r.get(0),
            )
            .optional()?
        } else {
            None
        };

        // 3. Check if token contains base64 url claims
        let jwt_session_id: Option<String> =
            if token_session_id.is_none() && direct_session_id.is_none() {
                extract_sid_from_token(token)
            } else {
                None
            };

        let target_sid = token_session_id.or(direct_session_id).or(jwt_session_id);

        let Some(target_session_id) = target_sid else {
            return Ok(None);
        };

        let row = conn
            .query_row(
                "SELECT
                    session_id,
                    subject,
                    scopes,
                    method,
                    client_label,
                    client_ip_address,
                    client_user_agent,
                    client_device_type,
                    client_os,
                    client_browser,
                    issued_at,
                    expires_at,
                    last_connected_at,
                    revoked_at
                FROM auth_sessions
                WHERE session_id = ?1 AND revoked_at IS NULL AND expires_at > ?2;",
                params![target_session_id, now_str],
                |r| {
                    let session_id: String = r.get(0)?;
                    let subject: String = r.get(1)?;
                    let scopes_json: String = r.get(2)?;
                    let method: String = r.get(3)?;
                    let client_label: Option<String> = r.get(4)?;
                    let client_ip_address: Option<String> = r.get(5)?;
                    let client_user_agent: Option<String> = r.get(6)?;
                    let client_device_type: String = r.get(7)?;
                    let client_os: Option<String> = r.get(8)?;
                    let client_browser: Option<String> = r.get(9)?;
                    let issued_str: String = r.get(10)?;
                    let expires_str: String = r.get(11)?;
                    let last_connected_str: Option<String> = r.get(12)?;
                    let revoked_str: Option<String> = r.get(13)?;
                    Ok((
                        session_id,
                        subject,
                        scopes_json,
                        method,
                        client_label,
                        client_ip_address,
                        client_user_agent,
                        client_device_type,
                        client_os,
                        client_browser,
                        issued_str,
                        expires_str,
                        last_connected_str,
                        revoked_str,
                    ))
                },
            )
            .optional()?;

        match row {
            Some((
                session_id,
                subject,
                scopes_json,
                method,
                client_label,
                client_ip_address,
                client_user_agent,
                client_device_type,
                client_os,
                client_browser,
                issued_str,
                expires_str,
                _last_connected_str,
                revoked_str,
            )) => {
                let _ = conn.execute(
                    "UPDATE auth_sessions SET last_connected_at = ?2 WHERE session_id = ?1;",
                    params![session_id, now_str],
                );

                let scopes = serde_json::from_str(&scopes_json).unwrap_or_default();
                Ok(Some(AuthSession {
                    session_id,
                    subject,
                    scopes,
                    method,
                    client_label,
                    client_ip_address,
                    client_user_agent,
                    client_device_type,
                    client_os,
                    client_browser,
                    issued_at: parse_datetime(&issued_str)?,
                    expires_at: parse_datetime(&expires_str)?,
                    last_connected_at: Some(now),
                    revoked_at: parse_datetime_opt(revoked_str)?,
                }))
            }
            None => Ok(None),
        }
    }

    pub fn get_session(&self, session_id: &str) -> Result<Option<AuthSession>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let row = conn
            .query_row(
                "SELECT
                    session_id,
                    subject,
                    scopes,
                    method,
                    client_label,
                    client_ip_address,
                    client_user_agent,
                    client_device_type,
                    client_os,
                    client_browser,
                    issued_at,
                    expires_at,
                    last_connected_at,
                    revoked_at
                FROM auth_sessions
                WHERE session_id = ?1;",
                params![session_id],
                |r| {
                    let session_id: String = r.get(0)?;
                    let subject: String = r.get(1)?;
                    let scopes_json: String = r.get(2)?;
                    let method: String = r.get(3)?;
                    let client_label: Option<String> = r.get(4)?;
                    let client_ip_address: Option<String> = r.get(5)?;
                    let client_user_agent: Option<String> = r.get(6)?;
                    let client_device_type: String = r.get(7)?;
                    let client_os: Option<String> = r.get(8)?;
                    let client_browser: Option<String> = r.get(9)?;
                    let issued_str: String = r.get(10)?;
                    let expires_str: String = r.get(11)?;
                    let last_connected_str: Option<String> = r.get(12)?;
                    let revoked_str: Option<String> = r.get(13)?;
                    Ok((
                        session_id,
                        subject,
                        scopes_json,
                        method,
                        client_label,
                        client_ip_address,
                        client_user_agent,
                        client_device_type,
                        client_os,
                        client_browser,
                        issued_str,
                        expires_str,
                        last_connected_str,
                        revoked_str,
                    ))
                },
            )
            .optional()?;

        match row {
            Some((
                session_id,
                subject,
                scopes_json,
                method,
                client_label,
                client_ip_address,
                client_user_agent,
                client_device_type,
                client_os,
                client_browser,
                issued_str,
                expires_str,
                last_connected_str,
                revoked_str,
            )) => {
                let scopes = serde_json::from_str(&scopes_json).unwrap_or_default();
                Ok(Some(AuthSession {
                    session_id,
                    subject,
                    scopes,
                    method,
                    client_label,
                    client_ip_address,
                    client_user_agent,
                    client_device_type,
                    client_os,
                    client_browser,
                    issued_at: parse_datetime(&issued_str)?,
                    expires_at: parse_datetime(&expires_str)?,
                    last_connected_at: parse_datetime_opt(last_connected_str)?,
                    revoked_at: parse_datetime_opt(revoked_str)?,
                }))
            }
            None => Ok(None),
        }
    }

    pub fn revoke_session(&self, session_id: &str) -> Result<bool, StorageError> {
        let conn = self.conn.lock().unwrap();
        let now_str = format_datetime(Utc::now());
        let count = conn.execute(
            "UPDATE auth_sessions SET revoked_at = ?2 WHERE session_id = ?1 AND revoked_at IS NULL;",
            params![session_id, now_str],
        )?;
        let _ = conn.execute(
            "DELETE FROM auth_tokens WHERE session_id = ?1;",
            params![session_id],
        );
        Ok(count > 0)
    }

    pub fn list_active_sessions(&self) -> Result<Vec<AuthSession>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let now_str = format_datetime(Utc::now());
        let mut stmt = conn.prepare(
            "SELECT
                session_id,
                subject,
                scopes,
                method,
                client_label,
                client_ip_address,
                client_user_agent,
                client_device_type,
                client_os,
                client_browser,
                issued_at,
                expires_at,
                last_connected_at,
                revoked_at
            FROM auth_sessions
            WHERE revoked_at IS NULL AND expires_at > ?1
            ORDER BY issued_at DESC;",
        )?;

        let rows = stmt.query_map(params![now_str], |r| {
            let session_id: String = r.get(0)?;
            let subject: String = r.get(1)?;
            let scopes_json: String = r.get(2)?;
            let method: String = r.get(3)?;
            let client_label: Option<String> = r.get(4)?;
            let client_ip_address: Option<String> = r.get(5)?;
            let client_user_agent: Option<String> = r.get(6)?;
            let client_device_type: String = r.get(7)?;
            let client_os: Option<String> = r.get(8)?;
            let client_browser: Option<String> = r.get(9)?;
            let issued_str: String = r.get(10)?;
            let expires_str: String = r.get(11)?;
            let last_connected_str: Option<String> = r.get(12)?;
            let revoked_str: Option<String> = r.get(13)?;
            Ok((
                session_id,
                subject,
                scopes_json,
                method,
                client_label,
                client_ip_address,
                client_user_agent,
                client_device_type,
                client_os,
                client_browser,
                issued_str,
                expires_str,
                last_connected_str,
                revoked_str,
            ))
        })?;

        let mut sessions = Vec::new();
        for row in rows {
            let (
                session_id,
                subject,
                scopes_json,
                method,
                client_label,
                client_ip_address,
                client_user_agent,
                client_device_type,
                client_os,
                client_browser,
                issued_str,
                expires_str,
                last_connected_str,
                revoked_str,
            ) = row?;

            let scopes = serde_json::from_str(&scopes_json).unwrap_or_default();
            sessions.push(AuthSession {
                session_id,
                subject,
                scopes,
                method,
                client_label,
                client_ip_address,
                client_user_agent,
                client_device_type,
                client_os,
                client_browser,
                issued_at: parse_datetime(&issued_str)?,
                expires_at: parse_datetime(&expires_str)?,
                last_connected_at: parse_datetime_opt(last_connected_str)?,
                revoked_at: parse_datetime_opt(revoked_str)?,
            });
        }

        Ok(sessions)
    }
}

fn extract_sid_from_token(token: &str) -> Option<String> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() >= 2 {
        for part in &parts[..2] {
            if let Some(json_str) = decode_base64_url_to_string(part) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&json_str) {
                    if let Some(sid) = val.get("sid").and_then(|v| v.as_str()) {
                        return Some(sid.to_string());
                    }
                    if let Some(sid) = val.get("sessionId").and_then(|v| v.as_str()) {
                        return Some(sid.to_string());
                    }
                }
            }
        }
    }
    None
}

fn decode_base64_url_to_string(input: &str) -> Option<String> {
    let mut normalized = input.replace('-', "+").replace('_', "/");
    while !normalized.len().is_multiple_of(4) {
        normalized.push('=');
    }
    let chars: Vec<char> = normalized.chars().collect();
    let mut bytes = Vec::new();
    for chunk in chars.chunks(4) {
        if chunk.len() < 4 {
            break;
        }
        let decode_char = |c: char| -> Option<u8> {
            match c {
                'A'..='Z' => Some(c as u8 - b'A'),
                'a'..='z' => Some(c as u8 - b'a' + 26),
                '0'..='9' => Some(c as u8 - b'0' + 52),
                '+' => Some(62),
                '/' => Some(63),
                '=' => Some(0),
                _ => None,
            }
        };
        let b0 = decode_char(chunk[0])?;
        let b1 = decode_char(chunk[1])?;
        let b2 = decode_char(chunk[2])?;
        let b3 = decode_char(chunk[3])?;

        let triple = ((b0 as u32) << 18) | ((b1 as u32) << 12) | ((b2 as u32) << 6) | (b3 as u32);
        bytes.push(((triple >> 16) & 0xFF) as u8);
        if chunk[2] != '=' {
            bytes.push(((triple >> 8) & 0xFF) as u8);
        }
        if chunk[3] != '=' {
            bytes.push((triple & 0xFF) as u8);
        }
    }
    String::from_utf8(bytes).ok()
}

fn extract_json_or_string(v: Option<&serde_json::Value>) -> Option<String> {
    match v {
        Some(serde_json::Value::String(s)) => Some(s.clone()),
        Some(serde_json::Value::Null) | None => None,
        Some(other) => Some(other.to_string()),
    }
}
