use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::broadcast;
use crate::protocol::models::*;
use crate::storage::StorageEngine;

#[derive(Clone)]
pub struct AppState {
    pub home_dir: PathBuf,
    pub environment_id: String,
    pub server_version: String,
    pub storage: Arc<StorageEngine>,
    pub shell_broadcast: broadcast::Sender<OrchestrationV2ShellStreamItem>,
    pub thread_broadcast: broadcast::Sender<(String, OrchestrationV2ThreadStreamItem)>,
}

impl AppState {
    pub fn new(home_dir: PathBuf, storage: Arc<StorageEngine>) -> Self {
        let (shell_tx, _) = broadcast::channel(128);
        let (thread_tx, _) = broadcast::channel(256);
        let env_id = crate::cloud::storage::get_or_create_environment_id(&home_dir)
            .unwrap_or_else(|_| uuid::Uuid::new_v4().to_string());

        Self {
            home_dir,
            environment_id: env_id,
            server_version: "0.1.73".to_string(),
            storage,
            shell_broadcast: shell_tx,
            thread_broadcast: thread_tx,
        }
    }
}
