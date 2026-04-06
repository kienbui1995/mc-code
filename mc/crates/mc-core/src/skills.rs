use std::path::Path;

/// A loaded skill from `.magic-code/skills/SKILL.md`.
#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub allowed_tools: Vec<String>,
    pub model: Option<String>,
    pub content: String,
}

/// Discover skills from `.magic-code/skills/` directory.
#[must_use]
pub fn discover_skills(workspace: &Path) -> Vec<Skill> {
    let dir = workspace.join(".magic-code/skills");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| e.file_name().to_string_lossy() == "SKILL.md" || e.path().is_dir())
        .filter_map(|e| {
            if e.path().is_dir() {
                let skill_file = e.path().join("SKILL.md");
                parse_skill(&skill_file)
            } else {
                parse_skill(&e.path())
            }
        })
        .collect()
}

fn parse_skill(path: &Path) -> Option<Skill> {
    let content = std::fs::read_to_string(path).ok()?;
    let (frontmatter, body) = split_frontmatter(&content);

    let name = extract_field(&frontmatter, "name").unwrap_or_else(|| {
        path.parent()
            .and_then(|p| p.file_name())
            .map_or("unnamed".into(), |n| n.to_string_lossy().into())
    });
    let description = extract_field(&frontmatter, "description").unwrap_or_default();
    let allowed_tools = extract_field(&frontmatter, "allowed-tools")
        .map(|s| s.split(',').map(|t| t.trim().to_string()).collect())
        .unwrap_or_default();
    let model = extract_field(&frontmatter, "model");

    Some(Skill {
        name,
        description,
        allowed_tools,
        model,
        content: body.to_string(),
    })
}

fn split_frontmatter(content: &str) -> (String, &str) {
    if let Some(rest) = content.strip_prefix("---") {
        if let Some(end) = rest.find("---") {
            let fm = rest[..end].to_string();
            let body = &rest[end + 3..];
            return (fm, body.trim_start());
        }
    }
    (String::new(), content)
}

fn extract_field(frontmatter: &str, key: &str) -> Option<String> {
    for line in frontmatter.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(key) {
            if let Some(value) = rest.strip_prefix(':') {
                return Some(value.trim().trim_matches('"').to_string());
            }
        }
    }
    None
}

/// Build skill descriptions for the system prompt (metadata only, not full content).
#[must_use]
pub fn skills_prompt_section(skills: &[Skill]) -> String {
    if skills.is_empty() {
        return String::new();
    }
    let mut section = String::from("\n\n## Available Skills\nUse the `skill` tool to activate a skill when the task matches.\n");
    for skill in skills {
        section.push_str(&format!("- **{}**: {}\n", skill.name, skill.description));
    }
    section
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_frontmatter() {
        let content = "---\nname: review\ndescription: Code review\nallowed-tools: bash, read_file\n---\n# Review\nDo a code review.";
        let skill = parse_skill(std::path::Path::new("test")).unwrap_or_else(|| {
            let (fm, body) = split_frontmatter(content);
            Skill {
                name: extract_field(&fm, "name").unwrap_or_default(),
                description: extract_field(&fm, "description").unwrap_or_default(),
                allowed_tools: extract_field(&fm, "allowed-tools")
                    .map(|s| s.split(',').map(|t| t.trim().to_string()).collect())
                    .unwrap_or_default(),
                model: None,
                content: body.to_string(),
            }
        });
        assert_eq!(skill.name, "review");
        assert_eq!(skill.description, "Code review");
        assert_eq!(skill.allowed_tools, vec!["bash", "read_file"]);
    }

    #[test]
    fn empty_dir() {
        let skills = discover_skills(Path::new("/nonexistent"));
        assert!(skills.is_empty());
    }
}
