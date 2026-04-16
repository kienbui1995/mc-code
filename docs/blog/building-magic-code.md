# Building magic-code: An Open-Source AI Coding Agent That Runs on Your Hardware

*How we built a TUI coding agent in Rust, tested it with 274 scenarios across 5 platforms, and made it work with a $600 GPU.*

---

## The Problem

AI coding assistants are powerful — but they come with trade-offs. Cloud-based tools send your code to external servers. Proprietary agents lock you into specific providers. And the costs add up fast.

We wanted something different: a coding agent that's **fast, private, and runs on your own hardware**. That's why we built [magic-code](https://github.com/kienbui1995/mc-code).

## What is magic-code?

magic-code is an open-source TUI (terminal UI) agentic AI coding agent built in Rust. It works with any LLM provider — from Claude and GPT to self-hosted models like Qwen 3.5 on your own GPU.

```
$ magic-code "add error handling to the API endpoints"
```

The agent reads your code, plans changes, edits files, runs tests, and iterates — all from your terminal.

### Key numbers

| Metric | Value |
|--------|-------|
| Language | Rust |
| Binary size | 9.1 MB (static musl) |
| Startup time | 0ms |
| Built-in tools | 30 |
| Test coverage | 274 unit tests |
| Golden test scenarios | 274 |
| Supported providers | 15+ |
| Lines of code | 18,691 |
| License | MIT |

## Architecture: 6 Crates, Zero Coupling

```
mc-cli      → Binary, TUI runner, provider selection
mc-tui      → Terminal UI (ratatui), no dependencies on other mc-* crates
mc-core     → Runtime, ReAct loop, agents, memory, compaction
mc-provider → LLM providers (Anthropic, OpenAI, Gemini, generic)
mc-tools    → 30 tool implementations, permissions, sandbox
mc-config   → Configuration types and loader
```

The strict rule: `mc-provider` and `mc-tools` never depend on each other. Only `mc-core` orchestrates them. This keeps the codebase maintainable as it grows.

## The Self-Hosted Challenge

Our primary goal was making magic-code work well with **Qwen 3.5 9B** — a model that runs on a single RTX 4070 Ti. This is a fundamentally different challenge than building for Claude or GPT-4.

### What we learned

**1. Small models need explicit instructions**

With Claude, you can say "add a greet function" and it figures out the rest. With Qwen 9B, you need "read src/lib.rs then add a greet function using edit_file." We built a 4-tier prompt system that adapts instructions based on model capability:

- **Tier 1** (Frontier: Claude, GPT-4): Full autonomy, 30 tools
- **Tier 2** (Strong: Gemini, DeepSeek): Slightly more structured
- **Tier 3** (Local: Llama, Mistral): Minimal tools, simple English
- **Tier 4** (Qwen): Optimized for agentic tool calling, 10 tools

**2. Thinking mode and tool calling don't mix (yet)**

We discovered that Qwen 3.5 with vLLM's `--reasoning-parser qwen3` puts tool calls inside thinking blocks — which the tool call parser can't extract. The fix: disable thinking when tools are present, re-enable for pure Q&A. This is actually [recommended by the Qwen team](https://qwenlm.github.io/blog/qwen3/).

**3. Context window matters more than model size**

Qwen 3.5 9B with 256K context on vLLM outperforms larger models with smaller context windows for real coding tasks. We added Qwen to our model registry with proper context window settings and adaptive compaction thresholds.

## Testing: 274 Scenarios, 5 Platforms, Honest Results

We built a comprehensive golden test suite to evaluate magic-code across different languages and app types. Every scenario runs in a Docker sandbox with a fresh project, and results are verified by checking actual file contents — not just "did the model respond."

### Test structure

```
tests/golden/
├── fixtures/          # 6 project templates (Rust, Python, React, Go, etc.)
├── scenarios/         # 274 scenarios across 22 categories
├── run.sh             # Parallel test runner (Docker sandbox)
├── run-platform.sh    # Platform-specific runner
├── verify.py          # Content verification (L1/L2 checks)
└── compare.py         # Cross-model comparison
```

### Verification levels

We don't just check if the model responded. We verify:

- **L0**: Did the model produce output? (tool calls + text)
- **L1**: Does the expected file exist?
- **L2**: Does the file contain the expected code patterns?

### Results: Qwen 3.5 9B (self-hosted, RTX 4070 Ti)

| Platform | L0 (responds) | L2 (verified correct) |
|----------|:-------------:|:---------------------:|
| Python Web API (FastAPI) | 100% | **69%** |
| Python Desktop (Tkinter) | 100% | **82%** |
| Go Web API | 100% | **68%** |
| React Web App | 100% | **28%** |
| React Native Mobile | 100% | **47%** |

**Overall: 60% verified correct across 110 platform scenarios.**

We're sharing these numbers honestly. A 9B model on a single GPU won't match Claude Sonnet — but it handles Python and Go tasks well, and it costs nothing to run.

### Where Qwen 9B excels

- ✅ Single file edits (add function, fix bug)
- ✅ Python code (FastAPI, Tkinter)
- ✅ Go code (stdlib HTTP, tests)
- ✅ Bug fixes with clear descriptions
- ✅ Reading and understanding code

### Where it struggles

- ❌ Creating new files from scratch (often runs bash instead of write_file)
- ❌ Complex TypeScript/JSX (React components)
- ❌ Multi-step refactoring
- ❌ Abstract patterns (ABC, generics, advanced types)

### Comparison: Gemini 2.5 Pro via LiteLLM

| Platform | Qwen 3.5 9B | Gemini 2.5 Pro |
|----------|:-----------:|:--------------:|
| Python Web API | 69% | **96%** |
| React Web App | 28% | **100%** |
| Go Web API | 68% | **96%** |
| Python Desktop | 82% | **95%** |
| React Native | 47% | **90%** |

Gemini 2.5 Pro scores significantly higher — but it's a cloud model. The beauty of magic-code is you can switch between models with a single flag:

```bash
# Self-hosted (free)
magic-code --base-url http://localhost:4000 --model vllm/qwen3.5-9b "fix the bug"

# Cloud (when you need it)
magic-code --model gemini-2.5-pro "refactor the entire module"
```

## What Makes magic-code Different

### 1. Provider agnostic

15+ providers out of the box. Anthropic, OpenAI, Gemini, Groq, DeepSeek, Mistral, Ollama, LiteLLM, vLLM — or any OpenAI-compatible endpoint.

### 2. Full agentic loop

Not just code completion. magic-code runs a ReAct loop: read code → plan → edit → run tests → iterate. It has 30 built-in tools including file operations, search, bash, browser, memory, and MCP support.

### 3. Context engineering

Smart compaction keeps conversations going without losing important context. Repo maps (via tree-sitter) give the model project awareness without reading every file. Memory persists facts across sessions.

### 4. Security by default

- Permission system for dangerous operations
- Sandbox for bash execution
- Prompt injection guards
- Audit logging
- 8 CI security scanners (CodeQL, SonarCloud, cargo-audit, etc.)

### 5. Headless mode

Integrate magic-code into CI/CD pipelines:

```bash
# Auto-fix failing tests
magic-code --yes --json "fix the failing tests" -o result.json

# Batch processing
magic-code --yes --batch tasks.txt

# NDJSON streaming for web apps
magic-code --ndjson "explain auth.rs" | process_events.sh
```

## Installation

```bash
# Quick install (binary)
curl -fsSL https://raw.githubusercontent.com/kienbui1995/mc-code/main/install.sh | sh

# Via cargo
cargo install magic-code

# From source
git clone https://github.com/kienbui1995/mc-code.git
cd mc-code/mc && cargo install --path crates/mc-cli
```

## Self-Hosted Setup

Run Qwen 3.5 9B with vLLM:

```bash
vllm serve QuantTrio/Qwen3.5-9B-AWQ \
    --port 8300 \
    --gpu-memory-utilization 0.95 \
    --max-model-len 262144 \
    --quantization awq_marlin \
    --enable-prefix-caching \
    --reasoning-parser qwen3 \
    --enable-auto-tool-choice \
    --tool-call-parser qwen3_coder \
    --served-model-name qwen3.5-9b
```

Point magic-code at it:

```bash
magic-code --base-url http://localhost:8300 --model qwen3.5-9b "your task"
```

Or use LiteLLM as a proxy to switch between self-hosted and cloud models seamlessly.

## What's Next

- Improving Qwen 3.5 performance on file creation tasks
- Testing with larger self-hosted models (Qwen 32B, Llama 70B)
- HTTP API server for web app integration
- Watch mode (file watcher, auto-respond)

## Try It

magic-code is MIT licensed and available on [GitHub](https://github.com/kienbui1995/mc-code) and [crates.io](https://crates.io/crates/magic-code).

We built this because we believe AI coding tools should be **open, fast, and runnable on your own hardware**. The results aren't perfect — but they're honest, reproducible, and improving with every release.

```bash
cargo install magic-code
```

---

*magic-code is built by [kienbui1995](https://github.com/kienbui1995). Star the repo if you find it useful. Contributions welcome.*
