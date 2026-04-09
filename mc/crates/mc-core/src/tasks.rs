use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone)]
pub struct TaskInfo {
    pub id: String,
    pub description: String,
    pub command: String,
    pub status: TaskStatus,
    pub output: String,
    pub exit_code: Option<i32>,
}

struct TaskEntry {
    info: TaskInfo,
    handle: Option<JoinHandle<()>>,
}

pub struct TaskManager {
    tasks: Arc<Mutex<HashMap<String, TaskEntry>>>,
    next_id: std::sync::atomic::AtomicU32,
}

impl TaskManager {
    #[must_use]
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(Mutex::new(HashMap::new())),
            next_id: std::sync::atomic::AtomicU32::new(1),
        }
    }

    pub async fn create(&self, description: &str, command: &str) -> String {
        let id = format!(
            "task-{}",
            self.next_id
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        let info = TaskInfo {
            id: id.clone(),
            description: description.to_string(),
            command: command.to_string(),
            status: TaskStatus::Running,
            output: String::new(),
            exit_code: None,
        };

        let tasks = Arc::clone(&self.tasks);
        let task_id = id.clone();
        let cmd = command.to_string();
        let handle = tokio::spawn(async move {
            let result = tokio::process::Command::new("sh")
                .arg("-c")
                .arg(&cmd)
                .output()
                .await;
            let mut map = tasks.lock().await;
            if let Some(entry) = map.get_mut(&task_id) {
                match result {
                    Ok(output) => {
                        entry.info.output = String::from_utf8_lossy(&output.stdout).to_string();
                        if !output.stderr.is_empty() {
                            entry.info.output.push_str("\nSTDERR: ");
                            entry
                                .info
                                .output
                                .push_str(&String::from_utf8_lossy(&output.stderr));
                        }
                        entry.info.exit_code = output.status.code();
                        entry.info.status = if output.status.success() {
                            TaskStatus::Completed
                        } else {
                            TaskStatus::Failed
                        };
                    }
                    Err(e) => {
                        entry.info.output = e.to_string();
                        entry.info.status = TaskStatus::Failed;
                    }
                }
                entry.handle = None;
            }
        });

        self.tasks.lock().await.insert(
            id.clone(),
            TaskEntry {
                info,
                handle: Some(handle),
            },
        );
        id
    }

    pub async fn get(&self, id: &str) -> Option<TaskInfo> {
        self.tasks.lock().await.get(id).map(|e| e.info.clone())
    }

    pub async fn list(&self) -> Vec<TaskInfo> {
        self.tasks
            .lock()
            .await
            .values()
            .map(|e| e.info.clone())
            .collect()
    }

    /// Stop a running task — aborts the tokio task.
    pub async fn stop(&self, id: &str) -> bool {
        let mut map = self.tasks.lock().await;
        if let Some(entry) = map.get_mut(id) {
            if entry.info.status == TaskStatus::Running {
                if let Some(handle) = entry.handle.take() {
                    handle.abort();
                }
                entry.info.status = TaskStatus::Failed;
                entry.info.output.push_str("\n[stopped by user]");
                return true;
            }
        }
        false
    }
}

impl Default for TaskManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn create_and_poll_task() {
        let mgr = TaskManager::new();
        let id = mgr.create("test", "echo hello").await;
        assert!(id.starts_with("task-"));
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let info = mgr.get(&id).await.unwrap();
        assert_eq!(info.status, TaskStatus::Completed);
        assert!(info.output.contains("hello"));
    }

    #[tokio::test]
    async fn list_tasks() {
        let mgr = TaskManager::new();
        mgr.create("t1", "echo a").await;
        mgr.create("t2", "echo b").await;
        let tasks = mgr.list().await;
        assert_eq!(tasks.len(), 2);
    }

    #[tokio::test]
    async fn stop_kills_task() {
        let mgr = TaskManager::new();
        let id = mgr.create("long", "sleep 60").await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(mgr.stop(&id).await);
        let info = mgr.get(&id).await.unwrap();
        assert_eq!(info.status, TaskStatus::Failed);
        assert!(info.output.contains("stopped"));
    }
}
