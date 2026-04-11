use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::process::Command;

use crate::error::ToolError;
use crate::spec::ToolSpec;

/// Discovers user-defined script tools from `.magic-code/tools/*.sh`.
/// Each script becomes a tool named after the file (without extension).
/// Scripts receive JSON input via stdin and should print output to stdout.
#[must_use]
/// Discover plugins.
pub fn discover_plugins(workspace: &Path) -> Vec<ToolSpec> {
    let dir = workspace.join(".magic-code/tools");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|ext| ext == "sh" || ext == "py" || ext == "js")
        })
        .filter_map(|e| {
            let name = e.path().file_stem()?.to_string_lossy().to_string();
            let desc = read_plugin_description(&e.path());
            Some(ToolSpec {
                name: format!("plugin_{name}"),
                description: desc,
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "input": { "type": "string", "description": "Input for the plugin script" }
                    },
                    "required": ["input"]
                }),
            })
        })
        .collect()
}

/// Execute a plugin script. Passes input via stdin, captures stdout.
/// Execute a user plugin script from `.magic-code/tools/`.
pub async fn execute_plugin(
    workspace: &Path,
    plugin_name: &str,
    input: &str,
) -> Result<String, ToolError> {
    let name = plugin_name.strip_prefix("plugin_").unwrap_or(plugin_name);
    let script = find_plugin_script(workspace, name)
        .ok_or_else(|| ToolError::NotFound(format!("plugin script not found: {name}")))?;

    let interpreter = if script.extension().is_some_and(|e| e == "py") {
        "python3"
    } else if script.extension().is_some_and(|e| e == "js") {
        "node"
    } else {
        "sh"
    };

    let output = tokio::time::timeout(Duration::from_secs(60), async {
        Command::new(interpreter)
            .arg(&script)
            .env("PLUGIN_INPUT", input)
            .output()
            .await
            .map_err(ToolError::Io)
    })
    .await
    .map_err(|_| ToolError::Timeout(60_000))??;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.success() {
        Ok(stdout.to_string())
    } else {
        Ok(format!("{stdout}\nSTDERR: {stderr}"))
    }
}

fn find_plugin_script(workspace: &Path, name: &str) -> Option<PathBuf> {
    let dir = workspace.join(".magic-code/tools");
    for ext in ["sh", "py", "js"] {
        let path = dir.join(format!("{name}.{ext}"));
        if path.exists() {
            return Some(path);
        }
    }
    None
}

/// Read first line comment as description (e.g. `# Description: my tool`).
fn read_plugin_description(path: &Path) -> String {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|content| {
            content.lines().find_map(|line| {
                line.strip_prefix("# ")
                    .or_else(|| line.strip_prefix("// "))
                    .map(String::from)
            })
        })
        .unwrap_or_else(|| format!("User plugin: {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_empty_dir() {
        let plugins = discover_plugins(Path::new("/nonexistent"));
        assert!(plugins.is_empty());
    }

    #[test]
    fn discover_all_script_types() {
        let dir = std::env::temp_dir().join(format!("mc-plugin-test-{}", std::process::id()));
        let tools = dir.join(".magic-code/tools");
        std::fs::create_dir_all(&tools).unwrap();
        std::fs::write(tools.join("a.sh"), "# Shell tool\necho hi").unwrap();
        std::fs::write(tools.join("b.py"), "# Python tool\nprint('hi')").unwrap();
        std::fs::write(tools.join("c.js"), "// JS tool\nconsole.log('hi')").unwrap();
        std::fs::write(tools.join("d.txt"), "not a plugin").unwrap();
        let plugins = discover_plugins(&dir);
        assert_eq!(plugins.len(), 3);
        let names: Vec<&str> = plugins.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"plugin_a"));
        assert!(names.contains(&"plugin_b"));
        assert!(names.contains(&"plugin_c"));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn reads_description_from_comment() {
        let dir = std::env::temp_dir().join(format!("mc-plugin-desc-{}", std::process::id()));
        let tools = dir.join(".magic-code/tools");
        std::fs::create_dir_all(&tools).unwrap();
        std::fs::write(tools.join("greet.sh"), "# Say hello to someone\necho hi").unwrap();
        let plugins = discover_plugins(&dir);
        assert_eq!(plugins.len(), 1);
        assert!(plugins[0].description.contains("Say hello"));
        std::fs::remove_dir_all(dir).ok();
    }

    #[tokio::test]
    async fn execute_sh_plugin() {
        let dir = std::env::temp_dir().join(format!("mc-plugin-exec-{}", std::process::id()));
        let tools = dir.join(".magic-code/tools");
        std::fs::create_dir_all(&tools).unwrap();
        std::fs::write(
            tools.join("echo.sh"),
            "#!/bin/sh\necho \"got: $PLUGIN_INPUT\"",
        )
        .unwrap();
        let result = execute_plugin(&dir, "plugin_echo", "test123")
            .await
            .unwrap();
        assert!(result.contains("got: test123"));
        std::fs::remove_dir_all(dir).ok();
    }

    #[tokio::test]
    async fn execute_missing_plugin() {
        let err = execute_plugin(Path::new("/tmp"), "plugin_nope", "hi")
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::NotFound(_)));
    }
}
