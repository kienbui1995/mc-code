use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[allow(clippy::too_many_lines)]
pub fn all_tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "bash".into(),
            description: "Execute a shell command.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string" },
                    "timeout": { "type": "integer", "minimum": 1 }
                },
                "required": ["command"]
            }),
        },
        ToolSpec {
            name: "read_file".into(),
            description: "Read a text file.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "offset": { "type": "integer", "minimum": 0 },
                    "limit": { "type": "integer", "minimum": 1 }
                },
                "required": ["path"]
            }),
        },
        ToolSpec {
            name: "write_file".into(),
            description: "Write content to a file.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["path", "content"]
            }),
        },
        ToolSpec {
            name: "edit_file".into(),
            description: "Replace text in a file. Returns a diff preview.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "old_string": { "type": "string" },
                    "new_string": { "type": "string" },
                    "replace_all": { "type": "boolean" }
                },
                "required": ["path", "old_string", "new_string"]
            }),
        },
        ToolSpec {
            name: "glob_search".into(),
            description: "Find files matching a glob pattern.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string" },
                    "path": { "type": "string" }
                },
                "required": ["pattern"]
            }),
        },
        ToolSpec {
            name: "grep_search".into(),
            description: "Search file contents with a regex pattern.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string" },
                    "path": { "type": "string" },
                    "glob": { "type": "string" }
                },
                "required": ["pattern"]
            }),
        },
        ToolSpec {
            name: "subagent".into(),
            description: "Delegate a task to an isolated subagent with its own context. Use for independent subtasks that don't need the current conversation history.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "task": { "type": "string", "description": "The task description for the subagent" },
                    "context": { "type": "string", "description": "Optional context to provide (file contents, specs, etc)" }
                },
                "required": ["task"]
            }),
        },
        ToolSpec {
            name: "memory_read".into(),
            description: "Read project facts from long-term memory. Omit key to list all facts.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "key": { "type": "string", "description": "Fact key to read. Omit to list all." }
                }
            }),
        },
        ToolSpec {
            name: "memory_write".into(),
            description: "Save a project fact to long-term memory (persists across sessions). Set delete=true to remove.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "key": { "type": "string", "description": "Fact key" },
                    "value": { "type": "string", "description": "Fact value" },
                    "delete": { "type": "boolean", "description": "Set true to delete this key" }
                },
                "required": ["key"]
            }),
        },
        ToolSpec {
            name: "web_fetch".into(),
            description: "Fetch content from a URL. Returns plain text (HTML tags stripped). Use for reading documentation, API specs, web pages.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "URL to fetch" }
                },
                "required": ["url"]
            }),
        },
        ToolSpec {
            name: "web_search".into(),
            description: "Search the web using DuckDuckGo. Returns instant answers and related topics. Use when you need to look up current information.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query" }
                },
                "required": ["query"]
            }),
        },
    ]
}
