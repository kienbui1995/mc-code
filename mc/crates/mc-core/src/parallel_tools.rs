use std::sync::Arc;
use std::time::Instant;

use mc_tools::{
    AuditEntry, AuditLog, HookEngine, HookEvent, PermissionOutcome, PermissionPolicy, ToolRegistry,
};
use tokio::sync::{mpsc, Semaphore};
use tokio_util::sync::CancellationToken;

/// Toolresult.
pub struct ToolResult {
    pub id: String,
    pub name: String,
    pub output: String,
    pub is_error: bool,
}

/// Read-only tools safe to run concurrently.
fn is_read_tool(name: &str) -> bool {
    matches!(
        name,
        "read_file"
            | "glob_search"
            | "grep_search"
            | "web_fetch"
            | "web_search"
            | "lsp_query"
            | "memory_read"
            | "task_get"
            | "task_list"
    )
}

/// Execute multiple tools: read tools run concurrently, write tools serialized.
#[allow(clippy::too_many_arguments, clippy::ref_option)]
/// Execute batch.
pub async fn execute_batch(
    tools: Vec<(String, String, String)>,
    registry: &Arc<ToolRegistry>,
    hook_engine: &Option<Arc<HookEngine>>,
    audit_log: &Option<Arc<AuditLog>>,
    policy: &PermissionPolicy,
    cancel: &CancellationToken,
    max_concurrent: usize,
    output_tx: Option<&mpsc::UnboundedSender<String>>,
) -> Vec<ToolResult> {
    if tools.is_empty() {
        return Vec::new();
    }

    let (reads, writes): (Vec<_>, Vec<_>) = tools.into_iter().partition(|t| is_read_tool(&t.1));

    let mut results = Vec::new();

    // Reads: concurrent
    if reads.len() > 1 && max_concurrent > 1 {
        let sem = Arc::new(Semaphore::new(max_concurrent));
        let mut handles = Vec::with_capacity(reads.len());
        for (id, name, input) in reads {
            let sem = Arc::clone(&sem);
            let reg = Arc::clone(registry);
            let hooks = hook_engine.clone();
            let audit = audit_log.clone();
            let pol = policy.clone();
            let cancel = cancel.clone();
            let otx = output_tx.cloned();
            handles.push(tokio::spawn(async move {
                let _permit = sem.acquire().await.ok();
                if cancel.is_cancelled() {
                    return ToolResult {
                        id,
                        name,
                        output: "cancelled".into(),
                        is_error: true,
                    };
                }
                execute_one(&id, &name, &input, &reg, &hooks, &audit, &pol, otx.as_ref()).await
            }));
        }
        for handle in handles {
            match handle.await {
                Ok(r) => results.push(r),
                Err(e) => results.push(ToolResult {
                    id: String::new(),
                    name: "unknown".into(),
                    output: e.to_string(),
                    is_error: true,
                }),
            }
        }
    } else {
        results.extend(
            execute_sequential(
                reads,
                registry,
                hook_engine,
                audit_log,
                policy,
                cancel,
                output_tx,
            )
            .await,
        );
    }

    // Writes: always sequential
    results.extend(
        execute_sequential(
            writes,
            registry,
            hook_engine,
            audit_log,
            policy,
            cancel,
            output_tx,
        )
        .await,
    );

    results
}

#[allow(clippy::ref_option)]
async fn execute_sequential(
    tools: Vec<(String, String, String)>,
    registry: &Arc<ToolRegistry>,
    hook_engine: &Option<Arc<HookEngine>>,
    audit_log: &Option<Arc<AuditLog>>,
    policy: &PermissionPolicy,
    cancel: &CancellationToken,
    output_tx: Option<&mpsc::UnboundedSender<String>>,
) -> Vec<ToolResult> {
    let mut results = Vec::with_capacity(tools.len());
    for (id, name, input) in tools {
        if cancel.is_cancelled() {
            results.push(ToolResult {
                id,
                name,
                output: "cancelled".into(),
                is_error: true,
            });
            break;
        }
        results.push(
            execute_one(
                &id,
                &name,
                &input,
                registry,
                hook_engine,
                audit_log,
                policy,
                output_tx,
            )
            .await,
        );
    }
    results
}

#[allow(clippy::too_many_arguments, clippy::ref_option)]
async fn execute_one(
    id: &str,
    name: &str,
    input: &str,
    registry: &Arc<ToolRegistry>,
    hook_engine: &Option<Arc<HookEngine>>,
    audit_log: &Option<Arc<AuditLog>>,
    policy: &PermissionPolicy,
    output_tx: Option<&mpsc::UnboundedSender<String>>,
) -> ToolResult {
    // Pre-hook
    if let Some(ref engine) = hook_engine {
        if let Err(e) = engine.fire(&HookEvent::PreToolCall, Some(name), &[("tool_name", name)]) {
            return ToolResult {
                id: id.into(),
                name: name.into(),
                output: e.to_string(),
                is_error: true,
            };
        }
    }

    let outcome = policy.authorize(name, input, None);
    let allowed = matches!(outcome, PermissionOutcome::Allow);
    let timer = Instant::now();

    let (output, is_error) = match outcome {
        PermissionOutcome::Allow => {
            let input_val: serde_json::Value =
                serde_json::from_str(input).unwrap_or_else(|_| serde_json::json!({"raw": input}));
            let res = if let Some(tx) = output_tx {
                registry.execute_streaming(name, &input_val, tx).await
            } else {
                registry.execute(name, &input_val).await
            };
            match res {
                Ok(out) => (out, false),
                Err(e) => (e.to_string(), true),
            }
        }
        PermissionOutcome::Deny { reason } => (reason, true),
    };

    let duration_ms = timer.elapsed().as_millis() as u64;

    // Audit
    if let Some(ref log) = audit_log {
        log.log(&AuditEntry {
            tool: name.into(),
            input_summary: input.into(),
            output_len: output.len(),
            is_error,
            duration_ms,
            allowed,
        });
    }

    // Post-hook
    if let Some(ref engine) = hook_engine {
        let _ = engine.fire(&HookEvent::PostToolCall, Some(name), &[("tool_name", name)]);
    }

    ToolResult {
        id: id.into(),
        name: name.into(),
        output,
        is_error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_tools::PermissionMode;

    #[tokio::test]
    async fn batch_empty() {
        let reg = Arc::new(ToolRegistry::new());
        let policy = PermissionPolicy::new(PermissionMode::Allow);
        let cancel = CancellationToken::new();
        let results = execute_batch(vec![], &reg, &None, &None, &policy, &cancel, 4, None).await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn batch_single_tool() {
        let reg = Arc::new(ToolRegistry::new());
        let policy = PermissionPolicy::new(PermissionMode::Allow);
        let cancel = CancellationToken::new();
        let tools = vec![(
            "t1".into(),
            "bash".into(),
            r#"{"command":"echo hi"}"#.into(),
        )];
        let results = execute_batch(tools, &reg, &None, &None, &policy, &cancel, 4, None).await;
        assert_eq!(results.len(), 1);
        assert!(results[0].output.contains("hi"));
    }

    #[tokio::test]
    async fn batch_cancelled() {
        let reg = Arc::new(ToolRegistry::new());
        let policy = PermissionPolicy::new(PermissionMode::Allow);
        let cancel = CancellationToken::new();
        cancel.cancel();
        let tools = vec![(
            "t1".into(),
            "bash".into(),
            r#"{"command":"echo hi"}"#.into(),
        )];
        let results = execute_batch(tools, &reg, &None, &None, &policy, &cancel, 4, None).await;
        assert!(results[0].is_error);
    }
}

fn is_write_tool(name: &str) -> bool {
    matches!(name, "write_file" | "edit_file" | "batch_edit" | "apply_patch")
}

/// Compute a diff preview summary for write tools (used by runtime for review_writes).
#[must_use]
pub fn diff_preview_summary(tool_name: &str, input_json: &str) -> String {
    compute_diff_preview(tool_name, input_json)
}

fn compute_diff_preview(tool_name: &str, input_json: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(input_json).unwrap_or_default();
    let path = v["path"].as_str().unwrap_or("?");
    match tool_name {
        "write_file" => {
            let old = std::fs::read_to_string(path).unwrap_or_default();
            let new = v["content"].as_str().unwrap_or("");
            let action = if old.is_empty() { "CREATE" } else { "MODIFY" };
            let diff = simple_diff(&old, new);
            format!("📝 [{action}] {path}\n{diff}")
        }
        "edit_file" => {
            let old_str = v["old_string"].as_str().unwrap_or("");
            let new_str = v["new_string"].as_str().unwrap_or("");
            let mut diff = format!("📝 [EDIT] {path}\n");
            for line in old_str.lines() {
                diff.push_str(&format!("- {line}\n"));
            }
            for line in new_str.lines() {
                diff.push_str(&format!("+ {line}\n"));
            }
            diff
        }
        _ => format!("📝 {tool_name}: {path}"),
    }
}

fn simple_diff(old: &str, new: &str) -> String {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let mut out = String::new();
    let max = old_lines.len().max(new_lines.len());
    let mut changes = 0;
    for i in 0..max {
        let ol = old_lines.get(i).copied();
        let nl = new_lines.get(i).copied();
        if ol != nl {
            if let Some(o) = ol {
                out.push_str(&format!("- {o}\n"));
            }
            if let Some(n) = nl {
                out.push_str(&format!("+ {n}\n"));
            }
            changes += 1;
            if changes > 30 {
                out.push_str(&format!("... ({} more lines)\n", max - i));
                break;
            }
        }
    }
    if changes == 0 {
        out.push_str("(no changes)\n");
    }
    out
}

#[cfg(test)]
mod diff_tests {
    use super::*;

    #[test]
    fn simple_diff_no_changes() {
        let d = simple_diff("hello\n", "hello\n");
        assert!(d.contains("no changes"));
    }

    #[test]
    fn simple_diff_detects_changes() {
        let d = simple_diff("old line\n", "new line\n");
        assert!(d.contains("- old line"));
        assert!(d.contains("+ new line"));
    }

    #[test]
    fn diff_preview_write_file() {
        let input = r#"{"path":"/tmp/test.txt","content":"hello world"}"#;
        let preview = diff_preview_summary("write_file", input);
        assert!(preview.contains("/tmp/test.txt"));
        assert!(preview.contains("CREATE") || preview.contains("MODIFY"));
    }

    #[test]
    fn diff_preview_edit_file() {
        let input = r#"{"path":"test.rs","old_string":"fn old()","new_string":"fn new()"}"#;
        let preview = diff_preview_summary("edit_file", input);
        assert!(preview.contains("EDIT"));
        assert!(preview.contains("- fn old()"));
        assert!(preview.contains("+ fn new()"));
    }

    #[test]
    fn is_write_tool_check() {
        assert!(is_write_tool("write_file"));
        assert!(is_write_tool("edit_file"));
        assert!(is_write_tool("batch_edit"));
        assert!(!is_write_tool("read_file"));
        assert!(!is_write_tool("bash"));
    }
}
