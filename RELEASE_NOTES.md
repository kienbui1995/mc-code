# What's New

## Version 1.1.0

### 🎉 Highlights
- **26 tools** (was 12): ask_user, sleep, notebook_edit, task system, worktree, batch_edit, apply_patch, todo_write, MCP resources
- **15 providers** (was 3): Groq, DeepSeek, Mistral, xAI, OpenRouter, Together, Perplexity, Cohere, Cerebras, LM Studio, llama.cpp
- **152 tests**, all passing
- **13.6K lines** of pure Rust

### 🔐 Safety & Budget
- `--max-budget-usd` — stop when cost exceeds limit
- `--max-turns` — stop after N model turns
- `--max-tokens-total` — stop after N total tokens
- Read-before-write enforcement — blocks writes to unread files
- Model whitelist/blacklist per provider

### 🛠️ New Tools
| Tool | Description |
|------|-------------|
| `task_create` | Spawn background shell commands |
| `task_get/list/stop` | Poll, list, terminate background tasks |
| `batch_edit` | Multiple file edits in one call |
| `apply_patch` | Apply unified diff patches |
| `todo_write` | LLM-managed TODO list |
| `ask_user` | Pause and ask user for clarification |
| `sleep` | Pause execution (polling loops) |
| `notebook_edit` | Edit Jupyter notebook cells |
| `worktree_enter/exit` | Isolated git branch work |
| `mcp_list_resources` | List MCP server resources |
| `mcp_read_resource` | Read MCP server resources |

### 🌐 Provider Expansion
- `/connect` wizard — guided setup with API key URLs
- `/providers` — list all providers with config status
- Auto-detect provider from model name
- Interactive `/model` picker with pricing info

### 📋 New Commands
| Command | Description |
|---------|-------------|
| `/connect` | Provider setup wizard |
| `/providers` | List configured providers |
| `/security-review` | Security audit prompt |
| `/resume` | Resume sessions with fuzzy search |
| `/tasks` | Manage background tasks |
| `/agents` | Manage sub-agents |
| `/cron` | Scheduled triggers |
| `/update` | Check for new version |

### 🏗️ Architecture
- Commands module extracted (app.rs -60%)
- All slash commands non-blocking (async RunShell)
- Hierarchical AGENTS.md/CLAUDE.md loading (root→cwd)
- @include directive in instruction files
- `--add-dir` for extra directory access
- Config hot-reload via mtime polling
- MCP auto-reconnect on server crash
- 9 command aliases (/h, /?, /q, /exit, /new, /reset, /continue, /v, /settings)
