use std::path::Path;

/// A loaded skill from `.magic-code/skills/SKILL.md`.
#[derive(Debug, Clone)]
/// Skill.
pub struct Skill {
    pub name: String,
    pub description: String,
    pub allowed_tools: Vec<String>,
    pub model: Option<String>,
    pub content: String,
}

/// Discover skills from `.magic-code/skills/` and `.magic-code/plugins/*/skills/`.
#[must_use]
/// Discover skills.
pub fn discover_skills(workspace: &Path) -> Vec<Skill> {
    let mut skills = discover_from_dir(&workspace.join(".magic-code/skills"));
    // Also scan plugins
    let plugins_dir = workspace.join(".magic-code/plugins");
    if let Ok(entries) = std::fs::read_dir(&plugins_dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                skills.extend(discover_from_dir(&entry.path().join("skills")));
            }
        }
    }
    skills
}

fn discover_from_dir(dir: &Path) -> Vec<Skill> {
    let Ok(entries) = std::fs::read_dir(dir) else {
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
/// Skills prompt section.
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

    #[test]
    fn discover_with_valid_skill() {
        let dir = std::env::temp_dir().join(format!("mc-skill-{}", std::process::id()));
        let skill_dir = dir.join(".magic-code/skills/greet");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "---\nname: greet\n---\nSay hello").unwrap();
        let skills = discover_skills(&dir);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "greet");
        std::fs::remove_dir_all(dir).ok();
    }
