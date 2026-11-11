use chrono::Utc;
use std::env;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::protocol::models::*;
use crate::provider::{Attachment, DriverEvent, PiProcess, PiProcessConfig};
use crate::state::AppState;
use super::process_manager::ProcessManager;

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub fn next_sequence() -> u64 {
    SEQUENCE.fetch_add(1, Ordering::SeqCst) + 1
}

pub fn current_sequence() -> u64 {
    SEQUENCE.load(Ordering::SeqCst)
}

pub async fn handle_dispatch_command(
    state: &Arc<AppState>,
    payload: serde_json::Value,
    process_mgr: &ProcessManager,
) -> Result<u64, String> {
    let cmd_obj = if payload.get("type").is_some() {
        &payload
    } else if let Some(cmd) = payload.get("command") {
        cmd
    } else {
        &payload
    };

    let cmd_type = cmd_obj
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing command type".to_string())?;

    match cmd_type {
        "thread.create" => {
            let thread_id = cmd_obj
                .get("threadId")
                .or_else(|| cmd_obj.get("thread_id"))
                .and_then(|v| v.as_str())
                .map(ToString::to_string)
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

            let title = cmd_obj
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("New Thread");

            let cwd = env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| ".".to_string());

            let project_id = if let Some(pid) = cmd_obj
                .get("projectId")
                .or_else(|| cmd_obj.get("project_id"))
                .and_then(|v| v.as_str())
            {
                pid.to_string()
            } else {
                let p = state
                    .storage
                    .get_or_create_project(&cwd, "Default Project")
                    .map_err(|e| e.to_string())?;
                p.id
            };

            let model_selection = cmd_obj
                .get("modelSelection")
                .and_then(|v| serde_json::from_value::<ModelSelection>(v.clone()).ok())
                .unwrap_or_else(|| ModelSelection {
                    instance_id: "pi".to_string(),
                    model: "default".to_string(),
                    options: None,
                });

            let provider_instance_id = model_selection.instance_id.clone();

            let runtime_mode = cmd_obj
                .get("runtimeMode")
                .or_else(|| cmd_obj.get("runtime_mode"))
                .and_then(|v| v.as_str())
                .unwrap_or("autonomous");

            let interaction_mode = cmd_obj
                .get("interactionMode")
                .or_else(|| cmd_obj.get("interaction_mode"))
                .and_then(|v| v.as_str())
                .unwrap_or("chat");

            let branch = cmd_obj
                .get("branch")
                .and_then(|v| v.as_str())
                .map(ToString::to_string);

            let worktree_path = cmd_obj
                .get("worktreePath")
                .or_else(|| cmd_obj.get("worktree_path"))
                .and_then(|v| v.as_str())
                .map(ToString::to_string);

            let shell = OrchestrationV2ThreadShell {
                id: thread_id,
                project_id,
                title: title.to_string(),
                provider_instance_id,
                model_selection,
                runtime_mode: runtime_mode.to_string(),
                interaction_mode: interaction_mode.to_string(),
                branch,
                worktree_path,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                archived_at: None,
                deleted_at: None,
                status: Some("idle".to_string()),
                latest_run_id: None,
                item_count: 0,
            };

            let created = state
                .storage
                .create_thread(shell)
                .map_err(|e| e.to_string())?;

            let seq = next_sequence();
            let _ = state.shell_broadcast.send(
                OrchestrationV2ShellStreamItem::ThreadUpdated {
                    sequence: seq,
                    location: "active".to_string(),
                    thread: created,
                },
            );

            Ok(seq)
        }

        "message.dispatch" => {
            let thread_id = cmd_obj
                .get("threadId")
                .or_else(|| cmd_obj.get("thread_id"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing threadId".to_string())?
                .to_string();

            let message_id = cmd_obj
                .get("messageId")
                .or_else(|| cmd_obj.get("message_id"))
                .and_then(|v| v.as_str())
                .map(ToString::to_string)
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

            let text = cmd_obj
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let attachments: Vec<Attachment> = cmd_obj
                .get("attachments")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();

            let thread = state
                .storage
                .get_thread(&thread_id)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("Thread {} not found", thread_id))?;

            // 1. Record user message in storage
            let user_msg = crate::storage::models::OrchestrationMessage {
                id: message_id.clone(),
                thread_id: thread_id.clone(),
                run_id: None,
                node_id: None,
                role: "user".to_string(),
                text: text.clone(),
                streaming: false,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };
            let _ = state.storage.add_message(user_msg);

            // 2. Add user turn item in storage
            let user_turn_item = OrchestrationV2TurnItem {
                id: format!("item-user-{}", uuid::Uuid::new_v4()),
                thread_id: thread_id.clone(),
                run_id: None,
                ordinal: 0,
                status: "completed".to_string(),
                item_type: "user_message".to_string(),
                title: None,
                text: Some(text.clone()),
                input: None,
                output: None,
                exit_code: None,
                created_at: Some(Utc::now()),
                updated_at: Utc::now(),
            };
            let created_user_item = state
                .storage
                .add_turn_item(user_turn_item)
                .map_err(|e| e.to_string())?;

            let seq_item = next_sequence();
            let _ = state.thread_broadcast.send((
                thread_id.clone(),
                OrchestrationV2ThreadStreamItem::Event {
                    sequence: seq_item,
                    event: serde_json::json!({
                        "type": "turn_item.completed",
                        "item": created_user_item,
                    }),
                },
            ));

            // 3. Create run in storage
            let run = OrchestrationV2Run {
                id: String::new(),
                thread_id: thread_id.clone(),
                ordinal: 0,
                provider_instance_id: thread.provider_instance_id.clone(),
                model_selection: thread.model_selection.clone(),
                status: "requested".to_string(),
                requested_at: Utc::now(),
                started_at: None,
                completed_at: None,
            };
            let created_run = state.storage.create_run(run).map_err(|e| e.to_string())?;
            let run_id = created_run.id.clone();

            let seq_run = next_sequence();
            let _ = state.thread_broadcast.send((
                thread_id.clone(),
                OrchestrationV2ThreadStreamItem::Event {
                    sequence: seq_run,
                    event: serde_json::json!({
                        "type": "run.started",
                        "run": created_run,
                    }),
                },
            ));

            // 4. Update thread status to running
            let mut updated_thread = thread.clone();
            updated_thread.status = Some("running".to_string());
            updated_thread.latest_run_id = Some(run_id.clone());
            updated_thread.updated_at = Utc::now();
            let _ = state.storage.update_thread(&updated_thread);

            let seq_th = next_sequence();
            let _ = state.shell_broadcast.send(
                OrchestrationV2ShellStreamItem::ThreadUpdated {
                    sequence: seq_th,
                    location: "active".to_string(),
                    thread: updated_thread.clone(),
                },
            );

            // 5. Spawn background task to drive Pi subprocess
            let state_bg = state.clone();
            let run_id_bg = run_id;
            let text_bg = text;
            let attachments_bg = attachments;
            let process_mgr_bg = process_mgr.clone();
            let thread_bg = thread;

            tokio::spawn(async move {
                run_pi_agent_turn(
                    state_bg,
                    thread_bg,
                    run_id_bg,
                    text_bg,
                    attachments_bg,
                    process_mgr_bg,
                )
                .await;
            });

            Ok(seq_th)
        }

        "thread.stop" => {
            let thread_id = cmd_obj
                .get("threadId")
                .or_else(|| cmd_obj.get("thread_id"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing threadId".to_string())?;

            if let Some(proc) = process_mgr.get(thread_id).await {
                let _ = proc.abort().await;
            }

            if let Ok(runs) = state.storage.list_runs(thread_id) {
                for r in runs {
                    if r.status == "running" || r.status == "requested" {
                        let _ = state.storage.update_run_status(&r.id, "cancelled");
                    }
                }
            }

            let seq = next_sequence();
            let _ = state.thread_broadcast.send((
                thread_id.to_string(),
                OrchestrationV2ThreadStreamItem::Event {
                    sequence: seq,
                    event: serde_json::json!({
                        "type": "run.cancelled",
                        "threadId": thread_id,
                    }),
                },
            ));

            if let Ok(Some(mut th)) = state.storage.get_thread(thread_id) {
                th.status = Some("idle".to_string());
                th.updated_at = Utc::now();
                let _ = state.storage.update_thread(&th);
                let loc = if th.archived_at.is_some() { "archive" } else { "active" };
                let _ = state.shell_broadcast.send(
                    OrchestrationV2ShellStreamItem::ThreadUpdated {
                        sequence: seq,
                        location: loc.to_string(),
                        thread: th,
                    },
                );
            }

            Ok(seq)
        }

        "thread.archive" => {
            let thread_id = cmd_obj
                .get("threadId")
                .or_else(|| cmd_obj.get("thread_id"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing threadId".to_string())?;

            let _ = state
                .storage
                .archive_thread(thread_id)
                .map_err(|e| e.to_string())?;

            let seq = next_sequence();
            if let Ok(Some(th)) = state.storage.get_thread(thread_id) {
                let _ = state.shell_broadcast.send(
                    OrchestrationV2ShellStreamItem::ThreadUpdated {
                        sequence: seq,
                        location: "archive".to_string(),
                        thread: th,
                    },
                );
            }

            Ok(seq)
        }

        "thread.unarchive" => {
            let thread_id = cmd_obj
                .get("threadId")
                .or_else(|| cmd_obj.get("thread_id"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing threadId".to_string())?;

            let mut th = state
                .storage
                .get_thread(thread_id)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("Thread {} not found", thread_id))?;

            th.archived_at = None;
            th.updated_at = Utc::now();
            let updated = state
                .storage
                .update_thread(&th)
                .map_err(|e| e.to_string())?;

            let seq = next_sequence();
            let _ = state.shell_broadcast.send(
                OrchestrationV2ShellStreamItem::ThreadUpdated {
                    sequence: seq,
                    location: "active".to_string(),
                    thread: updated,
                },
            );

            Ok(seq)
        }

        "thread.settle" => {
            let thread_id = cmd_obj
                .get("threadId")
                .or_else(|| cmd_obj.get("thread_id"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing threadId".to_string())?;

            let _ = state
                .storage
                .settle_thread(thread_id)
                .map_err(|e| e.to_string())?;

            let seq = next_sequence();
            if let Ok(Some(th)) = state.storage.get_thread(thread_id) {
                let loc = if th.archived_at.is_some() { "archive" } else { "active" };
                let _ = state.shell_broadcast.send(
                    OrchestrationV2ShellStreamItem::ThreadUpdated {
                        sequence: seq,
                        location: loc.to_string(),
                        thread: th,
                    },
                );
            }

            Ok(seq)
        }

        "thread.unsettle" => {
            let thread_id = cmd_obj
                .get("threadId")
                .or_else(|| cmd_obj.get("thread_id"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing threadId".to_string())?;

            let mut th = state
                .storage
                .get_thread(thread_id)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("Thread {} not found", thread_id))?;

            th.updated_at = Utc::now();
            let updated = state
                .storage
                .update_thread(&th)
                .map_err(|e| e.to_string())?;

            let seq = next_sequence();
            let loc = if updated.archived_at.is_some() { "archive" } else { "active" };
            let _ = state.shell_broadcast.send(
                OrchestrationV2ShellStreamItem::ThreadUpdated {
                    sequence: seq,
                    location: loc.to_string(),
                    thread: updated,
                },
            );

            Ok(seq)
        }

        "thread.delete" => {
            let thread_id = cmd_obj
                .get("threadId")
                .or_else(|| cmd_obj.get("thread_id"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing threadId".to_string())?;

            // Abort and clean up any running Pi process
            process_mgr.abort(thread_id).await;

            let loc = if let Ok(Some(th)) = state.storage.get_thread(thread_id) {
                if th.archived_at.is_some() { "archive" } else { "active" }
            } else {
                "active"
            };

            let _ = state
                .storage
                .delete_thread(thread_id)
                .map_err(|e| e.to_string())?;

            let seq = next_sequence();
            let _ = state.shell_broadcast.send(
                OrchestrationV2ShellStreamItem::ThreadRemoved {
                    sequence: seq,
                    location: loc.to_string(),
                    thread_id: thread_id.to_string(),
                },
            );

            Ok(seq)
        }

        other => Err(format!("Unsupported command type: {}", other)),
    }
}

async fn run_pi_agent_turn(
    state: Arc<AppState>,
    thread: OrchestrationV2ThreadShell,
    run_id: String,
    text: String,
    attachments: Vec<Attachment>,
    process_mgr: ProcessManager,
) {
    let thread_id = thread.id.clone();

    // Check if we already have a running PiProcess
    let proc = if let Some(existing) = process_mgr.get(&thread_id).await {
        if existing.is_running() {
            Some(existing)
        } else {
            process_mgr.remove(&thread_id).await;
            None
        }
    } else {
        None
    };

    let proc = match proc {
        Some(p) => p,
        None => {
            let mut config = PiProcessConfig::default();
            if let Some(prog) = thread
                .model_selection
                .options
                .as_ref()
                .and_then(|o| o.get("program"))
                .and_then(|p| p.as_str())
            {
                config.program = PathBuf::from(prog);
            } else if let Ok(prog) = std::env::var("PI_PROGRAM_PATH") {
                config.program = PathBuf::from(prog);
            }

            config.thread_id = Some(thread_id.clone());
            config.run_id = Some(run_id.clone());
            config.provider_instance_id = Some(thread.provider_instance_id.clone());
            config.model_selection = Some(thread.model_selection.clone());

            let cwd = thread.worktree_path.map(PathBuf::from).or_else(|| {
                state
                    .storage
                    .get_project(&thread.project_id)
                    .ok()
                    .flatten()
                    .map(|p| PathBuf::from(p.workspace_root))
            });
            config.cwd = cwd;

            match PiProcess::spawn(config).await {
                Ok(new_proc) => {
                    let arc_proc = Arc::new(new_proc);
                    process_mgr.set(thread_id.clone(), arc_proc.clone()).await;
                    arc_proc
                }
                Err(e) => {
                    tracing::warn!("Failed to spawn Pi agent: {e}");
                    let _ = state.storage.update_run_status(&run_id, "failed");
                    let err_item = OrchestrationV2TurnItem {
                        id: format!("item-err-{}", uuid::Uuid::new_v4()),
                        thread_id: thread_id.clone(),
                        run_id: Some(run_id.clone()),
                        ordinal: 0,
                        status: "failed".to_string(),
                        item_type: "assistant_message".to_string(),
                        title: Some("Error".to_string()),
                        text: Some(format!("Pi process failed to start: {}", e)),
                        input: None,
                        output: None,
                        exit_code: Some(1),
                        created_at: Some(Utc::now()),
                        updated_at: Utc::now(),
                    };
                    let _ = state.storage.add_turn_item(err_item.clone());
                    let seq = next_sequence();
                    let _ = state.thread_broadcast.send((
                        thread_id.clone(),
                        OrchestrationV2ThreadStreamItem::Event {
                            sequence: seq,
                            event: serde_json::json!({
                                "type": "turn_item.completed",
                                "item": err_item,
                            }),
                        },
                    ));
                    if let Ok(Some(mut th)) = state.storage.get_thread(&thread_id) {
                        th.status = Some("idle".to_string());
                        th.updated_at = Utc::now();
                        let _ = state.storage.update_thread(&th);
                        let _ = state.shell_broadcast.send(
                            OrchestrationV2ShellStreamItem::ThreadUpdated {
                                sequence: seq,
                                location: "active".to_string(),
                                thread: th,
                            },
                        );
                    }
                    return;
                }
            }
        }
    };

    let state_ev = state.clone();
    let thread_id_ev = thread_id.clone();
    let run_id_ev = run_id.clone();
    let mut driver_rx = proc.subscribe();

    let event_loop = tokio::spawn(async move {
        while let Ok(event) = driver_rx.recv().await {
            match event {
                DriverEvent::TurnItemStarted(mut item) => {
                    item.run_id = Some(run_id_ev.clone());
                    let _ = state_ev.storage.add_turn_item(item.clone());
                    let seq = next_sequence();
                    let _ = state_ev.thread_broadcast.send((
                        thread_id_ev.clone(),
                        OrchestrationV2ThreadStreamItem::Event {
                            sequence: seq,
                            event: serde_json::json!({
                                "type": "turn_item.started",
                                "item": item,
                            }),
                        },
                    ));
                }
                DriverEvent::TurnItemUpdated(mut item) => {
                    item.run_id = Some(run_id_ev.clone());
                    let _ = state_ev.storage.update_turn_item(item.clone());
                    let seq = next_sequence();
                    let _ = state_ev.thread_broadcast.send((
                        thread_id_ev.clone(),
                        OrchestrationV2ThreadStreamItem::Event {
                            sequence: seq,
                            event: serde_json::json!({
                                "type": "turn_item.updated",
                                "item": item,
                            }),
                        },
                    ));
                }
                DriverEvent::TurnItemCompleted(mut item) => {
                    item.run_id = Some(run_id_ev.clone());
                    let _ = state_ev.storage.update_turn_item(item.clone());
                    let seq = next_sequence();
                    let _ = state_ev.thread_broadcast.send((
                        thread_id_ev.clone(),
                        OrchestrationV2ThreadStreamItem::Event {
                            sequence: seq,
                            event: serde_json::json!({
                                "type": "turn_item.completed",
                                "item": item,
                            }),
                        },
                    ));
                }
                DriverEvent::RunStarted(mut run) => {
                    run.id = run_id_ev.clone();
                    let _ = state_ev.storage.update_run_status(&run_id_ev, "running");
                    let seq = next_sequence();
                    let _ = state_ev.thread_broadcast.send((
                        thread_id_ev.clone(),
                        OrchestrationV2ThreadStreamItem::Event {
                            sequence: seq,
                            event: serde_json::json!({
                                "type": "run.started",
                                "run": run,
                            }),
                        },
                    ));
                }
                DriverEvent::RunUpdated(mut run) => {
                    run.id = run_id_ev.clone();
                    let _ = state_ev.storage.update_run_status(&run_id_ev, &run.status);
                    let seq = next_sequence();
                    let _ = state_ev.thread_broadcast.send((
                        thread_id_ev.clone(),
                        OrchestrationV2ThreadStreamItem::Event {
                            sequence: seq,
                            event: serde_json::json!({
                                "type": "run.updated",
                                "run": run,
                            }),
                        },
                    ));
                }
                DriverEvent::RunCompleted(mut run) => {
                    run.id = run_id_ev.clone();
                    let _ = state_ev.storage.update_run_status(&run_id_ev, &run.status);
                    let seq = next_sequence();
                    let _ = state_ev.thread_broadcast.send((
                        thread_id_ev.clone(),
                        OrchestrationV2ThreadStreamItem::Event {
                            sequence: seq,
                            event: serde_json::json!({
                                "type": "run.completed",
                                "run": run,
                            }),
                        },
                    ));
                    if let Ok(Some(mut th)) = state_ev.storage.get_thread(&thread_id_ev) {
                        th.status = Some("idle".to_string());
                        th.updated_at = Utc::now();
                        let _ = state_ev.storage.update_thread(&th);
                        let _ = state_ev.shell_broadcast.send(
                            OrchestrationV2ShellStreamItem::ThreadUpdated {
                                sequence: seq,
                                location: "active".to_string(),
                                thread: th,
                            },
                        );
                    }
                    break;
                }
                DriverEvent::Raw(_) => {}
            }
        }
    });

    // Send prompt to Pi
    let send_res = proc.send_prompt(&text, &attachments).await;
    if let Err(e) = send_res {
        tracing::warn!("Failed to send prompt to Pi agent: {e}");
        event_loop.abort();
        let _ = state.storage.update_run_status(&run_id, "failed");
        let err_item = OrchestrationV2TurnItem {
            id: format!("item-err-{}", uuid::Uuid::new_v4()),
            thread_id: thread_id.clone(),
            run_id: Some(run_id.clone()),
            ordinal: 0,
            status: "failed".to_string(),
            item_type: "assistant_message".to_string(),
            title: Some("Error".to_string()),
            text: Some(format!("Failed to send prompt to Pi: {}", e)),
            input: None,
            output: None,
            exit_code: Some(1),
            created_at: Some(Utc::now()),
            updated_at: Utc::now(),
        };
        let _ = state.storage.add_turn_item(err_item.clone());
        let seq = next_sequence();
        let _ = state.thread_broadcast.send((
            thread_id.clone(),
            OrchestrationV2ThreadStreamItem::Event {
                sequence: seq,
                event: serde_json::json!({
                    "type": "turn_item.completed",
                    "item": err_item,
                }),
            },
        ));
    } else {
        let _ = event_loop.await;
    }

    // Ensure run and thread statuses are settled
    if let Ok(Some(r)) = state.storage.get_run(&run_id) {
        if r.status == "running" || r.status == "requested" {
            let _ = state.storage.update_run_status(&run_id, "completed");
        }
    }

    if let Ok(Some(mut th)) = state.storage.get_thread(&thread_id) {
        if th.status.as_deref() == Some("running") {
            th.status = Some("idle".to_string());
            th.updated_at = Utc::now();
            let _ = state.storage.update_thread(&th);
            let seq = next_sequence();
            let _ = state.shell_broadcast.send(
                OrchestrationV2ShellStreamItem::ThreadUpdated {
                    sequence: seq,
                    location: "active".to_string(),
                    thread: th,
                },
            );
        }
    }
}
