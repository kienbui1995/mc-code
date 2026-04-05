use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionMode {
    Allow,
    Deny,
    Prompt,
}

#[derive(Debug, Clone)]
pub struct PermissionRequest {
    pub tool_name: String,
    pub input_summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionOutcome {
    Allow,
    Deny { reason: String },
}

pub trait PermissionPrompter: Send {
    fn decide(&mut self, request: &PermissionRequest) -> PermissionOutcome;
}

#[derive(Clone)]
pub struct PermissionPolicy {
    default_mode: PermissionMode,
    tool_modes: BTreeMap<String, PermissionMode>,
}

impl PermissionPolicy {
    #[must_use]
    pub fn new(default_mode: PermissionMode) -> Self {
        Self {
            default_mode,
            tool_modes: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn with_tool_mode(mut self, tool: impl Into<String>, mode: PermissionMode) -> Self {
        self.tool_modes.insert(tool.into(), mode);
        self
    }

    #[must_use]
    pub fn authorize(
        &self,
        tool_name: &str,
        input_summary: &str,
        prompter: Option<&mut dyn PermissionPrompter>,
    ) -> PermissionOutcome {
        let mode = self
            .tool_modes
            .get(tool_name)
            .copied()
            .unwrap_or(self.default_mode);
        match mode {
            PermissionMode::Allow => PermissionOutcome::Allow,
            PermissionMode::Deny => PermissionOutcome::Deny {
                reason: format!("tool '{tool_name}' denied by policy"),
            },
            PermissionMode::Prompt => match prompter {
                Some(p) => p.decide(&PermissionRequest {
                    tool_name: tool_name.to_string(),
                    input_summary: input_summary.to_string(),
                }),
                None => PermissionOutcome::Deny {
                    reason: format!("tool '{tool_name}' requires interactive approval"),
                },
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct AllowAll;
    impl PermissionPrompter for AllowAll {
        fn decide(&mut self, _: &PermissionRequest) -> PermissionOutcome {
            PermissionOutcome::Allow
        }
    }

    #[test]
    fn allow_mode_passes() {
        let policy = PermissionPolicy::new(PermissionMode::Allow);
        assert_eq!(
            policy.authorize("bash", "ls", None),
            PermissionOutcome::Allow
        );
    }

    #[test]
    fn deny_mode_blocks() {
        let policy = PermissionPolicy::new(PermissionMode::Deny);
        assert!(matches!(
            policy.authorize("bash", "rm -rf", None),
            PermissionOutcome::Deny { .. }
        ));
    }

    #[test]
    fn prompt_mode_delegates() {
        let policy = PermissionPolicy::new(PermissionMode::Prompt);
        assert_eq!(
            policy.authorize("bash", "ls", Some(&mut AllowAll)),
            PermissionOutcome::Allow
        );
    }

    #[test]
    fn tool_override() {
        let policy = PermissionPolicy::new(PermissionMode::Allow)
            .with_tool_mode("bash", PermissionMode::Deny);
        assert!(matches!(
            policy.authorize("bash", "ls", None),
            PermissionOutcome::Deny { .. }
        ));
        assert_eq!(
            policy.authorize("read_file", "x", None),
            PermissionOutcome::Allow
        );
    }
}
