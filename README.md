# magic-code

**Open-source TUI agentic AI coding agent.** Built in Rust. Fast. Multi-provider.

```
┌─ magic-code ████████░░ 62% ──────────────────────────────┐
│                                                           │
│ › Fix the failing test in src/auth.rs                     │
│                                                           │
│   ⚙ tool: grep_search                                    │
│   ⚙ tool: read_file                                      │
│   ⚙ tool: edit_file                                      │
│   ⚙ tool: bash (streaming output)                        │
│     running tests...                                      │
│     test auth::test_login ... ok                          │
│     test auth::test_token ... ok                          │
│                                                           │
│ Fixed. The issue was a missing lifetime bound on line 42. │
│ Tests pass now: 47/47 ✓                                   │
│                                                           │
├───────────────────────────────────────────────────────────┤
│ Input                                                     │
├───────────────────────────────────────────────────────────┤
│ claude-sonnet │ ctx ████████░░ 62% │ 12K↓ 2K↑ $0.066     │
└───────────────────────────────────────────────────────────┘
```

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/kienbui1995/mc-code/main/install.sh | sh
```

Or build from source:
```bash
git clone https://github.com/kienbui1995/mc-code.git
cd mc-code/mc
cargo install --path crates/mc-cli
```

## Features

### Multi-Provider
Works with **15 providers**: Anthropic, OpenAI, Gemini, Groq, DeepSeek, Mistral, xAI, OpenRouter, Together, Perplexity, Cohere, Cerebras, Ollama, LM Studio, llama.cpp. Switch mid-session with `/model`.

### 26 Built-in Tools
| Tool | Description |
|------|-------------|
| `bash` | Execute shell commands (streaming output) |
| `read_file` | Read files with offset/limit |
| `write_file` | Create or overwrite files |
| `edit_file` | Surgical text replacement with diff preview |
| `batch_edit` | Multiple file edits in one call |
| `apply_patch` | Apply unified diff patches |
| `glob_search` | Find files by pattern |
| `grep_search` | Search file contents with regex |
| `subagent` | Delegate tasks to isolated sub-conversations |
| `memory_read` | Read persistent project facts |
| `memory_write` | Save facts across sessions |
| `web_fetch` | Fetch URL content (streaming) |
| `web_search` | Search DuckDuckGo |
| `lsp_query` | Code intelligence (go-to-def, references) |
| `task_create` | Spawn background shell commands |
| `task_get/list/stop` | Manage background tasks |
| `todo_write` | LLM-managed TODO list |
| `ask_user` | Ask user for clarification |
| `sleep` | Pause execution |
| `notebook_edit` | Edit Jupyter notebook cells |
| `worktree_enter/exit` | Isolated git branch work |
| `mcp_list/read_resources` | MCP server resources |

### TUI
- Syntax highlighting (syntect)
- Markdown rendering
- Scroll (PageUp/PageDown, mouse wheel)
- Input history (persisted)
- Tab completion for slash commands
- Context window usage bar
- Permission prompts for destructive operations

### Intelligence
- True async streaming (tokens render as they arrive)
- LLM-based smart context compaction
- Extended thinking / reasoning blocks
- Image support (`/image`)
- Long-term memory (persists across sessions)
- `@file` mentions (auto-read file content)
- Conversation branching (fork/switch)
- Parallel tool execution (up to 4 concurrent)
- Tool result caching (30s TTL for read-only tools)
- Prompt caching (Anthropic, up to 90% cost savings)
- Dynamic token budget
- Mid-stream retry with exponential backoff

### Git Integration
- `/diff` — show changes
- `/commit` — LLM-generated commit messages
- `/log` — recent history
- `/stash` / `/stash pop`

### Safety
- Permission modes: read-only, workspace-write, full-access
- File protection (`.env`, `*.key`, `.git/*` + configurable)
- Workspace sandboxing
- Audit log for all tool executions
- Dry-run mode
- Undo/rollback (`/undo`)

### Developer Experience
- `/model` — switch provider/model mid-session
- `/cost` / `/cost --total` — session and all-time cost tracking
- `/doctor` — check connectivity, config, API keys
- `/init` — project setup wizard
- `/export` — export conversation to markdown
- `/search` — search across saved sessions
- `/summary` — session statistics
- Session save/load/resume
- Pipe mode (`echo "fix this" | magic-code`)
- Config layering: global → project → local
- Custom instructions (`.magic-code/instructions.md`)
- Pre/post tool call hooks
- MCP server support

## Configuration

```bash
magic-code --init  # or /init in TUI
```

Creates `.magic-code/config.toml`:
```toml
model = "claude-sonnet-4-20250514"
provider = "anthropic"
permission_mode = "workspace-write"

[model_aliases]
fast = "claude-haiku"
smart = "claude-sonnet-4-20250514"
```

Global config: `~/.config/magic-code/config.toml`

## Slash Commands

| Command | Description |
|---------|-------------|
| `/help` | Show all commands |
| `/quit` | Exit |
| `/status` | Session info |
| `/cost` | Session cost (`--total` for all-time) |
| `/model <name>` | Switch model |
| `/diff` | Git diff |
| `/commit` | Auto-commit with LLM message |
| `/log` | Git log |
| `/stash` | Git stash (`pop` to restore) |
| `/compact` | Compress context |
| `/undo` | Revert last turn's file changes |
| `/save <name>` | Save session |
| `/load <name>` | Load session |
| `/export` | Export to markdown |
| `/search <q>` | Search sessions |
| `/summary` | Session stats |
| `/plan` | Toggle plan mode |
| `/image` | Attach image |
| `/clear` | Clear output |
| `/init` | Project setup |
| `/doctor` | Health check |
| `/dry-run` | Toggle dry-run |

## Requirements

- Rust 1.75+ (build from source)
- API key for at least one provider (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, etc.)

## License

MIT

<!-- v1.1.0 -->
