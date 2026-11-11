use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::provider::PiProcess;

#[derive(Default, Clone)]
pub struct ProcessManager {
    processes: Arc<Mutex<HashMap<String, Arc<PiProcess>>>>,
}

impl ProcessManager {
    pub fn new() -> Self {
        Self {
            processes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn get(&self, thread_id: &str) -> Option<Arc<PiProcess>> {
        let lock = self.processes.lock().await;
        lock.get(thread_id).cloned()
    }

    pub async fn set(&self, thread_id: String, proc: Arc<PiProcess>) {
        let mut lock = self.processes.lock().await;
        lock.insert(thread_id, proc);
    }

    pub async fn remove(&self, thread_id: &str) -> Option<Arc<PiProcess>> {
        let mut lock = self.processes.lock().await;
        lock.remove(thread_id)
    }

    pub async fn abort(&self, thread_id: &str) -> bool {
        let proc = {
            let mut lock = self.processes.lock().await;
            lock.remove(thread_id)
        };
        if let Some(proc) = proc {
            let _ = proc.abort().await;
            true
        } else {
            false
        }
    }
}

static PROCESS_MANAGER: std::sync::OnceLock<ProcessManager> = std::sync::OnceLock::new();

pub fn global_process_manager() -> &'static ProcessManager {
    PROCESS_MANAGER.get_or_init(ProcessManager::new)
}
