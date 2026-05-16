# CLAUDE.md — Project Instructions for AI Agents

## Project

magic-code: open-source TUI agentic AI coding agent. Built in Rust.

## Current Version: v1.9.1

### Release History

- v0.1.0: Initial release (multi-provider, streaming, 7 tools, TUI, MCP, permissions)
- v0.2.0: Extended thinking, image support, long-term memory, @-mentions, undo/rollback, conversation branching, parallel tools, tool caching, prompt caching, dynamic token budget, mid-stream retry, context pressure warning. 11 tools, 141 tests.
- v0.7.0: Streaming bash output (line-by-line to TUI). 141 tests.
- v1.0.0: Production-ready. 15 providers, safety controls, background tasks, hierarchical memory. 152 tests.
- v1.1.0: 26 tools, interactive model picker, git worktree, cron triggers, config hot-reload. 152 tests.
- v1.2.0: /cost breakdown, /export, /diff-preview, plugin system, streaming edit preview, /auto-test, MCP in single-shot mode. 162 tests.
- v1.3.0: /auto-commit, README docs update. 180 tests.
- v1.4.0: Plugin marketplace, subagent model routing + shared context, --yes CI mode, per-tool permissions, --validate-config. 180 tests.
- v1.4.1: Named agents (`agents/*.md` with YAML frontmatter), `--trace` structured logging, audit timestamps. Security: `--yes` no longer bypasses bash.
- v1.4.2: Session branching wired to BranchManager, JSON output completeness (cost, cache tokens), token budget config.
- v1.5.0: Managed agents initial — manager-executor architecture, tool filtering per agent, background agent polling.
- v1.6.0: `codebase_search` (TF-IDF symbol-aware), `edit_plan`, auto-verify (syntax check after writes), FallbackProvider, auto-permission learning within session, smart compaction (importance scoring), structured output (ResponseFormat::Json). 28 tools, 192 tests.
- v1.7.0: Memory System v2 — categories (project/user/feedback/reference), self-skeptical prompt, dream cleanup/dedup, auto-compact on start. System prompt hardening: all 30 tools described, security section, cost-awareness rules. 30 tools.
- v1.7.1: musl static binary for Linux x86_64 (GLIBC compat), SonarCloud + CodeQL integration.
- v1.8.0: Headless NDJSON streaming, `--batch`/`--pipe` with shared session across turns, selective tool injection per turn, adaptive compaction, model-tier prompt profiles (tiers 1–4), golden test suite (154 scenarios × 22 categories), crates.io publishing, `mc-core` renamed to `mc-code-core` on crates.io.
- v1.9.1: Stream-level provider fallback, `/raw` command (view response in plain terminal), GitHub Models free tier (GPT-4o, Llama 405B), Tier 5 minimal prompt (5 tools for token-limited providers), 350-point golden test suite (154 core + 120 platform + 76 session turns), Qwen 3.5 27B promoted to tier 2, TUI text selection (disabled mouse capture). 238 tests.

## Repository Layout

```
mc-code/                     ← Repo root
  mc/                        ← Rust workspace root (run cargo commands here)
    crates/
      mc-cli/                ← Binary. CLI parsing, provider dispatch.
      mc-tui/                ← TUI. ratatui + crossterm. Markdown, syntax highlighting.
      mc-core/               ← Conversation runtime, 23 feature modules. (crates.io: mc-code-core)
      mc-provider/           ← LLM providers. Anthropic, Gemini, OpenAI-compatible.
      mc-tools/              ← Tool execution. Async bash, file ops, MCP, permissions.
      mc-config/             ← TOML config. Layered merge, project context discovery.
    Cargo.toml               ← Workspace root (version = "1.9.1")
    rust-toolchain.toml      ← Pins stable Rust + clippy/rustfmt
    .rustfmt.toml            ← max_width=100, field_init_shorthand=true
    deny.toml                ← License compliance + security advisory checks
    justfile                 ← Dev task runner
  docs/                      ← Documentation site (blog, guides, ADRs)
  examples/                  ← Example scripts (batch-review.sh, ci-fix.sh, demo.sh)
  tests/                     ← Golden test suite (scenarios/, golden/)
  CHANGELOG.md               ← Full release history
  CONTRIBUTING.md
  install.sh
```

## mc-core Modules (23)

```
runtime.rs          — ConversationRuntime: main orchestrator, run_turn loop
session.rs          — Session, Block (Text/ToolUse/ToolResult/Image/Thinking), save/load
compact.rs          — Token estimation, naive + LLM-based context compaction, smart_compact
parallel_tools.rs   — Concurrent tool execution with semaphore (max 4)
subagent.rs         — Isolated subagent conversations, SharedContext
memory.rs           — Persistent project facts (.magic-code/memory.json), MemoryStore
token_budget.rs     — Dynamic max_tokens based on context usage
retry.rs            — Mid-stream retry with exponential backoff
context_resolver.rs — @path file mentions in user input
undo.rs             — Per-turn file change rollback, UndoManager
tool_cache.rs       — TTL cache for read-only tools (glob/grep/read_file)
branch.rs           — Conversation fork/switch/list/delete, BranchManager
model_registry.rs   — Model metadata (context window, costs, tool support, tier)
usage.rs            — Token usage tracking + cache stats
cost.rs             — Cost tracking per turn and session
agents.rs           — Named agent discovery (agents/*.md with YAML frontmatter)
auto_skill.rs       — Auto-skill discovery and activation
skills.rs           — Skill discovery and system prompt injection
cron.rs             — Cron trigger management, CronManager
tasks.rs            — Background task management (task_create/get/list/stop)
repo_map.rs         — Repository structure analysis, symbol-aware code retrieval
fts.rs              — Full-text search over session history (Session.search())
debug.rs            — Debug utilities and /debug command
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
cargo test --workspace          # 238 tests
cargo clippy --workspace --all-targets  # zero warnings required
cargo fmt --all -- --check      # format check
cargo build --release           # release build
just check                      # runs fmt + clippy + test (what CI does)
just pre-release                # fmt + clippy + test + build + size report
just smoke-test                 # quick validation in a temp sandbox
just sandbox                    # build and run in Docker sandbox
```

CI enforces: `RUSTFLAGS=-Dwarnings` — all warnings are errors.
`cargo deny check` enforces license compliance and security advisories (deny.toml).

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
- max_width = 100 (enforced by .rustfmt.toml)

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
- Golden test suite: `tests/scenarios/` — JSON scenarios evaluated by a parallel runner

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
- `ProviderEvent` — streaming events: TextDelta, ThinkingDelta, ToolUse, Usage, MessageStop, RetryAttempt, StreamReset, ToolOutputDelta, ToolInputDelta
- `ToolRegistry` — tool dispatch with sandbox, timeout, MCP support (Arc-wrapped)
- `PermissionPolicy` — controls which tools can execute
- `TokenBudget` — dynamic max_tokens based on context window usage
- `MemoryStore` — persistent key-value facts with categories (project/user/feedback/reference)
- `UndoManager` — per-turn file snapshots for rollback
- `BranchManager` — conversation fork/switch/list/delete
- `CostTracker` — per-turn and cumulative cost tracking
- `RepoMap` — symbol-aware repository structure for context injection
- `TaskManager` — background task lifecycle (create/get/list/stop)

### Tool Execution Flow
1. LLM returns tool_use blocks
2. Runtime splits: subagent/memory → sequential, rest → parallel batch
3. Parallel batch: `parallel_tools::execute_batch()` with semaphore (max 4), optional streaming output sender
4. Each tool: pre-hook → permission check → cache check → execute (streaming for bash) → cache store → audit → post-hook
5. Write tools (write_file/edit_file): undo snapshot before execution, auto-verify syntax after, cache invalidation
6. Streaming bash output: chunks forwarded via `ProviderEvent::ToolOutputDelta` → TUI renders in real-time
7. Results stored as tool_result messages in session

### FallbackProvider Pattern
- `RuntimeConfig` has `fallback_provider` / `fallback_model` fields
- On non-retryable provider error or stream failure, runtime switches to fallback
- Stream-level fallback: mid-stream errors trigger retry on fallback provider
- Auto-permission learning: approved tool patterns remembered within session

### Model Tier System
Model capabilities are classified into tiers for prompt adaptation:
- **Tier 1** (Claude Opus/Sonnet, GPT-4o, Gemini Pro): all 30 tools, full system prompt
- **Tier 2** (27B+ self-hosted, e.g. Qwen 3.5 27B): 25 tools (adds edit_plan, batch_edit, subagent, debug)
- **Tier 3** (mid self-hosted): ~15 tools, reduced prompt complexity
- **Tier 4** (9B self-hosted, e.g. Qwen 3.5 9B): 10 tools, simplified prompt with explicit rules
- **Tier 5** (minimal, GitHub Models free / Groq free): 5 tools, minimal prompt for token-limited providers

Tier assignment lives in `mc-core/src/model_registry.rs` (`ModelMeta`).

### Managed Agents Pattern
- Named agents defined in `agents/*.md` with YAML frontmatter (name, model, tools, instructions)
- Manager-executor pattern: expensive manager model routes to cheap executor models
- Agents run as isolated subagents (`SubagentSpawner`) with their own session and tool filter
- `ManagedAgentConfig` in `mc-config/src/types.rs`: max_concurrent, budget controls

## 30 Built-in Tools

Organized by category:

**File Operations**: `bash`, `read_file`, `write_file`, `edit_file`, `batch_edit`, `apply_patch`

**Search & Navigation**: `glob_search`, `grep_search`, `codebase_search` (TF-IDF, symbol-aware)

**Web**: `web_fetch`, `web_search`, `browser`

**Memory**: `memory_read`, `memory_write`

**Agents & Tasks**: `subagent`, `task_create`, `task_get`, `task_list`, `task_stop`

**Development**: `lsp_query`, `edit_plan`, `todo_write`, `notebook_edit`, `debug`

**Git Worktree**: `worktree_enter`, `worktree_exit`

**MCP**: `mcp_list_resources`, `mcp_read_resource`

**Utility**: `ask_user`, `sleep`

## Slash Commands (TUI)

Key commands available in the TUI (`mc-tui/src/commands.rs`):

| Command | Description |
|---------|-------------|
| `/help` | Show all commands |
| `/model` | Interactive model picker |
| `/cost` | Token/cost breakdown for session |
| `/tokens` | Detailed token usage (estimated vs actual, context %) |
| `/context` | Show context window contents |
| `/export` | Export conversation to file |
| `/diff-preview` | Preview pending file changes |
| `/undo` | Roll back last file changes |
| `/memory` | View/edit persistent memory facts |
| `/raw` | View last response in plain terminal (for text selection) |
| `/copy` | Copy last response to clipboard |
| `/alias` | Define custom command aliases |
| `/template` | Pre-built prompts (review, refactor, test, explain, etc.) |
| `/doctor` | Check version, provider, API key, git, config validity |
| `/review` | Show all file changes (git diff HEAD) |
| `/retry` | Re-submit last user input |
| `/pin` | Pin message to survive context compaction |
| `/theme` | Toggle dark/light theme |
| `/spec` | Generate technical spec before coding |
| `/config` | Show current runtime configuration |
| `/add` | Add file/directory content to next prompt |
| `/sessions` | List and delete saved sessions |
| `/permissions` | Show and toggle permission modes |
| `/auto-test` | Run tests after every file write |
| `/auto-commit` | Auto-commit after successful turns |
| `/plugin` | install/list/update/remove plugins |
| `/tree` | Directory tree with depth control |
| `/todo` | Find all TODO/FIXME/HACK in codebase |
| `/recent` | Show recently modified files |
| `/ship` | Git add all + LLM commit message |
| `/test` | Auto-detect test runner and run tests |
| `/version` | Show version, OS, architecture |
| `/history` | Display last 20 input history entries |

Custom commands: `.magic-code/commands/*.md` auto-discovered.

## Configuration

Config files use TOML, loaded in layers (global → project → runtime). Key types in `mc-config/src/types.rs`:

```
RuntimeConfig           — provider, model, max_tokens, permission_mode, thinking_enabled,
                          mcp_servers, hooks, managed_agents, fallback_provider, fallback_model,
                          auto_compact_threshold, tool_tier
PermissionMode          — ReadOnly | WorkspaceWrite (default) | FullAccess
ProviderConfig          — api_key_env, max_retries, base_url, models_whitelist/blacklist
CompactionConfig        — auto_compact_threshold, preserve_recent_messages, strategy ("smart" | "naive")
ManagedAgentConfig      — max_concurrent, budget controls
McpServerConfig         — command, args, env
HookConfig              — event (pre/post tool), command, tool filter
ThinkingConfig          — enabled, budget_tokens
RetryConfig             — max_retries, initial_backoff, max_backoff
```

## Supported Providers (15+)

Anthropic (Claude), Google Gemini, OpenAI, Groq, DeepSeek, Mistral, xAI (Grok), OpenRouter, Together AI, Perplexity, Cohere, Cerebras, Ollama, LM Studio, llama.cpp, GitHub Models (GPT-4o, Llama 405B via `--base-url https://models.inference.ai.azure.com`)

Provider implementations: `mc-provider/src/anthropic.rs`, `mc-provider/src/gemini.rs`, `mc-provider/src/generic.rs` (OpenAI-compatible).

## Do / Don't

### Do
- Run `cargo clippy` before committing
- Keep crate boundaries clean
- Write tests for new functionality
- Use `tracing::debug!` / `tracing::warn!` for logging
- Handle cancellation via `CancellationToken` in async loops
- Serialize new Session fields with `#[serde(default)]` for backward compat
- Use `Arc<T>` for shared state that needs concurrent access
- **Create a feature branch + PR for every change** (never push directly to main)
- Run `cargo deny check` when adding dependencies

### Don't
- Don't add `unsafe` code
- Don't add dependencies without checking workspace `Cargo.toml` first
- Don't put business logic in mc-tui (it's presentation only)
- Don't make mc-provider depend on mc-tools or vice versa
- Don't use `println!` — use `tracing` macros
- Don't dispatch memory/subagent tools through mc-tools (circular dep risk)
- Don't push directly to `main` — always go through PR

## Development Workflow

Every change — feature, fix, refactor — goes through this flow:

1. **Branch**: `git checkout -b feat/short-description` (or `fix/`, `refactor/`)
2. **Code**: implement, run `cargo test --workspace && cargo clippy --workspace --all-targets`
3. **Commit**: `git add -A && git commit -m "feat: short description"`
4. **Push**: `git push -u origin feat/short-description`
5. **PR**: create PR on GitHub → Qodo Merge auto-reviews
6. **Fix**: address Qodo feedback if needed, push again
7. **Merge**: squash merge into main

### Branch Naming
- `feat/` — new feature
- `fix/` — bug fix
- `refactor/` — code cleanup
- `docs/` — documentation only
- `test/` — test additions

### PR Title Format
```
feat: add MCP support to single-shot mode
fix: handle empty response from provider
refactor: extract tool registry setup into helper
```
