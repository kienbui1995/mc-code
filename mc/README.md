# magic-code

Open-source TUI agentic AI coding agent. Built in Rust.

## Features

- **Multi-provider** — Anthropic, OpenAI, Gemini, Ollama, LiteLLM, any OpenAI-compatible endpoint
- **True streaming** — tokens render as they arrive, cancel anytime with Ctrl+C
- **Agentic** — LLM autonomously selects tools, executes them, iterates until done
- **7 built-in tools** — bash, read/write/edit file, glob, grep, subagent delegation
- **TUI** — real terminal app with syntax highlighting, markdown rendering, scroll, input history
- **MCP support** — connect external tool servers via config
- **Smart compaction** — LLM-based context summarization when approaching token limits
- **Permission control** — read-only / workspace-write / full-access modes
- **Audit log** — every tool execution logged to `~/.local/share/magic-code/audit.jsonl`
- **Session management** — save, load, resume sessions across restarts
- **Cost tracking** — live cost estimate in status bar, `/cost` command
- **Hooks** — pre/post tool call hooks for custom automation

## Install

```bash
# From source
git clone https://github.com/kienbui1995/magic-code
cd magic-code/mc
cargo build --release
cp target/release/magic-code ~/.local/bin/
```

## Usage

```bash
# Interactive TUI
magic-code

# Single prompt
magic-code "explain this codebase"

# Pipe mode
cat error.log | magic-code "fix this error"
echo "list all TODOs" | magic-code > todos.md

# Resume last session
magic-code --resume

# Choose provider
magic-code --provider openai --model gpt-4o "hello"
magic-code --provider gemini --model gemini-2.5-flash "hello"
magic-code --provider ollama --model llama3 "hello"
magic-code --provider litellm --model gpt-4o "hello"
magic-code --base-url http://localhost:4000 --model my-model "hello"

# Verbose
magic-code -v "hello"
```

## TUI Keybindings

| Key | Action |
|-----|--------|
| `Enter` | Submit prompt |
| `Shift+Enter` | Newline in input |
| `Ctrl+C` | Cancel current turn / Quit if idle |
| `PageUp/PageDown` | Scroll output |
| `Up/Down` | Input history |
| `Ctrl+U` | Clear input line |
| `Ctrl+W` | Delete word |

## Slash Commands

| Command | Description |
|---------|-------------|
| `/help` | Show commands |
| `/quit` | Exit |
| `/status` | Model, tokens, mode |
| `/cost` | Session cost estimate |
| `/plan` | Toggle plan mode (think, don't execute) |
| `/compact` | Force context compaction |
| `/save <name>` | Save session |
| `/load <name>` | Load session |

## Configuration

Global: `~/.config/magic-code/config.toml`
Project: `.magic-code/config.toml`
Local: `.magic-code/config.local.toml` (gitignored)

```toml
[default]
provider = "anthropic"
model = "claude-sonnet-4-20250514"
max_tokens = 8192
permission_mode = "workspace-write"  # read-only | workspace-write | full-access

[providers.anthropic]
api_key_env = "ANTHROPIC_API_KEY"

[providers.litellm]
base_url = "http://localhost:4000"
api_key_env = "LITELLM_API_KEY"
format = "openai-compatible"

[providers.ollama]
base_url = "http://localhost:11434"

[context]
instruction_files = ["MAGIC_CODE.md"]

[compaction]
auto_compact_threshold = 0.8
preserve_recent_messages = 4

# MCP servers
[[mcp_servers]]
name = "github"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]

[mcp_servers.env]
GITHUB_TOKEN_ENV = "GITHUB_TOKEN"

# Hooks
[[hooks]]
event = "pre_tool_call"
command = "echo $MC_TOOL_NAME >> /tmp/mc-audit.log"
match_tools = ["bash"]
```

## Architecture

```
mc-cli          Binary. CLI parsing, provider dispatch.
mc-tui          TUI. ratatui + crossterm. Markdown, syntax highlighting.
mc-core         Conversation runtime. Streaming, compaction, subagents.
mc-provider     LLM providers. Anthropic, Gemini, OpenAI-compatible.
mc-tools        Tool execution. Async bash, file ops, MCP, permissions.
mc-config       TOML config. Layered merge, project context discovery.
```

## License

MIT
