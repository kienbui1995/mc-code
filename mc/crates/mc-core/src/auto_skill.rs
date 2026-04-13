use std::path::Path;

/// Check if a completed turn should generate a skill.
/// Heuristic: if the turn used > threshold tool calls and succeeded,
/// it's a candidate for skill extraction.
#[must_use]
pub fn should_create_skill(tool_call_count: usize, had_errors: bool) -> bool {
    tool_call_count >= 6 && !had_errors
}

/// Generate a skill markdown file from a completed task.
#[must_use]
pub fn generate_skill_content(task_summary: &str, tools_used: &[String]) -> String {
    let tools_list = tools_used
        .iter()
        .map(|t| format!("- {t}"))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "# Auto-generated Skill\n\n\
         ## Task\n{task_summary}\n\n\
         ## Tools Used\n{tools_list}\n\n\
         ## Steps\n\
         (Agent will refine these steps on next use)\n\n\
         ---\n\
         *Auto-created by magic-code. Edit to improve.*\n"
    )
}

/// Save an auto-generated skill to the skills directory.
/// Returns the path if successful.
pub fn save_auto_skill(skills_dir: &Path, name: &str, content: &str) -> Option<String> {
    let dir = skills_dir.join("auto");
    if std::fs::create_dir_all(&dir).is_err() {
        return None;
    }
    let safe_name: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let path = dir.join(format!("{safe_name}.md"));
    if path.exists() {
        return None; // don't overwrite existing skills
    }
    std::fs::write(&path, content).ok()?;
    Some(path.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_threshold() {
        assert!(!should_create_skill(3, false));
        assert!(!should_create_skill(8, true));
        assert!(should_create_skill(6, false));
    }

    #[test]
    fn generate_content() {
        let content =
            generate_skill_content("Setup Next.js", &["bash".into(), "write_file".into()]);
        assert!(content.contains("Setup Next.js"));
        assert!(content.contains("- bash"));
    }

    #[test]
    fn save_skill() {
        let dir = std::env::temp_dir().join(format!("mc-skill-{}", std::process::id()));
        let path = save_auto_skill(&dir, "test-skill", "# Test");
        assert!(path.is_some());
        // Don't overwrite
        let path2 = save_auto_skill(&dir, "test-skill", "# Test 2");
        assert!(path2.is_none());
        std::fs::remove_dir_all(&dir).ok();
    }
}
