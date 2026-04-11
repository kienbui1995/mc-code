use std::path::Path;

/// A named agent definition loaded from `agents/*.md`.
#[derive(Debug, Clone)]
pub struct AgentDef {
    pub name: String,
    pub model: Option<String>,
    pub description: String,
    pub instructions: String,
    pub allowed_tools: Vec<String>,
}

/// Discover agent definitions from `.magic-code/agents/` and `agents/` directories.
#[must_use]
pub fn discover_agents(workspace: &Path) -> Vec<AgentDef> {
    let mut agents = Vec::new();
    for dir in [
        workspace.join(".magic-code/agents"),
        workspace.join("agents"),
    ] {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "md") {
                    if let Some(agent) = parse_agent_file(&path) {
                        agents.push(agent);
                    }
                }
            }
        }
    }
    agents
}

fn parse_agent_file(path: &Path) -> Option<AgentDef> {
    let content = std::fs::read_to_string(path).ok()?;
    let name = path.file_stem()?.to_string_lossy().to_string();
    let (frontmatter, body) = split_frontmatter(&content);

    Some(AgentDef {
        name,
        model: extract_field(&frontmatter, "model"),
        description: extract_field(&frontmatter, "description").unwrap_or_default(),
        instructions: body.to_string(),
        allowed_tools: extract_list(&frontmatter, "tools"),
    })
}

fn split_frontmatter(content: &str) -> (String, &str) {
    if content.starts_with("---") {
        if let Some(end) = content[3..].find("---") {
            let fm = content[3..3 + end].to_string();
            let body = &content[3 + end + 3..];
            return (fm, body.trim_start());
        }
    }
    (String::new(), content)
}

fn extract_field(frontmatter: &str, key: &str) -> Option<String> {
    for line in frontmatter.lines() {
        if let Some(rest) = line.strip_prefix(&format!("{key}:")) {
            let val = rest.trim().trim_matches('"').trim_matches('\'');
            if !val.is_empty() {
                return Some(val.to_string());
            }
        }
    }
    None
}

fn extract_list(frontmatter: &str, key: &str) -> Vec<String> {
    let mut in_list = false;
    let mut items = Vec::new();
    for line in frontmatter.lines() {
        if line.starts_with(&format!("{key}:")) {
            in_list = true;
            continue;
        }
        if in_list {
            if let Some(item) = line.trim().strip_prefix("- ") {
                items.push(item.trim().to_string());
            } else {
                break;
            }
        }
    }
    items
}

/// Format agents as a system prompt section.
#[must_use]
pub fn agents_prompt_section(agents: &[AgentDef]) -> String {
    if agents.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "\n## Named Agents\nUse subagent tool with these agent names for specialized tasks:\n",
    );
    for a in agents {
        out.push_str(&format!("- **{}**", a.name));
        if let Some(ref m) = a.model {
            out.push_str(&format!(" (model: {m})"));
        }
        if !a.description.is_empty() {
            out.push_str(&format!(": {}", a.description));
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_agent_md() {
        let dir = std::env::temp_dir().join(format!("mc-agents-{}", std::process::id()));
        let agents_dir = dir.join(".magic-code/agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::write(
            agents_dir.join("reviewer.md"),
            "---\nmodel: claude-haiku-4-5\ndescription: Code review agent\ntools:\n- read_file\n- grep_search\n---\nReview code for bugs and style issues.",
        ).unwrap();
        let agents = discover_agents(&dir);
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].name, "reviewer");
        assert_eq!(agents[0].model.as_deref(), Some("claude-haiku-4-5"));
        assert_eq!(agents[0].allowed_tools.len(), 2);
        assert!(agents[0].instructions.contains("Review code"));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn discover_empty() {
        let agents = discover_agents(Path::new("/nonexistent"));
        assert!(agents.is_empty());
    }

    #[test]
    fn agents_prompt_section_formats() {
        let agents = vec![AgentDef {
            name: "tester".into(),
            model: Some("haiku".into()),
            description: "Runs tests".into(),
            instructions: String::new(),
            allowed_tools: vec![],
        }];
        let section = agents_prompt_section(&agents);
        assert!(section.contains("**tester**"));
        assert!(section.contains("haiku"));
    }
}
