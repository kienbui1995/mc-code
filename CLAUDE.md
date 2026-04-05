# CLAUDE.md — Project Instructions for AI Agents

## Project

magic-code: open-source TUI agentic AI coding agent. Built in Rust.

## Repository Layout

```
mc/                          ← Rust workspace root (run cargo commands here)
  crates/
    mc-cli/                  ← Binary. CLI parsing, provider dispatch.
    mc-tui/                  ← TUI. ratatui + crossterm. Markdown, syntax highlighting.
    mc-core/                 ← Conversation runtime, 8 feature modules (see below).
    mc-provider/             ← LLM providers. Anthropic, Gemini, OpenAI-compatible.
    mc-tools/                ← Tool execution. Async bash, file ops, MCP, permissions.
    mc-config/               ← TOML config. Layered merge, project context discovery.
src/                         ← Legacy Python source (reference only, do not modify)
rust/                        ← Legacy Rust experiments (reference only)
tests/                       ← Python integration tests (legacy)
```

## mc-core Modules

```
runtime.rs          — ConversationRuntime: main orchestrator, run_turn loop
session.rs          — Session, Block (Text/ToolUse/ToolResult/Image/Thinking), save/load
compact.rs          — Token estimation, naive + LLM-based context compaction
model_registry.rs   — Model metadata (context window, costs, tool support)
subagent.rs         — Isolated subagent conversations
usage.rs            — Token usage tracking + cache stats
token_budget.rs     — Dynamic max_tokens based on context usage
retry.rs            — Mid-stream retry with exponential backoff
memory.rs           — Persistent project facts (.magic-code/memory.json)
context_resolver.rs — @path file mentions in user input
undo.rs             — Per-turn file change rollback
parallel_tools.rs   — Concurrent tool execution with semaphore
tool_cache.rs       — TTL cache for read-only tools (glob/grep/read_file)
branch.rs           — Conversation fork/switch/list/delete
```

## Crate Dependency Rules

```
mc-cli → mc-tui, mc-core, mc-provider, mc-tools, mc-config
mc-tui → (standalone, no mc-* deps)
mc-core → mc-provider, mc-tools, mc-config
mc-provider → (standalone)
mc-tools → (standalone)
mc-config → (standalone)
```

- `mc-provider` and `mc-tools` must NEVER depend on each other.
- `mc-tui` must NEVER depend on mc-core/mc-provider/mc-tools.
- Only `mc-core` orchestrates provider + tools together.
- Memory tools dispatched in mc-core (not mc-tools) to avoid circular deps.

## Build & Test

```bash
cd mc
cargo test --workspace          # 123 tests
cargo clippy --workspace --all-targets  # zero warnings required
cargo fmt --all -- --check      # format check
cargo build --release           # release build
just check                      # runs fmt + clippy + test
```

CI enforces: `RUSTFLAGS=-Dwarnings` — all warnings are errors.

## Coding Conventions

### Rust Style
- Edition 2021, MSRV 1.75
- `unsafe` is **forbidden** (`unsafe_code = "forbid"` in workspace lints)
- Clippy pedantic enabled — follow all suggestions
- `#[must_use]` on all public functions returning values
- Use `thiserror` for error types, `anyhow` only in binary crate
- Prefer `impl Into<String>` over `&str` for constructor params
- All async code uses `tokio` runtime
- Shared state in runtime uses `Arc<T>` (hook_engine, audit_log, tool_registry)

### Naming
- Crate names: `mc-{name}` (kebab-case)
- Module files: `snake_case.rs`
- Types: `PascalCase`
- Functions/methods: `snake_case`
- Constants: `SCREAMING_SNAKE_CASE`

### Error Handling
- Each crate defines its own error enum with `thiserror`
- `mc-provider`: `ProviderError` (has `is_retryable()`)
- `mc-tools`: `ToolError`
- `mc-config`: `ConfigError`
- Never `unwrap()` in library code. `unwrap()` only in tests.

### Testing
- Unit tests: `#[cfg(test)] mod tests` at bottom of each file
- Integration tests: `crates/mc-core/tests/integration.rs`
- Use `MockProvider` (in integration.rs) for testing runtime behavior
- Test names: `snake_case` describing behavior
- Temp files: use atomic counter for unique paths (avoid parallel test collisions)

## Architecture Patterns

### Adding a New Tool
1. Implementation: `mc-tools/src/{tool}.rs`
2. Spec: add `ToolSpec` in `mc-tools/src/spec.rs`
3. Dispatch: add match arm in `mc-tools/src/registry.rs`
4. Export: update `mc-tools/src/lib.rs`
5. If tool needs mc-core state (like memory): dispatch in `runtime.rs` instead

### Adding a New Provider
1. Implementation: `mc-provider/src/{provider}.rs`
2. Export: update `mc-provider/src/lib.rs`
3. `LlmProvider` impl: add in `mc-core/src/runtime.rs`
4. CLI dispatch: add in `mc-cli/src/main.rs`
5. Image support: add wire conversion in provider's `to_wire_content()`

### Adding a New Core Feature
1. New module: `mc-core/src/{feature}.rs`
2. Integrate into `ConversationRuntime` in `runtime.rs` (add field + setter)
3. Export from `mc-core/src/lib.rs`
4. Config: add to `mc-config/src/types.rs` if configurable
5. TUI: add slash command in `mc-tui/src/app.rs`, wire in `mc-cli/src/main.rs`

### Key Types
- `ConversationRuntime` — main orchestrator, owns session + tools + provider dispatch
- `Session` — message history (with optional branch metadata), serializable to JSON
- `Block` — content unit: Text, ToolUse, ToolResult, Image, Thinking
- `ContentBlock` — provider-level content (always base64 for images)
- `ProviderEvent` — streaming events: TextDelta, ThinkingDelta, ToolUse, Usage, MessageStop, RetryAttempt, StreamReset
- `ToolRegistry` — tool dispatch with sandbox, timeout, MCP support (Arc-wrapped)
- `PermissionPolicy` — controls which tools can execute
- `TokenBudget` — dynamic max_tokens based on context window usage
- `MemoryStore` — persistent key-value facts for cross-session context
- `UndoManager` — per-turn file snapshots for rollback
- `BranchManager` — conversation fork/switch/list/delete

### Tool Execution Flow
1. LLM returns tool_use blocks
2. Runtime splits: subagent/memory → sequential, rest → parallel batch
3. Parallel batch: `parallel_tools::execute_batch()` with semaphore (max 4)
4. Each tool: pre-hook → permission check → cache check → execute → cache store → audit → post-hook
5. Write tools (write_file/edit_file): undo snapshot before execution, cache invalidation after
6. Results stored as tool_result messages in session

## Do / Don't

### Do
- Run `cargo clippy` before committing
- Keep crate boundaries clean
- Write tests for new functionality
- Use `tracing::debug!` / `tracing::warn!` for logging
- Handle cancellation via `CancellationToken` in async loops
- Serialize new Session fields with `#[serde(default)]` for backward compat
- Use `Arc<T>` for shared state that needs concurrent access

### Don't
- Don't add `unsafe` code
- Don't add dependencies without checking workspace `Cargo.toml` first
- Don't put business logic in mc-tui (it's presentation only)
- Don't make mc-provider depend on mc-tools or vice versa
- Don't use `println!` — use `tracing` macros
- Don't modify files in `src/` or `rust/` (legacy, reference only)
- Don't dispatch memory/subagent tools through mc-tools (circular dep risk)
