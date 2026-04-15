# magic-code

[![CI](https://github.com/kienbui1995/mc-code/actions/workflows/ci.yml/badge.svg)](https://github.com/kienbui1995/mc-code/actions/workflows/ci.yml)
[![Security](https://github.com/kienbui1995/mc-code/actions/workflows/security.yml/badge.svg)](https://github.com/kienbui1995/mc-code/actions/workflows/security.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Release](https://img.shields.io/github/v/release/kienbui1995/mc-code)](https://github.com/kienbui1995/mc-code/releases)
[![crates.io](https://img.shields.io/crates/v/magic-code.svg)](https://crates.io/crates/magic-code)

**Open-source TUI agentic AI coding agent.** Built in Rust. Fast. Multi-provider. Self-hostable.

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

### Quick install (Linux/macOS)
```bash
curl -fsSL https://raw.githubusercontent.com/kienbui1995/mc-code/main/install.sh | sh
```

### Via cargo
```bash
cargo install magic-code
```

### Download binary
Pre-built binaries for Linux (x86_64, aarch64) and macOS (x86_64, aarch64):
```bash
# Latest release
gh release download --repo kienbui1995/mc-code --pattern '*linux-x86_64*'
tar xzf magic-code-linux-x86_64.tar.gz
sudo mv magic-code /usr/local/bin/
```

### Build from source
```bash
git clone https://github.com/kienbui1995/mc-code.git
cd mc-code/mc
cargo install --path crates/mc-cli
```

### Setup
```bash
export ANTHROPIC_API_KEY="your-key"   # or OPENAI_API_KEY, GEMINI_API_KEY, etc.
magic-code                             # start TUI
magic-code "fix the bug in auth.rs"   # single-shot mode
```

## Features

### Multi-Provider
Works with **15 providers**: Anthropic, OpenAI, Gemini, Groq, DeepSeek, Mistral, xAI, OpenRouter, Together, Perplexity, Cohere, Cerebras, Ollama, LM Studio, llama.cpp. Switch mid-session with `/model`.

### 30 Built-in Tools
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
- `/cost` / `/cost --total` — per-turn breakdown and all-time cost tracking
- `/doctor` — check connectivity, config, API keys
- `/init` — project setup wizard
- `/export` — export conversation to markdown or JSON
- `/diff-preview` — approve/reject file changes with diff before writing
- `/auto-test` — auto-run tests after code changes, retry on failure
- `/search` — search across saved sessions
- `/summary` — session statistics
- Session save/load/resume
- Pipe mode (`echo "fix this" | magic-code`)
- Config layering: global → project → local
- Custom instructions (`.magic-code/instructions.md`)
- Pre/post tool call hooks
- MCP server support
- Plugin system (custom tools from scripts)

### Plugins

Create custom tools by adding scripts to `.magic-code/tools/`:

```bash
# .magic-code/tools/deploy.sh
# Deploy to staging environment
#!/bin/sh
echo "Deploying: $PLUGIN_INPUT"
./scripts/deploy.sh staging
```

```python
# .magic-code/tools/lint.py
# Run linter on a file
import os, subprocess
result = subprocess.run(["ruff", "check", os.environ["PLUGIN_INPUT"]], capture_output=True, text=True)
print(result.stdout or "No issues found")
```

Supports `.sh`, `.py`, `.js`. Auto-discovered on startup as `plugin_<name>` tools.

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
| `/export` | Export to markdown (`json` for JSON) |
| `/search <q>` | Search sessions |
| `/summary` | Session stats |
| `/plan` | Toggle plan mode |
| `/image` | Attach image |
| `/diff-preview` | Toggle diff approval for writes |
| `/auto-test` | Toggle auto-test after code changes |
| `/clear` | Clear output |
| `/init` | Project setup |
| `/doctor` | Health check |
| `/dry-run` | Toggle dry-run |

## Requirements

- Rust 1.75+ (build from source)
- API key for at least one provider (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, etc.)

## License

MIT

<!-- v1.2.0 -->
