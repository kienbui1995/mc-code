# What's New

## Version 1.8.0

### 🎉 Highlights
- **Headless mode** — NDJSON streaming, batch processing, proper exit codes for CI/CD
- **Context engineering** — selective tools + adaptive compaction = smaller prompts, better results
- **Model-tier profiles** — prompts auto-adapt to model capability (tier 1–4)
- **Qwen 3.5 support** — 9B and 27B models, fully self-hosted with 256K context
- **Golden test suite** — 154 scenarios to benchmark LLM intelligence

### 🤖 Headless & Batch
- `--batch` and `--pipe` now share session across turns — multi-turn pipelines work
- Auto-continue for read-only turns (no more stuck sessions)
- NDJSON output for machine-readable streaming

### 🧠 Smarter Prompts
- Only relevant tools injected per turn (reduces prompt bloat)
- Adaptive compaction keeps important context longer
- Per-model prompt profiles: tier 1 (GPT-4/Claude) through tier 4 (small local models)

### 🏠 Self-Hosted Models
- **Qwen 3.5 9B** — 256K context, works with vLLM
- **Qwen 3.5 27B** — promoted to tier 2, optimized prompts
- Fixed thinking mode conflicts during tool calling

### 📦 crates.io
- All crates now publishable on crates.io
- `mc-core` renamed to `mc-code-core` (name conflict)

### 🧪 Testing
- Golden test suite: 154 scenarios, 22 categories, 5 platforms
- Multi-turn session tests: 10 sessions, 76 turns
- Parallel test runner: `--parallel N`
- L1/L2 verification levels

### 🐛 Fixes
- Friendly onboarding when API key is missing
- Anthropic API compatibility (debug tool schema)
- Qwen context window and prompt improvements

## Version 1.1.0

### 🎉 Highlights
- **26 tools** (was 12): ask_user, sleep, notebook_edit, task system, worktree, batch_edit, apply_patch, todo_write, MCP resources
- **15 providers** (was 3): Groq, DeepSeek, Mistral, xAI, OpenRouter, Together, Perplexity, Cohere, Cerebras, LM Studio, llama.cpp
- **152 tests**, all passing
- **13.6K lines** of pure Rust

### 🔐 Safety & Budget
- `--max-budget-usd` — stop when cost exceeds limit
- `--max-turns` — stop after N model turns
- `--max-tokens-total` — stop after N total tokens
- Read-before-write enforcement — blocks writes to unread files
- Model whitelist/blacklist per provider

### 🛠️ New Tools
| Tool | Description |
|------|-------------|
| `task_create` | Spawn background shell commands |
| `task_get/list/stop` | Poll, list, terminate background tasks |
| `batch_edit` | Multiple file edits in one call |
| `apply_patch` | Apply unified diff patches |
| `todo_write` | LLM-managed TODO list |
| `ask_user` | Pause and ask user for clarification |
| `sleep` | Pause execution (polling loops) |
| `notebook_edit` | Edit Jupyter notebook cells |
| `worktree_enter/exit` | Isolated git branch work |
| `mcp_list_resources` | List MCP server resources |
| `mcp_read_resource` | Read MCP server resources |

### 🌐 Provider Expansion
- `/connect` wizard — guided setup with API key URLs
- `/providers` — list all providers with config status
- Auto-detect provider from model name
- Interactive `/model` picker with pricing info

### 📋 New Commands
| Command | Description |
|---------|-------------|
| `/connect` | Provider setup wizard |
| `/providers` | List configured providers |
| `/security-review` | Security audit prompt |
| `/resume` | Resume sessions with fuzzy search |
| `/tasks` | Manage background tasks |
| `/agents` | Manage sub-agents |
| `/cron` | Scheduled triggers |
| `/update` | Check for new version |

### 🏗️ Architecture
- Commands module extracted (app.rs -60%)
- All slash commands non-blocking (async RunShell)
- Hierarchical AGENTS.md/CLAUDE.md loading (root→cwd)
- @include directive in instruction files
- `--add-dir` for extra directory access
- Config hot-reload via mtime polling
- MCP auto-reconnect on server crash
- 9 command aliases (/h, /?, /q, /exit, /new, /reset, /continue, /v, /settings)
