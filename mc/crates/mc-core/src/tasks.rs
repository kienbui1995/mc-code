use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

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

pub struct TaskManager {
    tasks: Arc<Mutex<HashMap<String, TaskInfo>>>,
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

    /// Spawn a background task. Returns task ID immediately.
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
        self.tasks.lock().await.insert(id.clone(), info);

        let tasks = Arc::clone(&self.tasks);
        let task_id = id.clone();
        let cmd = command.to_string();
        tokio::spawn(async move {
            let result = tokio::process::Command::new("sh")
                .arg("-c")
                .arg(&cmd)
                .output()
                .await;
            let mut map = tasks.lock().await;
            if let Some(task) = map.get_mut(&task_id) {
                match result {
                    Ok(output) => {
                        task.output = String::from_utf8_lossy(&output.stdout).to_string();
                        if !output.stderr.is_empty() {
                            task.output.push_str("\nSTDERR: ");
                            task.output
                                .push_str(&String::from_utf8_lossy(&output.stderr));
                        }
                        task.exit_code = output.status.code();
                        task.status = if output.status.success() {
                            TaskStatus::Completed
                        } else {
                            TaskStatus::Failed
                        };
                    }
                    Err(e) => {
                        task.output = e.to_string();
                        task.status = TaskStatus::Failed;
                    }
                }
            }
        });

        id
    }

    /// Get task info by ID.
    pub async fn get(&self, id: &str) -> Option<TaskInfo> {
        self.tasks.lock().await.get(id).cloned()
    }

    /// List all tasks.
    pub async fn list(&self) -> Vec<TaskInfo> {
        self.tasks.lock().await.values().cloned().collect()
    }

    /// Stop a running task (best-effort, marks as failed).
    pub async fn stop(&self, id: &str) -> bool {
        let mut map = self.tasks.lock().await;
        if let Some(task) = map.get_mut(id) {
            if task.status == TaskStatus::Running {
                task.status = TaskStatus::Failed;
                task.output.push_str("\n[stopped by user]");
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
        // Wait for task to complete
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
}
