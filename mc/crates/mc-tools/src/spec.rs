use serde_json::{json, Value};

#[derive(Debug, Clone)]
/// Toolspec.
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[allow(clippy::too_many_lines)]
/// All tool specs.
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
            description: "Delegate a task to an isolated subagent with its own context. Use for independent subtasks. Supports model routing, tool filtering, and background execution.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "task": { "type": "string", "description": "The task description for the subagent" },
                    "agent_name": { "type": "string", "description": "Optional: use a named agent (from agents/*.md) with its model/tools/instructions" },
                    "context": { "type": "string", "description": "Optional context to provide (file contents, specs, etc)" },
                    "model": { "type": "string", "description": "Optional model override (e.g. 'claude-haiku-4-5' for cheaper tasks)" },
                    "tools": { "type": "array", "items": { "type": "string" }, "description": "Optional: restrict which tools the agent can use (enforced at execution)" },
                    "max_turns": { "type": "integer", "description": "Optional: max turns for this agent (default 8)" },
                    "poll_agent_id": { "type": "string", "description": "Poll a background agent's result by ID (no task needed)" }
                }
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
        ToolSpec {
            name: "lsp_query".into(),
            description: "Query a Language Server for code intelligence. Supports: definition (go-to-def), references (find usages), hover (type info). Requires LSP server installed for the language.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "file": { "type": "string", "description": "File path" },
                    "line": { "type": "integer", "description": "Line number (0-based)" },
                    "column": { "type": "integer", "description": "Column number (0-based)" },
                    "method": { "type": "string", "enum": ["definition", "references", "hover"], "description": "Query type" }
                },
                "required": ["file", "line", "column", "method"]
            }),
        },
        ToolSpec {
            name: "task_create".into(),
            description: "Create a background task that runs a shell command asynchronously. Returns a task ID for polling.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "description": {"type": "string", "description": "What this task does"},
                    "command": {"type": "string", "description": "Shell command to run"}
                },
                "required": ["description", "command"]
            }),
        },
        ToolSpec {
            name: "task_get".into(),
            description: "Get the status and output of a background task by ID.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "task_id": {"type": "string", "description": "Task ID returned by task_create"}
                },
                "required": ["task_id"]
            }),
        },
        ToolSpec {
            name: "task_list".into(),
            description: "List all background tasks with their statuses.".into(),
            input_schema: json!({"type": "object", "properties": {}}),
        },
        ToolSpec {
            name: "batch_edit".into(),
            description: "Apply multiple file edits in a single call. More efficient than calling edit_file repeatedly. Aborts if any edit fails.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "edits": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "path": {"type": "string"},
                                "old_string": {"type": "string"},
                                "new_string": {"type": "string"},
                                "replace_all": {"type": "boolean"}
                            },
                            "required": ["path", "old_string", "new_string"]
                        },
                        "description": "Array of edits to apply"
                    }
                },
                "required": ["edits"]
            }),
        },
        ToolSpec {
            name: "apply_patch".into(),
            description: "Apply a unified diff patch (git diff format) to one or more files. Use when you want to express changes as a diff rather than string replacements.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "patch": {"type": "string", "description": "Unified diff patch text (git diff format)"}
                },
                "required": ["patch"]
            }),
        },
        ToolSpec {
            name: "task_stop".into(),
            description: "Stop a running background task.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "task_id": {"type": "string", "description": "Task ID to stop"}
                },
                "required": ["task_id"]
            }),
        },
        ToolSpec {
            name: "todo_write".into(),
            description: "Write or update the session TODO list. Use to track progress on multi-step tasks.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "todos": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": {"type": "string"},
                                "content": {"type": "string"},
                                "status": {"type": "string", "enum": ["pending", "in_progress", "completed"]},
                                "priority": {"type": "string", "enum": ["low", "medium", "high"]}
                            },
                            "required": ["id", "content", "status"]
                        },
                        "description": "Array of TODO items"
                    }
                },
                "required": ["todos"]
            }),
        },
        ToolSpec {
            name: "worktree_enter".into(),
            description: "Create a git worktree for a branch and switch the working directory to it. Use for isolated work on a separate branch.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "branch": {"type": "string", "description": "Branch name for the worktree"}
                },
                "required": ["branch"]
            }),
        },
        ToolSpec {
            name: "worktree_exit".into(),
            description: "Remove the current worktree and return to the main working directory.".into(),
            input_schema: serde_json::json!({"type": "object", "properties": {}}),
        },
        ToolSpec {
            name: "ask_user".into(),
            description: "Pause and ask the user a question when you need clarification. Use sparingly — prefer acting with best judgment.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "question": {"type": "string", "description": "Question to ask the user"}
                },
                "required": ["question"]
            }),
        },
        ToolSpec {
            name: "codebase_search".into(),
            description: "Search the codebase for symbols (functions, structs, classes) and files matching a query. Returns ranked results with file paths and matching symbols. Use this to find relevant code before reading files.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Search query — function names, class names, concepts (e.g. 'authentication handler', 'SubagentSpawner')"},
                    "max_results": {"type": "integer", "description": "Max results to return (default 10)"}
                },
                "required": ["query"]
            }),
        },
        ToolSpec {
            name: "edit_plan".into(),
            description: "Present a multi-file edit plan to the user for approval BEFORE making changes. Use this when modifying 2+ files to show the full scope of changes upfront. After approval, execute with edit_file/write_file/batch_edit.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "title": {"type": "string", "description": "Short title for the plan"},
                    "steps": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "file": {"type": "string", "description": "File path"},
                                "action": {"type": "string", "enum": ["create", "edit", "delete"], "description": "What to do"},
                                "description": {"type": "string", "description": "What changes and why"}
                            },
                            "required": ["file", "action", "description"]
                        },
                        "description": "Ordered list of file changes"
                    }
                },
                "required": ["title", "steps"]
            }),
        },
        ToolSpec {
            name: "sleep".into(),
            description: "Pause execution for a duration. Useful in polling loops or waiting for processes. Max 60 seconds.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "duration_ms": {"type": "integer", "description": "Milliseconds to sleep (max 60000)"}
                },
                "required": ["duration_ms"]
            }),
        },
        ToolSpec {
            name: "notebook_edit".into(),
            description: "Edit a Jupyter notebook cell. Operations: edit (replace cell source), insert (add new cell), delete (remove cell).".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "notebook_path": {"type": "string", "description": "Path to .ipynb file"},
                    "cell_index": {"type": "integer", "description": "0-indexed cell number"},
                    "operation": {"type": "string", "enum": ["edit", "insert", "delete"]},
                    "new_source": {"type": "string", "description": "New cell content (for edit/insert)"},
                    "cell_type": {"type": "string", "enum": ["code", "markdown", "raw"]}
                },
                "required": ["notebook_path", "operation"]
            }),
        },
        ToolSpec {
            name: "mcp_list_resources".into(),
            description: "List resources exposed by a connected MCP server.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "server_name": {"type": "string", "description": "MCP server name"}
                },
                "required": ["server_name"]
            }),
        },
        ToolSpec {
            name: "mcp_read_resource".into(),
            description: "Read a specific resource from an MCP server by URI.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "server_name": {"type": "string", "description": "MCP server name"},
                    "uri": {"type": "string", "description": "Resource URI"}
                },
                "required": ["server_name", "uri"]
            }),
        },
    ]
}
