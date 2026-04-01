# magic-code

**Open-source TUI agentic AI coding agent.** Built in Rust. Fast. Multi-provider.

```
┌─ magic-code ──────────────────────────────────────────────┐
│                                                           │
│ › Fix the failing test in src/auth.rs                     │
│                                                           │
│   ⚙ tool: grep_search                                    │
│   ⚙ tool: read_file                                      │
│   ⚙ tool: edit_file                                      │
│   ⚙ tool: bash                                           │
│                                                           │
│ Fixed. The issue was a missing lifetime bound on line 42. │
│ Tests pass now: 47/47 ✓                                   │
│                                                           │
├───────────────────────────────────────────────────────────┤
│ Input                                                     │
├───────────────────────────────────────────────────────────┤
│ claude-sonnet-4-20250514 │ 12K↓ 2K↑ $0.066 │ ready       │
└───────────────────────────────────────────────────────────┘
```

## Install

```bash
# One-line install
curl -fsSL https://raw.githubusercontent.com/kienbui1995/magic-code/main/install.sh | sh

# Or with Cargo
cargo install magic-code

# Or build from source
git clone https://github.com/kienbui1995/magic-code
cd magic-code/mc && cargo build --release
```

## Quick Start

```bash
# Set your API key
export ANTHROPIC_API_KEY="sk-..."

# Interactive TUI
magic-code

# One-shot
magic-code "explain this codebase"

# Pipe
cat error.log | magic-code "fix this"
```

## Providers

Works with any LLM that supports tool calling:

| Provider | Setup |
|----------|-------|
| **Anthropic** | `export ANTHROPIC_API_KEY=...` |
| **OpenAI** | `--provider openai` + `export OPENAI_API_KEY=...` |
| **Gemini** | `--provider gemini` + `export GEMINI_API_KEY=...` |
| **Ollama** | `--provider ollama` (local, free) |
| **LiteLLM** | `--provider litellm --base-url http://...` |
| **Any OpenAI-compatible** | `--base-url http://your-endpoint` |

## Features

- **True streaming** — tokens render as they arrive
- **7 tools** — bash, read/write/edit file, glob, grep, subagent
- **TUI** — syntax highlighting, markdown, scroll, input history
- **MCP** — connect external tool servers
- **Smart compaction** — LLM summarizes context when approaching limits
- **Permissions** — read-only / workspace-write / full-access
- **Audit log** — every tool call logged
- **Sessions** — save, load, resume
- **Cost tracking** — live estimate in status bar

## Configuration

```bash
# Project config (committed)
.magic-code/config.toml

# Local overrides (gitignored)
.magic-code/config.local.toml

# Global
~/.config/magic-code/config.toml
```

```toml
[default]
provider = "anthropic"
model = "claude-sonnet-4-20250514"
permission_mode = "workspace-write"

[[mcp_servers]]
name = "github"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
```

## Keybindings

| Key | Action |
|-----|--------|
| `Enter` | Submit |
| `Ctrl+C` | Cancel / Quit |
| `PageUp/Down` | Scroll |
| `Up/Down` | History |
| `/help` | Commands |
| `/cost` | Session cost |
| `/plan` | Plan mode (think, don't execute) |

## Architecture

```
mc-cli       → Binary, CLI dispatch
mc-tui       → TUI (ratatui, syntect)
mc-core      → Runtime, compaction, subagents
mc-provider  → Anthropic, Gemini, OpenAI-compat
mc-tools     → Async tools, MCP, permissions
mc-config    → TOML config, project context
```

82 tests. 5,600 LOC Rust. Zero unsafe.

## License

MIT
