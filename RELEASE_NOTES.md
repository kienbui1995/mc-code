# What's New

## Version 0.7.0

### 🎉 Highlights
- **44+ new features** since v0.2.0
- **11 tools** (was 9): added `web_fetch`, `web_search`
- **141 tests** (was 123)
- **Pure Rust repo** — removed all legacy Python/Rust code (-14,858 lines)
- **8.1MB binary**, 1ms startup

### 🔧 v0.3.0 — Streaming & UX
- Streaming bash output (real-time stdout/stderr)
- Cost tracking persistence (`/cost --total`)
- Mouse scroll support
- Tab completion for slash commands

### 🌐 v0.4.0 — Web & Git
- `web_fetch` / `web_search` tools
- Git integration: `/diff`, `/commit` (LLM-generated messages), `/log`, `/stash`
- `/model` switch mid-session + model aliases
- Context window usage bar in status
- `/clear`, `/export`, `/summary`, `/search`
- `/init` project wizard + custom instructions (`.magic-code/instructions.md`)
- File protection (`.env`, `*.key`, `.git/*`)
- Dry-run mode

### 🚀 v0.5.0 — Polish
- LLM-generated commit messages
- Provider fallback config
- Session auto-save every 5 turns
- Terminal bell on completion
- `/doctor` health check

### 🧩 v0.6.0 — Productivity
- `/template` (review, refactor, test, explain, document, optimize, security)
- Plugin system (`.magic-code/tools/*.sh`)
- `/review` file changes
- Latency metrics (TTFT + total in status bar)
- `/retry`, `/pin`, `/theme`

### 🏗️ v0.7.0 — Architecture
- `/copy`, `/version`, `/history`, `/tokens`, `/context`, `/alias`
- Context window preflight (auto-compact before oversized requests)
- `--json` output mode for automation
- Error IDs (MC-E001 through MC-E006)
- System prompt v2 (tool guidelines, error recovery)
- `PendingCommand` enum (replaced 20 boolean flags)
- `AgentState` enum (Idle/Streaming/ToolExecuting/WaitingPermission)
- MCP timeout + health check
- ADR-003 permission model documentation
- 4 new integration tests
