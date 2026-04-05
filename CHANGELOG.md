# Changelog

## v0.5.0 (unreleased)

### New Features
- **LLM-generated commit messages** — `/commit` sends staged diff to LLM for a conventional commit message
- **Provider fallback** — `fallback_provider` / `fallback_model` config fields for automatic failover
- **Session auto-save** — automatically saves session every 5 turns
- **Terminal bell** — `\x07` bell on turn completion (audible notification)
- **`/doctor` command** — check version, provider, API key, git, config validity
- **README rewrite** — comprehensive documentation of all features (v0.1–v0.5)

### Infrastructure
- `ConversationRuntime::generate_commit_message()` for LLM-powered git commits
- `RuntimeConfig` gains `fallback_provider`, `fallback_model` fields
- Turn counter with periodic auto-save to `~/.local/share/magic-code/sessions/last.json`
- 135 tests

## v0.4.0 (unreleased)

### New Features — Tools
- **`web_fetch` tool** — LLM can fetch URL content (HTML stripped to plain text)
- **`web_search` tool** — search DuckDuckGo instant answers (no API key required)

### New Features — Git Integration
- **`/diff`** — show `git diff` in TUI
- **`/commit`** — auto-commit staged changes
- **`/log`** — show recent git log (last 10, oneline)
- **`/stash` / `/stash pop`** — quick git stash management

### New Features — Provider & Model
- **`/model` switch** — change model mid-session (`/model gpt-4o`)
- **Model aliases** — `model_aliases` in config, e.g. `fast = "claude-haiku"`

### New Features — TUI & UX
- **Context window bar** — visual progress bar in status bar showing % context used (green/yellow/red)
- **`/clear`** — clear output area, preserve session history
- **`/export`** — export conversation to markdown file
- **`/summary`** — show session statistics
- **`/search`** — search across saved sessions by keyword

### New Features — Configuration & Safety
- **`/init` wizard** — create `.magic-code/config.toml` + `instructions.md` for project
- **Custom instructions** — `.magic-code/instructions.md` auto-injected into system prompt
- **File protection** — built-in patterns (`.env`, `*.key`, `.git/*`) + configurable `protected_patterns`
- **Dry-run mode** — `/dry-run` toggles showing tool calls without executing

### Infrastructure
- 11 tools (was 9): added `web_fetch`, `web_search`
- 135 tests (was 132)
- `reqwest` added to mc-tools for HTTP fetching
- `Sandbox` now enforces file protection patterns
- `CostTracker` persists usage, `ModelRegistry.set_model()` for mid-session switch

## v0.3.0 (unreleased)

### New Features
- **Streaming bash output** — stdout/stderr from bash tool execution now streams line-by-line to the TUI in real-time instead of waiting for the command to finish. Long-running commands (e.g. `cargo build`, `npm install`) show progress as it happens.
- **Cost tracking persistence** — per-turn usage persisted to `~/.local/share/magic-code/usage.jsonl`. `/cost --total` shows cumulative cost across all sessions.
- **Mouse scroll support** — scroll wheel up/down in TUI output area
- **Tab completion for slash commands** — press Tab to auto-complete `/he` → `/help`, or show matching options

### Infrastructure
- New `BashTool::execute_streaming` method using piped stdout/stderr with `tokio::io::BufReader`
- New `ToolRegistry::execute_streaming` for tools that support incremental output
- `ProviderEvent::ToolOutputDelta` variant for forwarding tool output through the event pipeline
- `parallel_tools::execute_batch` accepts optional output sender for streaming
- Runtime uses `tokio::select!` to drain tool output concurrently during batch execution
- New `CostTracker` in mc-core for persistent usage tracking
- 132 tests (was 123)

## v0.2.0 (unreleased)

### New Features
- **Extended thinking** — Anthropic thinking/reasoning blocks streamed and stored in session
- **Image support** — Send images to LLM via `/image` command (Anthropic, OpenAI, Gemini)
- **Long-term memory** — Persistent project facts in `.magic-code/memory.json`, LLM can read/write via `memory_read`/`memory_write` tools
- **@-mentions** — `@src/main.rs fix this` auto-reads file content into prompt
- **Undo/rollback** — `/undo` reverts last turn's file changes (per-turn snapshots, max 10)
- **Conversation branching** — Fork, switch, list, delete conversation branches
- **Parallel tool execution** — Multiple tools run concurrently (semaphore, max 4)
- **Tool result caching** — Read-only tools (glob, grep, read_file) cached with 30s TTL

### Improvements
- **Prompt caching** — Anthropic `cache_control` on system prompt + tool definitions (up to 90% input cost savings)
- **Dynamic token budget** — `max_tokens` auto-adjusts based on context window usage
- **Mid-stream retry** — Exponential backoff on stream failures with `StreamReset` + `RetryAttempt` events
- **Context pressure warning** — Logs warning when session history exceeds 90% of available context

### Infrastructure
- 9 tools (was 7): added `memory_read`, `memory_write`
- 123 tests (was 82)
- Runtime internals use `Arc<T>` for concurrent tool execution
- Session backward compatible: new branch fields use `#[serde(default)]`
- New config sections: `[retry]`, `[memory]`, `[thinking]`

## v0.1.0 (2025-04-01)

Initial release.

### Features
- Multi-provider support: Anthropic, OpenAI, Gemini, Ollama, LiteLLM, any OpenAI-compatible
- True async streaming — tokens render as they arrive
- 7 built-in tools: bash, read_file, write_file, edit_file, glob_search, grep_search, subagent
- TUI with syntax highlighting (syntect), markdown rendering, scroll, input history
- MCP server support via TOML config
- LLM-based smart context compaction
- Permission modes: read-only, workspace-write, full-access
- Audit log for all tool executions
- Session save/load/resume
- Cost tracking with live status bar estimate
- Pre/post tool call hooks
- Workspace sandboxing
- Cancel with Ctrl+C (per-turn, not app-level)
- Pipe mode for non-interactive use
- Config layering: global → project → local
- Project context auto-discovery (git status, stack detection, instruction files)
