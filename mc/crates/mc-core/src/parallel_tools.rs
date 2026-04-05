use std::sync::Arc;
use std::time::Instant;

use mc_tools::{
    AuditEntry, AuditLog, HookEngine, HookEvent, PermissionOutcome, PermissionPolicy, ToolRegistry,
};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

pub struct ToolResult {
    pub id: String,
    pub name: String,
    pub output: String,
    pub is_error: bool,
}

/// Execute multiple tools concurrently with a concurrency limit.
/// Subagent calls are excluded (must be run sequentially by caller).
#[allow(clippy::ref_option)]
pub async fn execute_batch(
    tools: Vec<(String, String, String)>, // (id, name, input_json)
    registry: &Arc<ToolRegistry>,
    hook_engine: &Option<Arc<HookEngine>>,
    audit_log: &Option<Arc<AuditLog>>,
    policy: &PermissionPolicy,
    cancel: &CancellationToken,
    max_concurrent: usize,
) -> Vec<ToolResult> {
    if tools.is_empty() {
        return Vec::new();
    }
    if tools.len() == 1 || max_concurrent <= 1 {
        return execute_sequential(tools, registry, hook_engine, audit_log, policy, cancel).await;
    }

    let sem = Arc::new(Semaphore::new(max_concurrent));
    let mut handles = Vec::with_capacity(tools.len());

    for (id, name, input) in tools {
        let sem = Arc::clone(&sem);
        let reg = Arc::clone(registry);
        let hooks = hook_engine.clone();
        let audit = audit_log.clone();
        let pol = policy.clone();
        let cancel = cancel.clone();

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
            execute_one(&id, &name, &input, &reg, &hooks, &audit, &pol).await
        }));
    }

    let mut results = Vec::with_capacity(handles.len());
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
        results
            .push(execute_one(&id, &name, &input, registry, hook_engine, audit_log, policy).await);
    }
    results
}

#[allow(clippy::ref_option)]
async fn execute_one(
    id: &str,
    name: &str,
    input: &str,
    registry: &Arc<ToolRegistry>,
    hook_engine: &Option<Arc<HookEngine>>,
    audit_log: &Option<Arc<AuditLog>>,
    policy: &PermissionPolicy,
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
            match registry.execute(name, &input_val).await {
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
        let results = execute_batch(vec![], &reg, &None, &None, &policy, &cancel, 4).await;
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
        let results = execute_batch(tools, &reg, &None, &None, &policy, &cancel, 4).await;
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
        let results = execute_batch(tools, &reg, &None, &None, &policy, &cancel, 4).await;
        assert!(results[0].is_error);
    }
}
