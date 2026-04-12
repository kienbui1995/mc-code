/// Structured debug mode — guides agent through hypothesis-driven debugging.
#[must_use]
pub fn execute_debug(input: &serde_json::Value) -> (String, bool) {
    let action = input.get("action").and_then(|v| v.as_str()).unwrap_or("");
    match action {
        "hypothesize" => {
            let bug = match input.get("bug_description").and_then(|v| v.as_str()) {
                Some(b) if !b.is_empty() => b,
                _ => return ("bug_description is required for hypothesize".into(), true),
            };
            (format!(
                "🔍 **Debug Mode — Hypothesis Generation**\n\n\
                 Bug: {bug}\n\n\
                 Generate 3-5 hypotheses about what could cause this bug.\n\
                 For each hypothesis:\n\
                 1. What could be wrong\n\
                 2. How to verify (what logging/instrumentation to add)\n\
                 3. Expected vs actual behavior if this hypothesis is correct\n\n\
                 Next step: Use `debug` with action='instrument' to add logging for each hypothesis."
            ), false)
        }
        "instrument" => {
            let file = input.get("file").and_then(|v| v.as_str()).unwrap_or("");
            let hypotheses = input
                .get("hypotheses")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join("\n- ")
                })
                .unwrap_or_default();
            if file.is_empty() {
                return ("file is required for instrument".into(), true);
            }
            (
                format!(
                    "🔧 **Debug Mode — Instrumentation**\n\n\
                 File: `{file}`\n\
                 Testing hypotheses:\n- {hypotheses}\n\n\
                 Add targeted logging/assertions to verify each hypothesis.\n\
                 Use `edit_file` to add:\n\
                 - Print/log statements at key decision points\n\
                 - Variable value dumps before/after suspect operations\n\
                 - Timing markers for race condition hypotheses\n\n\
                 After adding instrumentation, reproduce the bug and collect output.\n\
                 Then use `debug` with action='analyze' and the collected evidence."
                ),
                false,
            )
        }
        "analyze" => {
            let evidence = input.get("evidence").and_then(|v| v.as_str()).unwrap_or("");
            if evidence.is_empty() {
                return ("evidence is required for analyze".into(), true);
            }
            (
                format!(
                    "🧪 **Debug Mode — Evidence Analysis**\n\n\
                 Evidence collected:\n```\n{evidence}\n```\n\n\
                 Analyze the evidence against each hypothesis:\n\
                 1. Which hypotheses are confirmed/eliminated?\n\
                 2. What is the most likely root cause?\n\
                 3. Is more evidence needed?\n\n\
                 If root cause identified, use `debug` with action='fix'."
                ),
                false,
            )
        }
        "fix" => {
            let root_cause = input
                .get("root_cause")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let file = input.get("file").and_then(|v| v.as_str()).unwrap_or("");
            if root_cause.is_empty() {
                return ("root_cause is required for fix".into(), true);
            }
            if file.is_empty() {
                return ("file is required for fix".into(), true);
            }
            (
                format!(
                    "🔨 **Debug Mode — Targeted Fix**\n\n\
                 Root cause: {root_cause}\n\
                 File: `{file}`\n\n\
                 Apply a targeted fix:\n\
                 1. Fix the root cause\n\
                 2. Remove instrumentation/logging added during debug\n\
                 3. Add a regression test\n\
                 4. Verify the fix resolves the original bug"
                ),
                false,
            )
        }
        _ => (format!("Unknown debug action: {action}"), true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn hypothesize_returns_prompt() {
        let (out, err) =
            execute_debug(&json!({"action": "hypothesize", "bug_description": "crash on login"}));
        assert!(!err);
        assert!(out.contains("crash on login"));
    }

    #[test]
    fn hypothesize_requires_bug_description() {
        let (out, err) = execute_debug(&json!({"action": "hypothesize"}));
        assert!(err);
        assert!(out.contains("bug_description is required"));
    }

    #[test]
    fn instrument_requires_file() {
        let (out, err) = execute_debug(&json!({"action": "instrument"}));
        assert!(err);
        assert!(out.contains("file is required"));
    }

    #[test]
    fn analyze_requires_evidence() {
        let (out, err) = execute_debug(&json!({"action": "analyze"}));
        assert!(err);
        assert!(out.contains("evidence is required"));
    }

    #[test]
    fn fix_requires_root_cause_and_file() {
        let (_, err) = execute_debug(&json!({"action": "fix"}));
        assert!(err);
        let (_, err) = execute_debug(&json!({"action": "fix", "root_cause": "null ptr"}));
        assert!(err);
        let (out, err) =
            execute_debug(&json!({"action": "fix", "root_cause": "null ptr", "file": "main.rs"}));
        assert!(!err);
        assert!(out.contains("null ptr"));
    }
}
