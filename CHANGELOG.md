# Changelog

## v0.3.0 (unreleased)

### New Features
- **Streaming bash output** — stdout/stderr from bash tool execution now streams line-by-line to the TUI in real-time instead of waiting for the command to finish. Long-running commands (e.g. `cargo build`, `npm install`) show progress as it happens.

### Infrastructure
- New `BashTool::execute_streaming` method using piped stdout/stderr with `tokio::io::BufReader`
- New `ToolRegistry::execute_streaming` for tools that support incremental output
- `ProviderEvent::ToolOutputDelta` variant for forwarding tool output through the event pipeline
- `parallel_tools::execute_batch` accepts optional output sender for streaming
- Runtime uses `tokio::select!` to drain tool output concurrently during batch execution

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
