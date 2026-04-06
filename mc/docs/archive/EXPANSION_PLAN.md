# mc-core Feature Expansion Plan v4 (Final)

> 11 features, bottom-up, semver minor, no drops. Optimized phasing by RICE.

## Design Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Retry scope | Runtime = mid-stream only | Provider already retries connection |
| Parallel tools | Refactor to `Arc<T>` | `&mut self` blocks concurrency |
| Memory arch | Dispatch in mc-core, specs in mc-tools | Avoid circular dep |
| Session compat | `Option<T>` + `#[serde(default)]` | Don't break saved sessions |
| Image input | `/image` slash command | Avoid false-positive path detection |
| Thinking display | Collapsed default, `/thinking` toggle | Prevent cognitive overload |
| Branching | Fork + switch + list + delete, NO merge | Merge conversations is ambiguous |
| Status bar | 3-tier: always / when-active / on-demand | Prevent info overload |
| Prompt caching | Anthropic cache_control on system + tools | 90% cost reduction |
| @-mentions | `@path` syntax in user input | Table stakes for coding agents |
| Undo | Track file changes per turn, revert on `/undo` | Safety net for LLM mistakes |

## RICE Ranking (all 11)

| # | Task | Reach | Impact | Confidence | Effort | RICE | Phase |
|---|---|---|---|---|---|---|---|
| 1 | Prompt Caching | 100% | High | High | 1 | ★★★★★ | 1 |
| 2 | Memory | 80% | High | High | 2 | ★★★★★ | 2 |
| 3 | Extended Thinking | 60% | High | High | 2 | ★★★★☆ | 2 |
| 4 | Token Budget | 100% | Low | High | 1 | ★★★★☆ | 1 |
| 5 | Retry | 30% | Med | High | 2 | ★★★☆☆ | 1 |
| 6 | Multimodal | 40% | High | High | 3 | ★★★☆☆ | 2 |
| 7 | @-mentions | 50% | Med | High | 2 | ★★★☆☆ | 3 |
| 8 | Undo/Rollback | 40% | Med | High | 2 | ★★★☆☆ | 3 |
| 9 | Parallel Tools | 20% | Low-Med | Low | 4 | ★★☆☆☆ | 4 |
| 10 | Tool Cache | 10% | Low | Low | 2 | ★☆☆☆☆ | 4 |
| 11 | Branching | 5% | Low | Low | 3 | ★☆☆☆☆ | 4 |

## Execution Phases (optimized)

```
Phase 1 — Foundation (low effort, high frequency):
  1A: Token Budget
  1B: Retry (mid-stream)
  1C: Prompt Caching ← NEW, highest ROI

Phase 2 — Differentiators (high RICE):
  2A: Memory
  2B: Multimodal (extends Block — pattern for 2C)
  2C: Extended Thinking (needs 1A + 2B pattern)

Phase 3 — Table Stakes UX:
  3A: @-mentions / Context Files ← NEW
  3B: Undo / Rollback ← NEW

Phase 4 — Performance & Advanced:
  4A: Parallel Tools (Arc refactor)
  4B: Tool Cache (after 4A execution path)
  4C: Branching (Session must be stable)
```

## Dependency Graph

```
1A Token Budget ──────────────────→ 2C Thinking (budget allocation)
1B Retry ── independent
1C Prompt Caching ── independent

2A Memory ── independent
2B Multimodal ─→ 2C Thinking (Block extension pattern)

3A @-mentions ── independent
3B Undo ── independent

4A Parallel Tools ─→ 4B Tool Cache (execution path)
All Session changes ─→ 4C Branching (last)
```

## Cross-crate Impact Matrix

| Task | mc-core | mc-provider | mc-tools | mc-config | mc-tui |
|---|---|---|---|---|---|
| 1A Token Budget | `token_budget.rs` | — | — | — | — |
| 1B Retry | `retry.rs` | `RetryAttempt` event | — | retry config | retry indicator |
| 1C Prompt Caching | — | cache_control in request | — | — | cache savings display |
| 2A Memory | `memory.rs` + dispatch | — | tool specs | memory config | `/memory` commands |
| 2B Multimodal | extend Block | extend ContentBlock + 3 providers | — | — | `/image` command |
| 2C Thinking | extend Block | events + Anthropic wire | — | thinking config | `/thinking` toggle |
| 3A @-mentions | context resolver | — | — | — | input parsing |
| 3B Undo | `undo.rs` + file tracker | — | track writes | — | `/undo` command |
| 4A Parallel Tools | Arc refactor + `parallel_tools.rs` | — | — | max_concurrent | batch display |
| 4B Tool Cache | `tool_cache.rs` | — | — | cache config | — |
| 4C Branching | `branch.rs` + Session fields | — | — | — | `/fork` `/switch` |

---

## Phase 1: Foundation

### Task 1A: Token Budget Management

**Files:** NEW `mc-core/src/token_budget.rs`, MOD `runtime.rs`, MOD `lib.rs`

```rust
pub struct TokenBudget {
    context_window: usize,
    response_reserve: usize,
}

impl TokenBudget {
    pub fn new(context_window: usize, response_reserve: usize) -> Self;
    /// context_window - system_tokens - tool_schema_tokens - response_reserve
    pub fn available_for_messages(&self, system_tokens: usize, tool_schema_tokens: usize) -> usize;
    /// min(response_reserve, context_window - used_context). Anthropic max_tokens = OUTPUT tokens.
    pub fn effective_max_tokens(&self, used_context: usize) -> u32;
}
```

Integration: `build_request()` uses `effective_max_tokens()` instead of static `self.max_tokens`.

---

### Task 1B: Runtime-level Retry (Mid-stream)

**Files:** NEW `mc-core/src/retry.rs`, MOD `runtime.rs`, MOD `lib.rs`, MOD `mc-provider/types.rs`, MOD `mc-config/types.rs`

```rust
pub struct RetryPolicy {
    pub max_attempts: u32,       // default 2
    pub initial_backoff_ms: u64, // default 500
    pub max_backoff_ms: u64,     // default 5000
}
```

Scope: mid-stream failures only. Provider handles connection retry.

New event: `ProviderEvent::RetryAttempt { attempt: u32, max: u32, reason: String }`

UX: Keep partial text visible, append `"⟳ stream interrupted, retrying (2/3)..."`, new text continues after.

Config: `[retry] max_attempts = 2, initial_backoff_ms = 500`

---

### Task 1C: Prompt Caching (NEW)

**Files:** MOD `mc-provider/src/types.rs`, MOD `mc-provider/src/anthropic.rs`, MOD `mc-core/src/runtime.rs`

Anthropic prompt caching: add `cache_control: { type: "ephemeral" }` to system prompt block and tool definitions. Cached content costs 90% less on subsequent turns.

**mc-provider changes:**

```rust
// types.rs — AnthropicRequest system becomes structured
pub(crate) struct AnthropicRequest {
    // system: Option<String>  →  becomes:
    pub system: Option<Vec<AnthropicSystemBlock>>,
    // ... rest unchanged
}

pub(crate) struct AnthropicSystemBlock {
    pub r#type: String,          // "text"
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

pub(crate) struct CacheControl {
    pub r#type: String, // "ephemeral"
}
```

**Strategy:**
- System prompt: always cache (stable across turns)
- Tool definitions: always cache (stable across turns)
- Last 2 user messages: mark with cache_control (sliding window)
- Track cache savings in `TokenUsage`: `cache_read_input_tokens` already exists

**UX:**
- Status bar: show cache savings when > 0: `"↓12K (8K cached)"`
- `/cost` command: show breakdown including cache savings

---

## Phase 2: Differentiators

### Task 2A: Long-term Memory

**Files:** NEW `mc-core/src/memory.rs`, MOD `runtime.rs`, MOD `lib.rs`, MOD `mc-tools/spec.rs`, MOD `mc-config/types.rs`, MOD `mc-tui/app.rs`

```rust
pub struct MemoryStore {
    facts: Vec<Fact>,
    path: PathBuf,        // .magic-code/memory.json
    max_facts: usize,     // default 50
}

#[derive(Serialize, Deserialize)]
pub struct Fact {
    pub key: String,
    pub value: String,
    pub updated_at: String,
}
```

Methods: `load()`, `save()`, `get()`, `set()`, `delete()`, `all()`, `to_prompt_section()`

Architecture: dispatch in `runtime.rs` (same as subagent), specs in `mc-tools/spec.rs`.

Bootstrap: auto-import from `instruction_files` config (CLAUDE.md, CONVENTIONS.md, .cursorrules) on first load if memory is empty.

UX:
- Session start: `"📌 12 project facts loaded"`
- On write: `"  📌 memory: saved 'test_framework' = 'pytest'"`
- `/memory` — list, `/memory delete <key>`, `/memory clear`

Config: `[memory] path = ".magic-code/memory.json", max_facts = 50`

---

### Task 2B: Multimodal Support (Images)

**Files:** MOD `mc-core/session.rs`, MOD `mc-provider/types.rs`, MOD `mc-provider/anthropic.rs`, MOD `mc-provider/generic.rs`, MOD `mc-provider/gemini.rs`, MOD `mc-core/runtime.rs`, MOD `mc-tui/app.rs`

```rust
// mc-core session.rs
pub enum ImageSource { Base64(String), Path(String) }
pub enum Block {
    // ... existing ...
    Image { source: ImageSource, media_type: String },
}

// mc-provider types.rs — always base64 at provider level
pub enum ContentBlock {
    // ... existing ...
    Image { data: String, media_type: String },
}
```

Per-provider wire:
- Anthropic: `{ "type": "image", "source": { "type": "base64", ... } }`
- OpenAI: `{ "type": "image_url", "image_url": { "url": "data:...;base64,..." } }`
- Gemini: `{ "inlineData": { "mimeType": "...", "data": "..." } }`

UX: `/image path/to/file.png [prompt]` — validate exists, format, size < 5MB. Display: `"🖼 image: file.png (1.2MB, image/png)"`

Token estimate: ~1600 tokens per image in `content_len()`.

---

### Task 2C: Extended Thinking

**Files:** MOD `mc-core/session.rs`, MOD `mc-provider/types.rs`, MOD `mc-provider/anthropic.rs`, MOD `mc-core/runtime.rs`, MOD `mc-core/token_budget.rs`, MOD `mc-config/types.rs`, MOD `mc-tui/app.rs`

```rust
// session.rs
pub enum Block { ..., Thinking { text: String } }

// provider types.rs
pub enum ProviderEvent { ..., ThinkingDelta(String) }
pub enum ContentBlock { ..., Thinking { text: String } }
pub struct CompletionRequest { ..., pub thinking_budget: Option<u32> }
```

Anthropic wire:
- Request: `"thinking": { "type": "enabled", "budget_tokens": N }`
- Stream: handle `type: "thinking"` blocks + `thinking_delta`
- Must pass thinking blocks back in subsequent messages

UX:
- Default collapsed: `"💭 thinking... (1,247 tokens)"`
- `/thinking` toggle: ON shows full text in `Color::DarkGray`
- Status bar tier 2: `"💭 $0.012"` when active

Config: `[thinking] enabled = true, budget_tokens = 10000`

---

## Phase 3: Table Stakes UX

### Task 3A: @-mentions / Context Files (NEW)

**Files:** NEW `mc-core/src/context_resolver.rs`, MOD `runtime.rs`, MOD `lib.rs`, MOD `mc-tui/app.rs`

User types `@src/main.rs fix the auth bug` → agent auto-reads file and includes content in the user message.

```rust
pub struct ContextResolver {
    workspace_root: PathBuf,
}

impl ContextResolver {
    /// Parse @-mentions from user input, resolve to file contents.
    /// Returns (cleaned_input, Vec<ResolvedContext>)
    pub fn resolve(&self, input: &str) -> (String, Vec<ResolvedContext>);
}

pub struct ResolvedContext {
    pub path: String,
    pub content: String,
    pub token_estimate: usize,
}
```

Logic:
- Regex: `@([\w./\-]+)` — match file paths after @
- Validate: file exists relative to workspace root
- Read file content, estimate tokens
- Inject into user message as: `"[File: src/main.rs]\n```\n{content}\n```\n\n{rest of prompt}"`
- If file too large (> 10K tokens), truncate with note

UX:
- TUI: highlight `@path` in input with `Color::Cyan`
- On resolve: `"  📎 attached: src/main.rs (245 lines, ~1.2K tokens)"`
- Tab completion for @-paths (stretch goal, not required for v1)

---

### Task 3B: Undo / Rollback (NEW)

**Files:** NEW `mc-core/src/undo.rs`, MOD `runtime.rs`, MOD `lib.rs`, MOD `mc-tui/app.rs`

Track file changes made by tools during each turn. `/undo` reverts the last turn's changes.

```rust
pub struct UndoManager {
    turns: Vec<TurnSnapshot>,
    max_turns: usize, // default 10
}

pub struct TurnSnapshot {
    pub files: Vec<FileSnapshot>,
    pub turn_index: usize,
}

pub struct FileSnapshot {
    pub path: PathBuf,
    pub original_content: Option<String>, // None = file didn't exist (was created)
}

impl UndoManager {
    /// Call before tool execution to snapshot affected files.
    pub fn snapshot_before_write(&mut self, path: &Path);
    /// Mark end of turn — finalize current snapshot.
    pub fn end_turn(&mut self);
    /// Revert last turn's file changes.
    pub fn undo_last_turn(&mut self) -> Result<Vec<String>, io::Error>;
}
```

Integration:
- `execute_tool()`: before `write_file`/`edit_file` → call `snapshot_before_write(path)`
- `bash` tool: cannot reliably track → warn user `"⚠ bash changes cannot be undone"`
- After `run_turn()` completes → `end_turn()`
- Wire `app.undo_requested` (already exists in TUI) to `undo_manager.undo_last_turn()`

UX:
- `/undo` → `"↩ Reverted 3 files: src/main.rs, src/lib.rs, Cargo.toml"`
- `/undo` when nothing to undo → `"Nothing to undo"`
- Max 10 turns of undo history

---

## Phase 4: Performance & Advanced

### Task 4A: Parallel Tool Execution

**Files:** NEW `mc-core/src/parallel_tools.rs`, MOD `runtime.rs` (Arc refactor), MOD `lib.rs`, MOD `mc-config/types.rs`

**Step 4A-1 — Refactor runtime internals:**
```rust
// Arc wrappers for concurrent access
hook_engine: Option<Arc<HookEngine>>,
audit_log: Option<Arc<AuditLog>>,
tool_registry: Arc<ToolRegistry>,
```
Extract `execute_single_tool()` as free async fn. Subagent stays sequential.

**Step 4A-2 — ParallelToolExecutor:**
```rust
pub struct ParallelToolExecutor {
    max_concurrent: usize, // default 4
}

impl ParallelToolExecutor {
    pub async fn execute_batch(
        &self,
        tools: Vec<(String, String, String)>,
        /* Arc refs, policy, cancel, callbacks */
    ) -> Vec<(String, String, String, bool)>;
}
```

Uses `tokio::JoinSet` + semaphore. Subagent tools filtered to sequential.

UX:
- Batch start: `"⚙ running 3 tools: read_file, grep_search, glob_search"`
- Each complete: `"✓ read_file (45ms)"` / `"✗ bash (error)"`

Config: `[tools] max_concurrent = 4`

---

### Task 4B: Tool Result Caching

**Files:** NEW `mc-core/src/tool_cache.rs`, MOD `runtime.rs` or `parallel_tools.rs`, MOD `lib.rs`, MOD `mc-config/types.rs`

```rust
pub struct ToolCache {
    entries: HashMap<u64, CacheEntry>,
    cacheable_tools: HashSet<String>,
    ttl: Duration,
    max_entries: usize,
}
```

- Default cacheable: `["glob_search", "grep_search", "read_file"]`
- Invalidation: `write_file`/`edit_file`/`bash` → `invalidate_all()`
- Hash: `std::hash::DefaultHasher`

Config: `[cache] ttl_secs = 30, max_entries = 64`

---

### Task 4C: Conversation Branching

**Files:** NEW `mc-core/src/branch.rs`, MOD `session.rs`, MOD `runtime.rs`, MOD `lib.rs`, MOD `mc-tui/app.rs`

Session (backward compat):
```rust
pub struct Session {
    // ... existing ...
    #[serde(default)] pub branch_id: Option<String>,
    #[serde(default)] pub parent_branch: Option<String>,
    #[serde(default)] pub fork_point: Option<usize>,
}
```

```rust
pub struct BranchManager {
    branches_dir: PathBuf,
    max_branches: usize, // default 5
}
```

Methods: `fork()`, `save_branch()`, `load_branch()`, `list_branches()`, `delete_branch()`

No merge. Auto-generated names (`fork-1`, `fork-2`).

UX: `/fork`, `/branches`, `/switch <name>`, `/branch delete <name>`. Status bar tier 2: `"🌿 fork-1"`.

---

## Status Bar Tiering

**Tier 1 (always):**
```
 claude-sonnet-4-20250514 │ ↓12K (8K cached) ↑2K $0.066 │ ready
```

**Tier 2 (when active, auto-show/hide):**
- Retry: `"⟳ retry 2/3"`
- Branch: `"🌿 fork-1"`
- Thinking: `"💭 $0.012"`

**Tier 3 (on-demand via `/status`):**
- Memory: `"📌 12 facts"`
- Cache: `"💾 hits: 5/12"`
- Budget: `"🎯 45K/200K tokens used"`

---

## New Slash Commands Summary

| Command | Task | Description |
|---|---|---|
| `/memory` | 2A | List all memory facts |
| `/memory delete <key>` | 2A | Delete a fact |
| `/memory clear` | 2A | Clear all facts |
| `/image <path> [prompt]` | 2B | Attach image to prompt |
| `/thinking` | 2C | Toggle thinking display |
| `/undo` | 3B | Revert last turn's file changes |
| `/fork` | 4C | Fork conversation at current point |
| `/branches` | 4C | List all branches |
| `/switch <name>` | 4C | Switch to branch |
| `/branch delete <name>` | 4C | Delete branch |

---

## File Change Summary

### New files (8):
```
mc-core/src/token_budget.rs      (1A)
mc-core/src/retry.rs             (1B)
mc-core/src/memory.rs            (2A)
mc-core/src/context_resolver.rs  (3A)
mc-core/src/undo.rs              (3B)
mc-core/src/parallel_tools.rs    (4A)
mc-core/src/tool_cache.rs        (4B)
mc-core/src/branch.rs            (4C)
```

### Modified files:
```
mc-core/src/lib.rs               — export all new modules
mc-core/src/runtime.rs           — integrate all features, Arc refactor (4A)
mc-core/src/session.rs           — Block::Image (2B), Block::Thinking (2C), branch fields (4C)
mc-core/src/token_budget.rs      — thinking reserve (2C)

mc-provider/src/types.rs         — ContentBlock::Image/Thinking, ThinkingDelta, RetryAttempt,
                                   thinking_budget, cache_control structs
mc-provider/src/anthropic.rs     — cache_control (1C), image wire (2B), thinking wire (2C)
mc-provider/src/generic.rs       — image wire (2B)
mc-provider/src/gemini.rs        — image wire (2B)

mc-tools/src/spec.rs             — memory_read/memory_write specs (2A)

mc-config/src/types.rs           — RetryConfig, CacheConfig, MemoryConfig, ThinkingConfig,
                                   max_concurrent_tools

mc-tui/src/app.rs                — new slash commands, events, status bar tiers
mc-tui/src/ui.rs                 — status bar tiering, thinking display, cache indicator
```

## Projected Impact

| Metric | v0.1.0 | After P1 | After P2 | After P3 | After P4 |
|---|---|---|---|---|---|
| Avg cost/session | $0.50 | $0.15 | $0.15 | $0.15 | $0.15 |
| Session success rate | ~70% | ~80% | ~85% | ~90% | ~92% |
| Feature parity vs Claude Code | 60% | 65% | 80% | 90% | 95% |
