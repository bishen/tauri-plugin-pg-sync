use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncTask {
    pub id: String,
    pub table_name: String,
    pub row_id: String,
    pub operation: String,
    pub payload: serde_json::Value,
    pub hlc: String,
    pub retries: u32,
    pub max_retries: u32,
}

impl SyncTask {
    pub fn new(
        table_name: &str,
        row_id: &str,
        operation: &str,
        payload: serde_json::Value,
        hlc: &str,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            table_name: table_name.to_string(),
            row_id: row_id.to_string(),
            operation: operation.to_string(),
            payload,
            hlc: hlc.to_string(),
            retries: 0,
            max_retries: 3,
        }
    }

    pub fn can_retry(&self) -> bool {
        self.retries < self.max_retries
    }

    pub fn increment_retry(&mut self) {
        self.retries += 1;
    }
}

pub struct SyncQueue {
    pending: Mutex<VecDeque<SyncTask>>,
    failed: Mutex<Vec<SyncTask>>,
}

impl SyncQueue {
    pub fn new() -> Self {
        Self {
            pending: Mutex::new(VecDeque::new()),
            failed: Mutex::new(Vec::new()),
        }
    }

    pub fn enqueue(&self, task: SyncTask) {
        self.pending.lock().unwrap().push_back(task);
    }

    pub fn dequeue(&self) -> Option<SyncTask> {
        self.pending.lock().unwrap().pop_front()
    }

    pub fn peek(&self) -> Option<SyncTask> {
        self.pending.lock().unwrap().front().cloned()
    }

    pub fn len(&self) -> usize {
        self.pending.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.lock().unwrap().is_empty()
    }

    pub fn mark_failed(&self, mut task: SyncTask) {
        task.increment_retry();
        if task.can_retry() {
            self.pending.lock().unwrap().push_back(task);
        } else {
            self.failed.lock().unwrap().push(task);
        }
    }

    pub fn get_failed(&self) -> Vec<SyncTask> {
        self.failed.lock().unwrap().clone()
    }

    pub fn clear_failed(&self) {
        self.failed.lock().unwrap().clear();
    }

    pub fn retry_failed(&self) {
        let failed: Vec<SyncTask> = self.failed.lock().unwrap().drain(..).collect();
        for mut task in failed {
            task.retries = 0;
            self.enqueue(task);
        }
    }
}

impl Default for SyncQueue {
    fn default() -> Self {
        Self::new()
    }
}
