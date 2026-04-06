use std::process::Command;

use crate::error::ToolError;

#[derive(Debug, Clone, PartialEq, Eq)]
/// Hookevent.
pub enum HookEvent {
    PreToolCall,
    PostToolCall,
    PreCompact,
    PostCompact,
}

#[derive(Debug, Clone)]
/// Hook.
pub struct Hook {
    pub event: HookEvent,
    pub command: String,
    /// Only fire for these tools (empty = all)
    pub match_tools: Vec<String>,
}

/// Hookengine.
pub struct HookEngine {
    hooks: Vec<Hook>,
}

impl HookEngine {
    #[must_use]
    /// New.
    pub fn new(hooks: Vec<Hook>) -> Self {
        Self { hooks }
    }

    /// Fire hooks for an event. Returns Err if a blocking hook (`PreToolCall`) returns non-zero.
    pub fn fire(
        &self,
        event: &HookEvent,
        tool_name: Option<&str>,
        context: &[(&str, &str)],
    ) -> Result<(), ToolError> {
        for hook in &self.hooks {
            if hook.event != *event {
                continue;
            }
            if let Some(name) = tool_name {
                if !hook.match_tools.is_empty() && !hook.match_tools.iter().any(|m| m == name) {
                    continue;
                }
            }

            tracing::debug!(event = ?event, command = %hook.command, "firing hook");

            let mut cmd = Command::new("sh");
            cmd.arg("-c").arg(&hook.command);
            for (k, v) in context {
                cmd.env(format!("MC_{}", k.to_uppercase()), v);
            }

            let output = cmd.output().map_err(ToolError::Io)?;

            // PreToolCall hooks can block execution
            if *event == HookEvent::PreToolCall && !output.status.success() {
                return Err(ToolError::PermissionDenied(format!(
                    "hook '{}' blocked execution (exit {})",
                    hook.command,
                    output.status.code().unwrap_or(-1)
                )));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fires_matching_hooks() {
        let engine = HookEngine::new(vec![Hook {
            event: HookEvent::PostToolCall,
            command: "true".into(),
            match_tools: vec!["bash".into()],
        }]);
        assert!(engine
            .fire(&HookEvent::PostToolCall, Some("bash"), &[])
            .is_ok());
    }

    #[test]
    fn skips_non_matching_tools() {
        let engine = HookEngine::new(vec![Hook {
            event: HookEvent::PostToolCall,
            command: "false".into(), // would fail if run
            match_tools: vec!["bash".into()],
        }]);
        // Should skip because tool is "read_file", not "bash"
        assert!(engine
            .fire(&HookEvent::PostToolCall, Some("read_file"), &[])
            .is_ok());
    }

    #[test]
    fn pre_tool_call_blocks_on_failure() {
        let engine = HookEngine::new(vec![Hook {
            event: HookEvent::PreToolCall,
            command: "false".into(),
            match_tools: Vec::new(),
        }]);
        let result = engine.fire(&HookEvent::PreToolCall, Some("bash"), &[]);
        assert!(matches!(result, Err(ToolError::PermissionDenied(_))));
    }

    #[test]
    fn passes_context_env_vars() {
        let engine = HookEngine::new(vec![Hook {
            event: HookEvent::PostToolCall,
            command: "test -n \"$MC_TOOL_NAME\"".into(),
            match_tools: Vec::new(),
        }]);
        assert!(engine
            .fire(
                &HookEvent::PostToolCall,
                Some("bash"),
                &[("tool_name", "bash")]
            )
            .is_ok());
    }
}
