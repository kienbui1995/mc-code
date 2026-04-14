# Getting Started

## Install

### Quick install (Linux/macOS)
```bash
curl -fsSL https://raw.githubusercontent.com/kienbui1995/mc-code/main/install.sh | sh
```

### Build from source
```bash
git clone https://github.com/kienbui1995/mc-code.git
cd mc-code/mc
cargo install --path crates/mc-cli
```

## Setup

Set your API key:
```bash
export ANTHROPIC_API_KEY="your-key"
# or: OPENAI_API_KEY, GEMINI_API_KEY, GROQ_API_KEY, etc.
```

## First run

```bash
magic-code                          # interactive TUI
magic-code "fix the bug in auth.rs" # single-shot mode
echo "explain this" | magic-code --pipe  # pipe mode
```

## Key concepts

- **Tools**: Agent has 30 built-in tools (bash, file ops, search, browser, debug, etc.)
- **Memory**: Persistent project facts across sessions (`/memory`)
- **Skills**: Reusable coding patterns (`.magic-code/skills/*.md`)
- **Agents**: Named agent configs (`agents/*.md`)
- **Sessions**: Save/load/branch conversations

## Essential commands

| Command | Description |
|---------|-------------|
| `/help` | Show all commands (categorized) |
| `/model` | Switch LLM model |
| `/plan` | Toggle plan mode (think before acting) |
| `/save` | Save current session |
| `/undo` | Undo last file changes |
| `/cost` | Show session cost |
| `/compact` | Compress context when running low |
| `/debug` | Enter structured debugging mode |
| `/gh` | GitHub integration |

## Configuration

Create `.magic-code/config.toml` in your project:
```toml
[default]
model = "claude-sonnet-4-20250514"
max_tokens = 8192
provider = "anthropic"
notifications = true
```

See [Configuration Reference](reference/config.md) for all options.
