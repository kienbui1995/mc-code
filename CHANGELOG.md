# Changelog

## v0.8.3 (2026-04-09)

### Bug Fixes
- **Fix UTF-8 panic in micro_compact** — byte-slicing long tool outputs could panic on multibyte characters; now uses `char_indices` for safe boundaries
- **Fix duplicate undo snapshot** — `dispatch_tool` was snapshotting files twice before write operations
- **Fix MCP server hang** — `recv()` had no timeout; MCP servers that stop responding would hang magic-code forever; now 30s timeout on all MCP reads
- **Fix bash classification bypass** — command splitting on `&|;` chars broke quoted strings (e.g. `echo "a;b"`); now uses quote-aware shell parsing
- **Fix auto_save_memory key collision** — two facts saved within the same second would overwrite each other; now uses millisecond + content-length key
- **Fix /env API key leak** — was showing first 4 + last 4 chars of keys; now only shows `...xxxx` (last 4)
- **Fix plugin .py interpreter** — Python plugins were run via `sh` instead of `python3`
- **Fix /run blocking TUI** — `/run` command used blocking `std::process::Command`, freezing the entire TUI; now routes through async `PendingCommand`

### Performance
- **Cache `all_specs()` with `OnceLock`** — tool specs were re-allocated every LLM iteration; now cached and returned as `&[ToolSpec]`
- **Reuse `reqwest::Client`** — `web_fetch` and `web_search` created a new HTTP client per call; now share a static client with connection pooling
- **`CostTracker` in-memory cache** — `/cost --total` read and parsed the entire `usage.jsonl` file every call; now caches running totals in memory
- **`UndoManager` uses `VecDeque`** — evicting oldest turn was O(n) with `Vec::remove(0)`; now O(1)
- **`BranchManager` atomic counter** — `next_id()` was scanning the entire branches directory; now uses `AtomicUsize`

### Improvements
- **Output lines capped at 10K** — prevents unbounded memory growth in long sessions
- **Auto-continue heuristic hardened** — added `}`, `)`, `]`, `;` as end-of-content markers to reduce false positives
- **Renamed `now_iso()` → `epoch_secs()`** — function name now matches what it returns

### Infrastructure
- 146 tests (all pass)
- 17 fixes across 12 files

## v0.9.0 (Roadmap)

### Planned — Stability & Architecture
- **Async slash commands** — migrate remaining ~12 blocking commands (`/grep`, `/cat`, `/head`, `/tail`, `/files`, `/tree`, `/wc`, `/todo`, `/recent`, `/test`, `/ship`, `/open`) from `std::process::Command` to async `PendingCommand` pattern
- **Split `handle_slash_command`** — extract 900-line, 40+ arm match into dedicated `commands` module with command registry pattern
- **Replace hand-rolled `base64_encode`** — use `base64` crate instead of custom implementation
- **Async image loading** — `resolve_image_base64` currently does blocking file I/O in `build_request`; move to `tokio::task::spawn_blocking`
- **MCP reconnection** — auto-reconnect to MCP servers after disconnect/crash
- **HTML parser upgrade** — `strip_html` breaks on `<script>`/`<style>` tags; use proper tag-aware stripping

### Planned — Features
- **Streaming tool output for all providers** — currently only bash streams; extend to web_fetch, plugins
- **Session search improvements** — full-text search across session content, not just filenames
- **Config hot-reload** — watch `.magic-code/config.toml` for changes without restart

## v0.8.2 (2026-04-06)

### New Features
- **Auto-continue** — if LLM output appears truncated by token limit, automatically sends "continue"
- **Large result persistence** — tool outputs exceeding 100KB saved to temp file with preview + path reference
- **Bash security hardening** — deep command classification with compound command support, dangerous pattern detection

### Infrastructure
- 398 doc comments, 100% public API coverage
- CI: sandbox testing, pre-release workflow, smoke tests

## v0.8.1

### New Features
- **Effort levels** — `/effort low|medium|high` controls thinking budget (○ none, ◐ 10K, ● 32K tokens)

## v0.8.0

### New Features — Claude Code Architectural Patterns
- **`PendingCommand` enum** — replaced 20+ boolean flags with typed command queue
- **`AgentState` enum** — `Idle`/`Streaming`/`ToolExecuting`/`WaitingPermission` state machine
- **Context window preflight** — auto-compact before oversized requests
- **`--json` output mode** — structured output for automation/scripting
- **Error IDs** — `MC-E001` through `MC-E006` for categorized errors
- **System prompt v2** — tool guidelines, error recovery instructions
- **MCP timeout + health check** — 60s tool call timeout, `is_alive()` check
- **ADR-003** — permission model documentation

## v0.7.6

### New Features — Claude Code Feature Parity
- **`/run`** — execute shell commands directly from TUI
- **`/grep`** — search codebase from TUI
- **`/vim`** — toggle vim keybindings (Esc=Normal, i=Insert)
- **`/spec`** — generate technical specification before coding
- **`/config`** — show current runtime configuration
- **`/add`** — add file/directory content to next prompt
- **`/sessions`** — list and delete saved sessions
- **`/permissions`** — show and toggle permission modes
- **Transcript mode** — show raw conversation
- **Custom commands** — `.magic-code/commands/*.md` auto-discovered

## v0.7.5

### New Features
- **Deep bash permission classification** — safe/dangerous/needs-review with compound command support
- **`/config` command** — show runtime config
- **`/add` command** — add file content to input
- **`/sessions` command** — list/delete saved sessions
- **`/spec` command** — generate spec before coding

## v0.7.4

### New Features
- **`/todo`** — find all TODO/FIXME/HACK in codebase
- **`/recent`** — show recently modified files
- **`/ship`** — git add all + LLM commit message (one command)
- **`/test`** — auto-detect test runner and run tests

## v0.7.3

### New Features
- **`/tree`** — directory tree with depth control
- **`/head`** / **`/tail`** — view file start/end
- **`/pwd`** — show current directory
- **`/env`** — show environment variables (keys masked)
- **`/size`** — show file size

## v0.7.2

### New Features
- **`/files`** — list directory contents
- **`/cat`** — view file contents
- **`/models`** — list known models
- **`/wc`** — count lines of code in workspace

## v0.7.1

### New Features
- **`/tip`** — random productivity tip
- **`/time`** — session elapsed time
- **`/whoami`** — show current config summary
- **`/last`** — show last tool output
- **`/open`** — open file in $EDITOR

## v0.7.0 (2026-04-06)

### New Features
- **`/copy`** — copy last assistant response to system clipboard (pbcopy/xclip)
- **`/version`** — show version, OS, architecture
- **`/history`** — display last 20 input history entries
- **`/tokens`** — detailed token breakdown (estimated, session, context window usage %)
- **`/context`** — show what's in context window (messages, tools, system prompt, estimated tokens)
- **`/alias`** — define custom command aliases (`/alias r review` → `/r` expands to `review`)
- **Graceful shutdown** — session auto-saved on exit via panic hook + normal exit path

### Infrastructure
- `InputHistory::entries()` public accessor for history display
- Binary already optimized: LTO + strip + codegen-units=1 in release profile
- 137 tests

## v0.6.0

### New Features
- **`/template` command** — pre-built prompts: review, refactor, test, explain, document, optimize, security
- **Plugin system** — user scripts in `.magic-code/tools/*.sh` auto-discovered as tools (`plugin_*`)
- **`/review`** — show all file changes (git diff HEAD) for review
- **`/retry`** — re-submit last user input
- **`/pin`** — pin important messages to survive compaction
- **`/theme`** — toggle dark/light theme
- **Latency metrics** — time-to-first-token + total turn time displayed in status bar
- **Terminal bell** — audible notification on turn completion

### Infrastructure
- Plugin discovery: `mc-tools/src/plugin.rs` scans `.magic-code/tools/` for `.sh`/`.py` scripts
- `UiMessage::Done` now carries `ttft_ms` and `total_ms` latency data
- `ConversationRuntime::generate_commit_message()` uses LLM for conventional commit messages
- 137 tests (was 135)

## v0.5.0

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

## v0.4.0

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

## v0.3.0

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

## v0.2.0 (2026-04-05)

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
