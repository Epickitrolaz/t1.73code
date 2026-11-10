use crate::protocol::models::ModelSelection;
use crate::storage::models::*;
use crate::storage::Storage;
use chrono::{Duration, Utc};
use tempfile::tempdir;

#[test]
fn test_storage_open_in_memory() {
    let storage = Storage::open_in_memory().expect("failed to open in-memory storage");
    let projects = storage.list_projects().expect("failed to list projects");
    assert!(projects.is_empty());
}

#[test]
fn test_storage_open_temp_file() {
    let dir = tempdir().expect("failed to create temp dir");
    let db_path = dir.path().join("userdata").join("statev2.sqlite");
    let storage = Storage::open(&db_path).expect("failed to open storage at path");
    let projects = storage.list_projects().expect("failed to list projects");
    assert!(projects.is_empty());
}

#[test]
fn test_storage_new_in_dir() {
    let dir = tempdir().expect("failed to create temp dir");
    let storage = Storage::new_in_dir(dir.path()).expect("failed to open storage in dir");
    let projects = storage.list_projects().expect("failed to list projects");
    assert!(projects.is_empty());
    assert!(dir.path().join("userdata").join("statev2.sqlite").exists());
}

#[test]
fn test_projects_crud() {
    let storage = Storage::open_in_memory().expect("failed to open storage");

    // 1. get_or_create_project creates new project
    let p1 = storage
        .get_or_create_project("/workspace/my-app", "My App")
        .expect("get_or_create failed");
    assert_eq!(p1.workspace_root, "/workspace/my-app");
    assert_eq!(p1.title, "My App");
    assert!(p1.deleted_at.is_none());

    // 2. get_or_create_project returns existing project if called again
    let p1_again = storage
        .get_or_create_project("/workspace/my-app", "Different Title")
        .expect("get_or_create again failed");
    assert_eq!(p1_again.id, p1.id);
    assert_eq!(p1_again.title, "My App"); // keeps existing title

    // 3. get_project
    let fetched = storage.get_project(&p1.id).expect("get_project failed");
    assert!(fetched.is_some());
    assert_eq!(fetched.unwrap().id, p1.id);

    let nonexistent = storage
        .get_project("missing-id")
        .expect("get_project failed");
    assert!(nonexistent.is_none());

    // 4. list_projects
    let p2 = storage
        .get_or_create_project("/workspace/other-app", "Other App")
        .expect("get_or_create p2 failed");
    let all = storage.list_projects().expect("list_projects failed");
    assert_eq!(all.len(), 2);
    let ids: Vec<String> = all.iter().map(|p| p.id.clone()).collect();
    assert!(ids.contains(&p1.id));
    assert!(ids.contains(&p2.id));

    // 5. update_project
    let mut to_update = p1.clone();
    to_update.title = "Renamed App".into();
    let updated = storage
        .update_project(&to_update)
        .expect("update_project failed");
    assert_eq!(updated.title, "Renamed App");
    let fetched_updated = storage
        .get_project(&p1.id)
        .expect("get_project failed")
        .unwrap();
    assert_eq!(fetched_updated.title, "Renamed App");

    // 6. delete_project (soft delete)
    let deleted = storage
        .delete_project(&p1.id)
        .expect("delete_project failed");
    assert!(deleted);
    let all_after_delete = storage.list_projects().expect("list_projects failed");
    assert_eq!(all_after_delete.len(), 1);
    assert_eq!(all_after_delete[0].id, p2.id);

    // Deleting already deleted project returns false
    let deleted_again = storage
        .delete_project(&p1.id)
        .expect("delete_project failed");
    assert!(!deleted_again);
}

#[test]
fn test_threads_crud() {
    let storage = Storage::open_in_memory().expect("failed to open storage");
    let project = storage
        .get_or_create_project("/workspace/my-app", "My App")
        .expect("get_or_create failed");

    // 1. create_thread with CreateThreadInput
    let thread_input = CreateThreadInput {
        id: Some("thread-1".into()),
        project_id: project.id.clone(),
        title: "Initial Thread".into(),
        provider_instance_id: "codex".into(),
        model_selection: Some(ModelSelection {
            instance_id: "codex".into(),
            model: "gpt-6".into(),
            options: None,
        }),
        runtime_mode: "full-access".into(),
        interaction_mode: "default".into(),
        branch: Some("main".into()),
        worktree_path: None,
    };
    let created = storage
        .create_thread(thread_input)
        .expect("create_thread failed");
    assert_eq!(created.id, "thread-1");
    assert_eq!(created.title, "Initial Thread");
    assert_eq!(created.provider_instance_id, "codex");
    assert_eq!(created.branch.as_deref(), Some("main"));

    // 2. get_thread
    let fetched = storage.get_thread("thread-1").expect("get_thread failed");
    assert!(fetched.is_some());
    let fetched = fetched.unwrap();
    assert_eq!(fetched.id, "thread-1");
    assert_eq!(fetched.title, "Initial Thread");
    assert_eq!(fetched.model_selection.model, "gpt-6");
    assert_eq!(fetched.item_count, 0);
    assert_eq!(fetched.latest_run_id, None);

    // 3. list_threads
    let thread2_input = CreateThreadInput {
        id: Some("thread-2".into()),
        project_id: project.id.clone(),
        title: "Second Thread".into(),
        provider_instance_id: "codex".into(),
        model_selection: None,
        runtime_mode: "full-access".into(),
        interaction_mode: "default".into(),
        branch: None,
        worktree_path: None,
    };
    storage
        .create_thread(thread2_input)
        .expect("create_thread 2 failed");

    let threads = storage
        .list_threads(&project.id)
        .expect("list_threads failed");
    assert_eq!(threads.len(), 2);

    // 4. update_thread
    let mut thread_to_update = fetched;
    thread_to_update.title = "Updated Title".into();
    thread_to_update.branch = Some("feature-1".into());
    let updated = storage
        .update_thread(&thread_to_update)
        .expect("update_thread failed");
    assert_eq!(updated.title, "Updated Title");
    assert_eq!(updated.branch.as_deref(), Some("feature-1"));

    // 5. settle_thread
    let settled = storage
        .settle_thread("thread-1")
        .expect("settle_thread failed");
    assert!(settled);

    // 6. archive_thread
    let archived = storage
        .archive_thread("thread-1")
        .expect("archive_thread failed");
    assert!(archived);
    let fetched_archived = storage
        .get_thread("thread-1")
        .expect("get_thread failed")
        .unwrap();
    assert!(fetched_archived.archived_at.is_some());

    // 7. delete_thread
    let deleted = storage
        .delete_thread("thread-1")
        .expect("delete_thread failed");
    assert!(deleted);
    let threads_after_delete = storage
        .list_threads(&project.id)
        .expect("list_threads failed");
    assert_eq!(threads_after_delete.len(), 1);
    assert_eq!(threads_after_delete[0].id, "thread-2");
}

#[test]
fn test_runs_and_turn_items() {
    let storage = Storage::open_in_memory().expect("failed to open storage");
    let project = storage
        .get_or_create_project("/workspace/my-app", "My App")
        .expect("get_or_create failed");
    let thread = storage
        .create_thread(CreateThreadInput {
            id: Some("thread-1".into()),
            project_id: project.id.clone(),
            title: "Thread".into(),
            provider_instance_id: "codex".into(),
            model_selection: None,
            runtime_mode: "full-access".into(),
            interaction_mode: "default".into(),
            branch: None,
            worktree_path: None,
        })
        .expect("create_thread failed");

    // 1. create_run
    let run1 = storage
        .create_run(CreateRunInput {
            id: None,
            thread_id: thread.id.clone(),
            ordinal: None,
            provider_instance_id: "codex".into(),
            model_selection: None,
            status: "running".into(),
            requested_at: None,
        })
        .expect("create_run failed");
    assert_eq!(run1.ordinal, 1);
    assert_eq!(run1.status, "running");

    let run2 = storage
        .create_run(CreateRunInput {
            id: None,
            thread_id: thread.id.clone(),
            ordinal: None,
            provider_instance_id: "codex".into(),
            model_selection: None,
            status: "queued".into(),
            requested_at: None,
        })
        .expect("create_run 2 failed");
    assert_eq!(run2.ordinal, 2);

    // 2. update_run_status
    let updated_run = storage
        .update_run_status(&run1.id, "completed")
        .expect("update_run_status failed");
    assert!(updated_run);
    let fetched_run1 = storage.get_run(&run1.id).expect("get_run failed").unwrap();
    assert_eq!(fetched_run1.status, "completed");
    assert!(fetched_run1.completed_at.is_some());

    // 3. list_runs
    let runs = storage.list_runs(&thread.id).expect("list_runs failed");
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].id, run1.id);
    assert_eq!(runs[1].id, run2.id);

    // 4. add_turn_item
    let item1 = storage
        .add_turn_item(CreateTurnItemInput {
            id: None,
            thread_id: thread.id.clone(),
            run_id: Some(run1.id.clone()),
            ordinal: None,
            status: "completed".into(),
            item_type: "user_message".into(),
            title: Some("Prompt".into()),
            text: Some("Hello agent".into()),
            input: None,
            output: None,
            exit_code: None,
            created_at: None,
        })
        .expect("add_turn_item failed");
    assert_eq!(item1.ordinal, 1);
    assert_eq!(item1.item_type, "user_message");
    assert_eq!(item1.text.as_deref(), Some("Hello agent"));

    let item2 = storage
        .add_turn_item(CreateTurnItemInput {
            id: None,
            thread_id: thread.id.clone(),
            run_id: Some(run1.id.clone()),
            ordinal: None,
            status: "running".into(),
            item_type: "command_execution".into(),
            title: Some("Run cargo".into()),
            text: None,
            input: Some("cargo test".into()),
            output: None,
            exit_code: None,
            created_at: None,
        })
        .expect("add_turn_item 2 failed");
    assert_eq!(item2.ordinal, 2);

    // 5. update_turn_item
    let mut item2_to_update = item2.clone();
    item2_to_update.status = "completed".into();
    item2_to_update.output = Some("test result: ok".into());
    item2_to_update.exit_code = Some(0);
    let updated_item2 = storage
        .update_turn_item(item2_to_update)
        .expect("update_turn_item failed");
    assert_eq!(updated_item2.status, "completed");
    assert_eq!(updated_item2.output.as_deref(), Some("test result: ok"));
    assert_eq!(updated_item2.exit_code, Some(0));

    // 6. get_turn_item
    let fetched_item = storage
        .get_turn_item(&item2.id)
        .expect("get_turn_item failed")
        .unwrap();
    assert_eq!(fetched_item.output.as_deref(), Some("test result: ok"));

    // 7. list_turn_items
    let items = storage
        .list_turn_items(&thread.id)
        .expect("list_turn_items failed");
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].id, item1.id);
    assert_eq!(items[1].id, item2.id);

    // 8. get_thread_projection
    let projection = storage
        .get_thread_projection(&thread.id)
        .expect("get_thread_projection failed")
        .unwrap();
    assert_eq!(projection.thread.id, thread.id);
    assert_eq!(projection.thread.item_count, 2);
    assert_eq!(projection.thread.latest_run_id, Some(run2.id));
    assert_eq!(projection.runs.len(), 2);
    assert_eq!(projection.turn_items.len(), 2);
}

#[test]
fn test_messages_crud() {
    let storage = Storage::open_in_memory().expect("failed to open storage");
    let project = storage
        .get_or_create_project("/workspace/my-app", "My App")
        .expect("get_or_create failed");
    let thread = storage
        .create_thread(CreateThreadInput {
            id: Some("thread-msg".into()),
            project_id: project.id.clone(),
            title: "Msg Thread".into(),
            provider_instance_id: "codex".into(),
            model_selection: None,
            runtime_mode: "full-access".into(),
            interaction_mode: "default".into(),
            branch: None,
            worktree_path: None,
        })
        .expect("create_thread failed");

    // 1. add_message
    let msg1 = storage
        .add_message(CreateMessageInput {
            id: None,
            thread_id: thread.id.clone(),
            run_id: None,
            node_id: None,
            role: "user".into(),
            text: "Hello".into(),
            streaming: false,
            created_at: None,
        })
        .expect("add_message failed");
    assert_eq!(msg1.role, "user");
    assert_eq!(msg1.text, "Hello");

    let msg2 = storage
        .add_message(CreateMessageInput {
            id: None,
            thread_id: thread.id.clone(),
            run_id: None,
            node_id: None,
            role: "assistant".into(),
            text: "Hi there!".into(),
            streaming: true,
            created_at: None,
        })
        .expect("add_message 2 failed");

    // 2. get_message
    let fetched = storage
        .get_message(&msg1.id)
        .expect("get_message failed")
        .unwrap();
    assert_eq!(fetched.text, "Hello");

    // 3. list_messages
    let messages = storage
        .list_messages(&thread.id)
        .expect("list_messages failed");
    assert_eq!(messages.len(), 2);
    let ids: Vec<String> = messages.iter().map(|m| m.id.clone()).collect();
    assert!(ids.contains(&msg1.id));
    assert!(ids.contains(&msg2.id));

    // 4. update_message
    let mut msg2_to_update = msg2.clone();
    msg2_to_update.streaming = false;
    msg2_to_update.text = "Hi there! How can I help?".into();
    let updated = storage
        .update_message(msg2_to_update)
        .expect("update_message failed");
    assert!(!updated.streaming);
    assert_eq!(updated.text, "Hi there! How can I help?");

    // 5. delete_message
    let deleted = storage
        .delete_message(&msg1.id)
        .expect("delete_message failed");
    assert!(deleted);
    let messages_after = storage
        .list_messages(&thread.id)
        .expect("list_messages failed");
    assert_eq!(messages_after.len(), 1);
    assert_eq!(messages_after[0].id, msg2.id);
}

#[test]
fn test_auth_and_pairing() {
    let storage = Storage::open_in_memory().expect("failed to open storage");

    // 1. create_pairing_link
    let token = "TOKEN123ABC";
    let scopes = vec![
        "orchestration:read".to_string(),
        "settings:write".to_string(),
    ];
    let expires_at = Utc::now() + Duration::minutes(15);

    let link = storage
        .create_pairing_link(token, &scopes, expires_at)
        .expect("create_pairing_link failed");
    assert_eq!(link.credential, token);
    assert_eq!(link.scopes, scopes);
    assert!(link.consumed_at.is_none());

    // 2. consume_pairing_link
    let consumed = storage
        .consume_pairing_link(token)
        .expect("consume_pairing_link failed");
    assert!(consumed.is_some());
    let consumed = consumed.unwrap();
    assert_eq!(consumed.credential, token);
    assert!(consumed.consumed_at.is_some());

    // Consuming second time fails (returns None)
    let consumed_again = storage
        .consume_pairing_link(token)
        .expect("consume_pairing_link failed");
    assert!(consumed_again.is_none());

    // Expired pairing link cannot be consumed
    let expired_token = "EXPIRED456";
    let past_expires_at = Utc::now() - Duration::minutes(5);
    storage
        .create_pairing_link(expired_token, &scopes, past_expires_at)
        .expect("create expired link failed");
    let consumed_expired = storage
        .consume_pairing_link(expired_token)
        .expect("consume failed");
    assert!(consumed_expired.is_none());

    // 3. create_session & validate_session
    let session_id = "session-xyz-123";
    let session_token = "session.bearer.token.abc";
    let session_expires_at = Utc::now() + Duration::days(7);

    let session = storage
        .create_session(session_id, session_token, session_expires_at)
        .expect("create_session failed");
    assert_eq!(session.session_id, session_id);
    assert_eq!(session.client_label.as_deref(), Some(session_token));

    // Validate using token
    let validated = storage
        .validate_session(session_token)
        .expect("validate_session failed");
    assert!(validated.is_some());
    let validated = validated.unwrap();
    assert_eq!(validated.session_id, session_id);
    assert!(validated.last_connected_at.is_some());

    // Validate using session_id
    let validated_by_sid = storage
        .validate_session(session_id)
        .expect("validate_session by sid failed");
    assert!(validated_by_sid.is_some());
    assert_eq!(validated_by_sid.unwrap().session_id, session_id);

    // List active sessions
    let active = storage
        .list_active_sessions()
        .expect("list_active_sessions failed");
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].session_id, session_id);

    // Revoke session
    let revoked = storage
        .revoke_session(session_id)
        .expect("revoke_session failed");
    assert!(revoked);

    // Revoked session cannot be validated
    let validated_after_revoke = storage
        .validate_session(session_token)
        .expect("validate_session failed");
    assert!(validated_after_revoke.is_none());

    // Expired session cannot be validated
    let exp_session_id = "expired-sid";
    let exp_token = "expired-token";
    storage
        .create_session(exp_session_id, exp_token, past_expires_at)
        .expect("create expired session failed");
    let validated_exp = storage
        .validate_session(exp_token)
        .expect("validate expired session failed");
    assert!(validated_exp.is_none());
}

#[test]
fn test_edge_cases_and_error_handling() {
    let storage = Storage::open_in_memory().expect("failed to open storage");

    // 1. update_project on non-existent returns NotFound
    let missing_project = OrchestrationProjectShell {
        id: "missing-proj".into(),
        title: "Missing".into(),
        workspace_root: "/no/where".into(),
        scripts: Vec::new(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        deleted_at: None,
    };
    let update_res = storage.update_project(&missing_project);
    assert!(matches!(update_res, Err(StorageError::NotFound(_))));

    // 2. update_turn_item on non-existent returns NotFound
    let missing_item = OrchestrationV2TurnItem {
        id: "missing-item".into(),
        thread_id: "thread-x".into(),
        run_id: None,
        ordinal: 1,
        status: "completed".into(),
        item_type: "user_message".into(),
        title: None,
        text: None,
        input: None,
        output: None,
        exit_code: None,
        created_at: None,
        updated_at: Utc::now(),
    };
    let update_item_res = storage.update_turn_item(missing_item);
    assert!(matches!(update_item_res, Err(StorageError::NotFound(_))));

    // 3. update_message on non-existent returns NotFound
    let missing_msg = OrchestrationMessage {
        id: "missing-msg".into(),
        thread_id: "thread-x".into(),
        run_id: None,
        node_id: None,
        role: "user".into(),
        text: "hello".into(),
        streaming: false,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    let update_msg_res = storage.update_message(missing_msg);
    assert!(matches!(update_msg_res, Err(StorageError::NotFound(_))));

    // 4. settle_thread and archive_thread on deleted thread return false
    let project = storage
        .get_or_create_project("/workspace/edge", "Edge")
        .expect("project create failed");
    let thread = storage
        .create_thread(CreateThreadInput {
            id: Some("thread-edge".into()),
            project_id: project.id.clone(),
            title: "Edge Thread".into(),
            provider_instance_id: "codex".into(),
            model_selection: None,
            runtime_mode: "full-access".into(),
            interaction_mode: "default".into(),
            branch: None,
            worktree_path: None,
        })
        .expect("thread create failed");

    let deleted = storage.delete_thread(&thread.id).expect("delete failed");
    assert!(deleted);

    let settle_deleted = storage.settle_thread(&thread.id).expect("settle failed");
    assert!(!settle_deleted);

    let archive_deleted = storage.archive_thread(&thread.id).expect("archive failed");
    assert!(!archive_deleted);

    // 5. update_run_status to "running" sets started_at
    let run = storage
        .create_run(CreateRunInput {
            id: Some("run-edge".into()),
            thread_id: thread.id.clone(),
            ordinal: Some(1),
            provider_instance_id: "codex".into(),
            model_selection: None,
            status: "queued".into(),
            requested_at: None,
        })
        .expect("run create failed");
    assert!(run.started_at.is_none());

    storage
        .update_run_status(&run.id, "running")
        .expect("update status failed");
    let fetched_run = storage.get_run(&run.id).expect("get_run failed").unwrap();
    assert_eq!(fetched_run.status, "running");
    assert!(fetched_run.started_at.is_some());
}

#[test]
fn test_compatibility_with_real_statev2_sqlite() {
    let source_path = std::path::Path::new("/root/.t3/userdata/statev2.sqlite");
    if !source_path.exists() {
        return;
    }
    let dir = tempdir().expect("failed to create temp dir");
    let dest_path = dir.path().join("statev2.sqlite");
    std::fs::copy(source_path, &dest_path).expect("failed to copy statev2.sqlite");

    let storage = Storage::open(&dest_path).expect("failed to open existing statev2.sqlite");
    let projects = storage.list_projects().expect("failed to list projects");
    println!(
        "Found {} projects in existing statev2.sqlite",
        projects.len()
    );
    for p in &projects {
        let threads = storage.list_threads(&p.id).expect("failed to list threads");
        println!("Project '{}' has {} threads", p.title, threads.len());
        for t in &threads {
            let runs = storage.list_runs(&t.id).expect("failed to list runs");
            let items = storage
                .list_turn_items(&t.id)
                .expect("failed to list items");
            println!(
                "  Thread '{}': {} runs, {} items",
                t.title,
                runs.len(),
                items.len()
            );
        }
    }
}
