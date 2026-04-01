# magic-code v1.0 — Product Design Document

**Author:** bmkien
**Date:** 2026-04-01
**Status:** Draft — Pending Review

---

## 1. Vision

magic-code là một production-grade TUI agentic AI coding agent. Mục tiêu v1.0 là đạt feature parity với Claude Code / Codex CLI, đồng thời vượt trội ở:

- **Tốc độ**: Rust-native, zero-copy streaming, async I/O toàn bộ
- **Multi-provider**: Chuyển đổi liền mạch giữa Anthropic / OpenAI / Gemini / Ollama / bất kỳ OpenAI-compatible endpoint
- **Token efficiency**: Compaction thông minh, diff-based editing, subagent delegation
- **Developer UX**: TUI thực sự (không phải REPL), syntax highlighting, permission control rõ ràng

---

## 2. Current State Assessment

### 2.1 Cái đã có (solid foundation)

| Component | Status | Quality |
|-----------|--------|---------|
| Workspace structure (6 crates) | ✅ Done | Good — clean boundaries |
| Anthropic provider + SSE parser | ✅ Done | Good — proper chunked parsing |
| GenericProvider (OpenAI/LiteLLM/Ollama) | ✅ Done | Good — retry + backoff |
| Tools: bash, read/write/edit, glob, grep | ✅ Done | Good — diff generation |
| Config layering (global → project → local) | ✅ Done | Good — TOML merge |
| TUI skeleton (ratatui) | ✅ Done | Basic — needs major work |
| Conversation runtime + agentic loop | ✅ Done | Works — needs streaming |
| Session save/load | ✅ Done | Good |
| Compaction | ✅ Done | Naive — needs LLM-based |
| Permission system | ✅ Done | Partial — Prompt mode broken |
| Hook system | ✅ Done | Good |
| MCP client | ✅ Done | Sync-only, no persistent conn |
| Subagent spawner | ✅ Done | Not wired into runtime |

### 2.2 Critical Gaps for Production

```
[BLOCKING]     Streaming is fake — collect-then-return, not true yield
[BLOCKING]     Tool execution is sync — blocks tokio runtime
[BLOCKING]     No cancel mechanism — Ctrl+C kills entire app
[CORRECTNESS]  Context window hardcoded 200k — wrong for non-Anthropic
[CORRECTNESS]  Permission Prompt mode always returns Deny
[UX]           No scroll, no syntax highlight, no input history
[FEATURE]      Subagents exist but unreachable from LLM
[FEATURE]      No Gemini native provider
```

---

## 3. Target Architecture (v1.0)

### 3.1 Crate Map (updated)

```
mc-cli          Binary. CLI parsing, provider construction, dispatch.
                NEW: --resume, --session-id, --permission-mode flags

mc-tui          TUI layer. ratatui + crossterm.
                NEW: syntax highlighting, scroll, permission prompt widget,
                     progress indicator, markdown rendering

mc-core         Conversation runtime.
                NEW: true async streaming (channel-based), LLM compaction,
                     subagent tool, cancel token, model registry

mc-provider     LLM providers.
                NEW: return Stream<ProviderEvent> instead of Vec,
                     Gemini native provider, model info registry

mc-tools        Tool execution.
                NEW: fully async, workspace sandboxing,
                     async MCP with persistent connection

mc-config       Configuration.
                NEW: permission mode wiring, provider alias resolution
```

### 3.2 Data Flow (v1.0)

```
User Input
    │
    ▼
mc-cli (parse args, load config, resolve provider)
    │
    ▼
mc-tui (event loop)
    │
    ├──► KeyEvent ──► InputBuffer ──► submit
    │                                    │
    │                                    ▼
    │                          mc-core::ConversationRuntime
    │                                    │
    │    ┌───────────────────────────────┤
    │    │                               │
    │    ▼                               ▼
    │  mc-provider::stream()        mc-tools::execute()
    │    │  (async Stream)              │  (async)
    │    │                               │
    │    ▼                               ▼
    │  channel ──► TUI render       channel ──► TUI render
    │    │                               │
    │    └───────────┬───────────────────┘
    │                │
    │                ▼
    │         CancelToken::cancelled()?
    │                │
    │                ▼
    │         Compaction check ──► LLM summarize if needed
    │                │
    │                ▼
    │         Session auto-save
    │
    └──► StatusBar (model, tokens, cost, latency)
```

---

## 4. Workstreams

### WS-1: Async Streaming Pipeline (Foundation)

**Why first:** Mọi thứ khác phụ thuộc vào đây. Không có true streaming thì TUI improvements vô nghĩa.

#### 4.1.1 Provider returns `Stream` thay vì `Vec`

```rust
// BEFORE (current)
pub trait LlmProvider {
    async fn stream(&self, req: &CompletionRequest)
        -> Result<Vec<ProviderEvent>, ProviderError>;
}

// AFTER
pub trait LlmProvider: Send + Sync {
    fn stream(&self, req: &CompletionRequest)
        -> Pin<Box<dyn Stream<Item = Result<ProviderEvent, ProviderError>> + Send>>;
}
```

**Trade-off:** `Pin<Box<dyn Stream>>` có allocation overhead nhưng cho phép trait object — cần thiết vì CLI dispatch provider dynamically. Alternative là enum dispatch nhưng thêm boilerplate khi thêm provider mới.

**Decision:** Dùng `async-stream` crate + `Pin<Box<dyn Stream>>`. Performance overhead negligible so với network latency.

#### 4.1.2 Runtime dùng channel thay vì callback

```rust
// BEFORE
runtime.run_turn(provider, input, policy, &mut |event| { ... }).await;

// AFTER
let (event_tx, event_rx) = mpsc::unbounded_channel();
let cancel = CancelToken::new();
let handle = runtime.run_turn(provider, input, policy, event_tx, cancel.clone());
// TUI consumes event_rx, can call cancel.cancel() anytime
```

#### 4.1.3 Async tool execution

```rust
// BEFORE (blocking)
impl BashTool {
    pub fn execute(command: &str, timeout: Option<u64>) -> Result<String, ToolError>;
}

// AFTER
impl BashTool {
    pub async fn execute(command: &str, timeout: Option<Duration>) -> Result<String, ToolError> {
        let child = tokio::process::Command::new("sh")
            .arg("-c").arg(command)
            .stdout(Stdio::piped()).stderr(Stdio::piped())
            .spawn()?;
        // ...
    }
}
```

**Files changed:** `mc-provider/src/lib.rs`, `mc-provider/src/anthropic.rs`, `mc-provider/src/generic.rs`, `mc-core/src/runtime.rs`, `mc-tools/src/bash.rs`, `mc-tools/src/registry.rs`, `mc-cli/src/main.rs`

**New deps:** `async-stream`, `tokio-util` (CancellationToken), `futures-core` (Stream trait)

---

### WS-2: TUI Production Quality

#### 4.2.1 Scroll & viewport

- `PageUp/PageDown` scroll output
- `Home/End` jump to top/bottom
- Mouse scroll support
- Auto-scroll khi ở bottom, stop auto-scroll khi user scroll up
- Scroll indicator (e.g., "line 42/180")

#### 4.2.2 Syntax highlighting

`syntect` đã là dependency. Cần:
- Detect code blocks trong output (` ``` ` fences)
- Apply syntax theme (base16-ocean.dark default, configurable)
- Fallback to plain text nếu language unknown

#### 4.2.3 Markdown rendering

Output từ LLM là markdown. Cần render:
- **Bold**, *italic* → ratatui Style modifiers
- `inline code` → Color::Cyan background
- Code blocks → syntax highlighted panel
- Lists → proper indentation
- Headers → bold + color

#### 4.2.4 Permission prompt widget

Khi tool cần approval:
```
┌─ Permission Required ──────────────────────┐
│ Tool: bash                                  │
│ Command: rm -rf ./build                     │
│                                             │
│ [A]llow  [D]eny  [S]ession-allow  [Q]uit   │
└─────────────────────────────────────────────┘
```

Wire vào `PermissionPrompter` trait qua channel.

#### 4.2.5 Input improvements

- Up/Down arrow → input history (persist across sessions)
- Tab completion cho slash commands
- Multi-line input (Shift+Enter, đã có)
- Ctrl+U clear line, Ctrl+W delete word

#### 4.2.6 Progress & status

- Spinner khi waiting
- Token count live update
- Cost estimate (configurable pricing per model)
- Latency (time-to-first-token, total)
- Tool execution progress bar cho long-running bash

**Files changed:** `mc-tui/src/ui.rs` (major rewrite), `mc-tui/src/app.rs`, `mc-tui/src/input.rs`, new files: `mc-tui/src/highlight.rs`, `mc-tui/src/markdown.rs`, `mc-tui/src/widgets/permission.rs`, `mc-tui/src/widgets/status.rs`

---

### WS-3: Intelligence Layer

#### 4.3.1 Model registry

```rust
pub struct ModelRegistry {
    models: HashMap<String, ModelInfo>,
}

impl ModelRegistry {
    pub fn context_window(&self, model: &str) -> u32;
    pub fn supports_tools(&self, model: &str) -> bool;
    pub fn cost_per_token(&self, model: &str) -> (f64, f64); // (input, output)
}
```

Built-in registry cho known models + override qua config. Giải quyết hardcoded context window.

#### 4.3.2 LLM-based compaction

```rust
// BEFORE: truncate + text summary
compact_session(&mut session, 4);

// AFTER: ask LLM to summarize
async fn smart_compact(
    provider: &dyn LlmProvider,
    session: &mut Session,
    model: &str,
    preserve_recent: usize,
) -> Result<(), CompactError> {
    let old = session.messages.drain(..split).collect();
    let summary_prompt = build_compaction_prompt(&old);
    let summary = provider.complete_simple(summary_prompt).await?;
    session.messages.insert(0, ConversationMessage::system(summary));
}
```

**Trade-off:** Costs extra tokens cho summarization call. Nhưng giữ được context quality tốt hơn nhiều so với text truncation. Dùng cheap model (haiku) cho compaction.

#### 4.3.3 Subagent as tool

Register `subagent` tool để LLM có thể tự delegate:

```json
{
  "name": "subagent",
  "description": "Delegate a task to an isolated subagent with its own context.",
  "input_schema": {
    "type": "object",
    "properties": {
      "task": { "type": "string" },
      "context": { "type": "string" }
    },
    "required": ["task"]
  }
}
```

Subagent chạy với context riêng (không forward session history — context isolation). Return summary khi done.

#### 4.3.4 System prompt engineering

Current system prompt quá basic. Cần:
- Tool usage guidelines (khi nào dùng tool nào)
- Output format preferences
- Error recovery instructions
- Project context injection (detected stack, git status, instruction files)
- Compaction-aware: khi session đã compact, note rằng earlier context is summarized

---

### WS-4: MCP & Extensibility

#### 4.4.1 Async MCP with persistent connection

```rust
pub struct McpClient {
    child: tokio::process::Child,
    stdin: tokio::io::BufWriter<ChildStdin>,
    stdout: tokio::io::BufReader<ChildStdout>,
    request_id: AtomicU64,
}

impl McpClient {
    pub async fn connect(command: &str, args: &[&str]) -> Result<Self, ToolError>;
    pub async fn discover_tools(&mut self) -> Result<Vec<ToolSpec>, ToolError>;
    pub async fn call_tool(&mut self, name: &str, args: &Value) -> Result<String, ToolError>;
    pub async fn disconnect(&mut self) -> Result<(), ToolError>;
}
```

Một connection, nhiều calls. Không spawn process mỗi lần.

#### 4.4.2 MCP config trong TOML

```toml
[[mcp_servers]]
name = "github"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
env = { GITHUB_TOKEN_ENV = "GITHUB_TOKEN" }

[[mcp_servers]]
name = "postgres"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-postgres"]
```

Auto-discover tools on startup, register vào `ToolRegistry`.

#### 4.4.3 Hook config trong TOML

```toml
[[hooks]]
event = "pre_tool_call"
match_tools = ["bash"]
command = "echo $MC_TOOL_NAME >> /tmp/mc-audit.log"

[[hooks]]
event = "post_tool_call"
command = "notify-send 'magic-code' 'Tool completed: $MC_TOOL_NAME'"
```

---

### WS-5: Permission & Safety

#### 4.5.1 Wire config permission mode

```
read-only       → Deny bash, Deny write_file, Deny edit_file
workspace-write → Allow read, Prompt write/edit (within cwd), Deny bash outside cwd
full-access     → Allow all (current behavior)
```

#### 4.5.2 Workspace sandboxing

Tools should refuse to operate outside project root:
- `write_file("/etc/passwd", ...)` → Deny
- `bash("rm -rf /")` → Deny (detect dangerous patterns)
- `read_file("../../secrets.env")` → Deny (path traversal)

```rust
pub struct WorkspaceSandbox {
    root: PathBuf,
    allowed_paths: Vec<PathBuf>,  // additional allowed dirs
}

impl WorkspaceSandbox {
    pub fn check(&self, path: &Path) -> Result<(), ToolError>;
}
```

#### 4.5.3 Audit log

Mọi tool execution ghi vào `~/.local/share/magic-code/audit.jsonl`:
```json
{"ts":"2026-04-01T12:00:00Z","tool":"bash","input":"ls -la","output_len":245,"duration_ms":12,"allowed":true}
```

---

### WS-6: Gemini Provider

#### 4.6.1 Native Gemini API

Gemini API khác OpenAI format đáng kể:
- Endpoint: `generativelanguage.googleapis.com`
- Auth: API key as query param, not header
- Tool format: `functionDeclarations` thay vì `tools[].function`
- Streaming: Server-Sent Events nhưng format khác

Cần native implementation thay vì route qua GenericProvider.

**Trade-off:** Thêm ~300 LOC cho Gemini-specific wire types. Nhưng đảm bảo tool calling hoạt động đúng — GenericProvider không handle Gemini tool format.

---

### WS-7: Developer Experience

#### 4.7.1 `--resume` flag

```bash
magic-code --resume              # resume last session
magic-code --session-id abc123   # resume specific session
```

#### 4.7.2 Non-interactive pipe mode

```bash
echo "explain this code" | magic-code --pipe
cat error.log | magic-code "fix this error"
magic-code "list all TODOs" > todos.md
```

Detect stdin is not TTY → non-interactive mode. No TUI, stream to stdout.

#### 4.7.3 Verbose/debug mode

```bash
magic-code -v "hello"     # show tool calls
magic-code -vv "hello"    # show full request/response
magic-code -vvv "hello"   # show SSE frames
```

#### 4.7.4 Cost tracking

```bash
magic-code /cost           # show session cost
magic-code /cost --total   # show all-time cost
```

Persist in `~/.local/share/magic-code/usage.jsonl`.

---

## 5. Implementation Order

```
Phase 1 — Async Foundation (WS-1)                          ~4 days
├── 1.1 Provider Stream trait + async-stream
├── 1.2 Async tool execution (tokio::process)
├── 1.3 CancelToken wiring
├── 1.4 Channel-based runtime ↔ TUI communication
└── 1.5 Update CLI dispatch

Phase 2 — TUI Rewrite (WS-2)                               ~5 days
├── 2.1 Scroll + viewport management
├── 2.2 Markdown renderer
├── 2.3 Syntax highlighting (syntect)
├── 2.4 Permission prompt widget
├── 2.5 Input history + keybindings
└── 2.6 Status bar (cost, latency, progress)

Phase 3 — Intelligence (WS-3)                               ~3 days
├── 3.1 Model registry + dynamic context window
├── 3.2 LLM-based compaction
├── 3.3 Subagent tool registration
└── 3.4 System prompt v2

Phase 4 — Safety & Permissions (WS-5)                       ~2 days
├── 4.1 Wire config permission modes
├── 4.2 Workspace sandboxing
└── 4.3 Audit log

Phase 5 — MCP & Extensibility (WS-4)                       ~3 days
├── 5.1 Async MCP client
├── 5.2 MCP config in TOML
└── 5.3 Hook config in TOML

Phase 6 — Gemini + DX (WS-6, WS-7)                         ~3 days
├── 6.1 Gemini native provider
├── 6.2 --resume, --pipe, -v flags
├── 6.3 Cost tracking
└── 6.4 Non-interactive mode

Phase 7 — Polish & Release                                  ~2 days
├── 7.1 Integration tests
├── 7.2 CI/CD (GitHub Actions)
├── 7.3 README rewrite
├── 7.4 Homebrew formula / cargo install
└── 7.5 Release binaries (cross-compile)
```

**Total estimate: ~22 working days**

---

## 6. New Dependencies

| Crate | Purpose | Size |
|-------|---------|------|
| `async-stream` | `Stream` impl for providers | Tiny |
| `futures-core` | `Stream` trait | Tiny |
| `tokio-util` | `CancellationToken` | Small |
| `unicode-width` | Proper TUI text width | Tiny |
| `textwrap` | Markdown line wrapping | Small |
| `chrono` | Timestamps for audit log | Medium |
| `dirs` | XDG directory resolution | Tiny |

---

## 7. Files to Create (new)

```
mc-tui/src/highlight.rs          Syntax highlighting with syntect
mc-tui/src/markdown.rs           Markdown → ratatui Spans converter
mc-tui/src/widgets/mod.rs        Custom widget module
mc-tui/src/widgets/permission.rs Permission prompt popup
mc-tui/src/widgets/status.rs     Enhanced status bar
mc-tui/src/history.rs            Input history persistence
mc-core/src/cancel.rs            CancelToken wrapper
mc-core/src/model_registry.rs    Known model metadata
mc-core/src/cost.rs              Cost calculation + persistence
mc-provider/src/gemini.rs        Native Gemini provider
mc-provider/src/stream.rs        Stream adapter utilities
mc-tools/src/sandbox.rs          Workspace path sandboxing
mc-tools/src/audit.rs            Audit log writer
docs/adr/002-async-streaming.md  ADR for streaming decision
docs/adr/003-permission-model.md ADR for permission model
```

---

## 8. Risk & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Stream trait refactor breaks all providers | High | Do provider-by-provider, keep old trait as compat shim during migration |
| LLM compaction costs too many tokens | Medium | Use cheapest model (haiku), cap summary length, make configurable |
| Gemini tool calling format changes | Low | Abstract behind same ToolDefinition type, version-pin API |
| TUI rewrite introduces regressions | Medium | Keep current `draw()` working, add new widgets incrementally |
| MCP servers crash/hang | Medium | Timeout + auto-restart, health check on startup |

---

## 9. Success Criteria for v1.0

- [ ] True streaming: first token visible < 500ms after LLM starts generating
- [ ] All tools async, no blocking calls on tokio runtime
- [ ] Cancel any running operation with Ctrl+C (single press)
- [ ] Syntax highlighted code output
- [ ] Permission prompt for destructive operations
- [ ] Session resume across restarts
- [ ] Works with Anthropic, OpenAI, Gemini, Ollama, LiteLLM out of the box
- [ ] MCP server support via config
- [ ] < 10MB binary size (release, stripped)
- [ ] < 50ms startup time (no network calls before first prompt)
- [ ] Audit log for all tool executions
