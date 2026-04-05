# magic-code

Open-source TUI agentic AI coding agent. Built in Rust.

[![CI](https://github.com/kienbui1995/magic-code/actions/workflows/ci.yml/badge.svg)](https://github.com/kienbui1995/magic-code/actions/workflows/ci.yml)

[![demo](https://asciinema.org/a/placeholder.svg)](docs/demo.cast)

## Features

- **Multi-provider** — Anthropic, OpenAI, Gemini, Ollama, LiteLLM, any OpenAI-compatible endpoint
- **True streaming** — tokens render as they arrive, cancel anytime with Ctrl+C
- **Agentic** — LLM autonomously selects tools, executes them, iterates until done
- **9 built-in tools** — bash, read/write/edit file, glob, grep, subagent, memory read/write
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
# Homebrew (macOS/Linux)
brew tap kienbui1995/magic-code
brew install magic-code

# Cargo (crates.io)
cargo install magic-code

# From source
git clone https://github.com/kienbui1995/magic-code
cd magic-code/mc
cargo build --release
cp target/release/magic-code ~/.local/bin/

# Shell completions
magic-code --completions bash >> ~/.bashrc
magic-code --completions zsh >> ~/.zshrc
magic-code --completions fish > ~/.config/fish/completions/magic-code.fish
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
| `/undo` | Revert last turn's file changes |
| `/save <name>` | Save session |
| `/load <name>` | Load session |
| `/image <path>` | Attach image to next prompt |
| `/memory` | List project memory facts |
| `/thinking` | Toggle thinking display |
| `/fork` | Fork conversation at current point |
| `/branches` | List all branches |
| `/switch <name>` | Switch to branch |
| `/branch delete <name>` | Delete branch |

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
mc-core         Runtime, memory, undo, branching, parallel tools, caching.
mc-provider     LLM providers. Anthropic, Gemini, OpenAI-compat. Prompt caching, thinking, images.
mc-tools        Tool execution. Async bash, file ops, MCP, permissions.
mc-config       TOML config. Layered merge, project context discovery.
```

## License

MIT
