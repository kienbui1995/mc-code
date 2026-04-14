# Tools Reference

magic-code has 30 built-in tools organized in 5 categories.

## Core Tools

| Tool | Description |
|------|-------------|
| `bash` | Execute shell commands (streaming output) |
| `read_file` | Read files with optional offset/limit |
| `write_file` | Create or overwrite files |
| `edit_file` | Replace specific text (surgical edits) |
| `batch_edit` | Multiple edits to one file atomically |
| `apply_patch` | Apply unified diff patches |
| `glob_search` | Find files by pattern |
| `grep_search` | Search file contents with regex |
| `codebase_search` | Symbol-aware code search (tree-sitter) |

## Planning & Delegation

| Tool | Description |
|------|-------------|
| `edit_plan` | Multi-file edit plan before execution |
| `subagent` | Delegate tasks to isolated sub-agents |
| `task_create` | Start background commands |
| `task_get` | Check background task status |
| `task_list` | List all background tasks |
| `task_stop` | Stop a background task |
| `todo_write` | Write structured TODO lists |

## Debugging & Testing

| Tool | Description |
|------|-------------|
| `debug` | Structured debugging (hypothesize → instrument → analyze → fix) |
| `browser` | Headless browser (navigate, screenshot, click, type, eval JS) |
| `lsp_query` | Language Server queries (diagnostics, definitions) |

## Context & Memory

| Tool | Description |
|------|-------------|
| `memory_read` | Read persistent project facts |
| `memory_write` | Save facts (categories: project, user, feedback, reference) |
| `web_fetch` | Fetch URL content |
| `web_search` | Search the web |
| `ask_user` | Ask clarifying questions |

## Workspace

| Tool | Description |
|------|-------------|
| `worktree_enter` | Create isolated git worktree |
| `worktree_exit` | Exit and clean up worktree |
| `notebook_edit` | Edit Jupyter notebook cells |
| `sleep` | Wait for specified duration |
| `mcp_list_resources` | List MCP server resources |
| `mcp_read_resource` | Read MCP resource by URI |
