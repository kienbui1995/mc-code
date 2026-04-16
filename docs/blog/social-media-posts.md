# Social Media Posts for magic-code Blog

## Twitter/X (thread)

### Tweet 1 (hook)
I built an open-source AI coding agent in Rust that runs on a $600 GPU.

Tested it with 274 scenarios across Python, React, Go.

Honest results: 60% verified correct with Qwen 3.5 9B. Here's what I learned 🧵

### Tweet 2
magic-code is a TUI agent — not just autocomplete.

It reads your code, plans changes, edits files, runs tests, and iterates.

30 built-in tools. 15+ LLM providers. 9.1MB binary. 0ms startup.

MIT licensed: github.com/kienbui1995/mc-code

### Tweet 3
The hard part: making a 9B model work as a coding agent.

3 lessons:
1. Small models need explicit instructions ("read file X then edit with Y")
2. Thinking mode + tool calling don't mix (Qwen team confirms this)
3. Context window > model size for real coding tasks

### Tweet 4
We built a golden test suite: 274 scenarios, 5 platforms, Docker sandbox.

Not just "did it respond" — we verify actual file contents.

Results (Qwen 3.5 9B):
• Python: 69-82% ✅
• Go: 68% ✅
• React: 28-47% ⚠️

Honest numbers, not marketing.

### Tweet 5
The beauty: switch models with one flag.

Self-hosted (free):
magic-code --model vllm/qwen3.5-9b "fix the bug"

Cloud (when needed):
magic-code --model gemini-2.5-pro "refactor the module"

Your code stays on your machine.

### Tweet 6
Try it:
cargo install magic-code

Or: curl -fsSL https://raw.githubusercontent.com/kienbui1995/mc-code/main/install.sh | sh

Star if useful ⭐ github.com/kienbui1995/mc-code

---

## LinkedIn

🔧 I built an open-source AI coding agent that runs on your own hardware.

After 20 releases and 59 merged PRs, I want to share what I learned building magic-code — a TUI agentic coding assistant written in Rust.

**The challenge:** Make a 9B parameter model (Qwen 3.5, running on a single RTX 4070 Ti) work as a real coding agent — not just autocomplete, but reading code, planning changes, editing files, and running tests.

**What we built:**
• 30 built-in tools (file ops, search, bash, browser, memory)
• 4-tier prompt system adapting to model capability
• Smart context compaction for long sessions
• Docker-sandboxed golden test suite with 274 scenarios

**Honest results:**
We tested across Python (FastAPI, Tkinter), React, React Native, and Go projects. With content verification — not just "did it respond" but "did it write correct code."

Qwen 3.5 9B (self-hosted, free): 60% verified correct
Gemini 2.5 Pro (cloud): 95% verified correct

The 9B model handles Python and Go well. It struggles with complex TypeScript. But it costs nothing to run and your code never leaves your machine.

**Key technical insight:** Qwen 3.5's thinking mode conflicts with tool calling — the model puts tool calls inside reasoning blocks where the parser can't find them. Disabling thinking during tool use (as Qwen team recommends) solved multi-turn agent loops completely.

MIT licensed. Try it: cargo install magic-code

GitHub: https://github.com/kienbui1995/mc-code

#OpenSource #AI #Rust #CodingAgent #SelfHosted #LLM

---

## Reddit (r/rust, r/LocalLLaMA, r/programming)

### Title
magic-code: Open-source TUI coding agent in Rust — tested with 274 scenarios, honest benchmarks with self-hosted Qwen 3.5 9B

### Body
I've been building magic-code, an agentic AI coding assistant that runs in your terminal. Written in Rust, MIT licensed, works with any LLM provider.

**What it does:** ReAct loop agent — reads code, plans, edits files, runs tests, iterates. 30 tools, 15+ providers, 9.1MB static binary.

**The interesting part:** I optimized it for self-hosted Qwen 3.5 9B on a single RTX 4070 Ti. Some findings:

1. Small models need a different prompt strategy — explicit tool instructions vs. letting the model figure it out
2. Qwen 3.5's thinking mode puts tool calls inside reasoning blocks, breaking vLLM's parser. Fix: disable thinking during tool use (Qwen team recommends this)
3. Built a 274-scenario golden test suite with Docker sandboxes and content verification

**Honest benchmarks (verified correct, not just "responded"):**

| Platform | Qwen 3.5 9B (free) | Gemini 2.5 Pro |
|----------|:---:|:---:|
| Python FastAPI | 69% | 96% |
| Python Tkinter | 82% | 95% |
| Go stdlib | 68% | 96% |
| React/TS | 28% | 100% |
| React Native | 47% | 90% |

The 9B model won't match frontier models, but it's free, private, and handles Python/Go tasks well.

Install: `cargo install magic-code`
GitHub: https://github.com/kienbui1995/mc-code

Happy to answer questions about the architecture, testing approach, or self-hosted optimization.

---

## Hacker News

### Title
Show HN: magic-code – Open-source TUI coding agent in Rust, self-hostable with Qwen 3.5

### Body
magic-code is a terminal-based AI coding agent. It runs a ReAct loop with 30 tools (file ops, search, bash, browser, memory) and works with any OpenAI-compatible LLM endpoint.

Built in Rust. 9.1MB static binary. MIT licensed.

I focused on making it work well with self-hosted Qwen 3.5 9B (RTX 4070 Ti). The main technical challenge was that Qwen's thinking mode conflicts with tool calling in vLLM — tool calls end up in reasoning blocks where the parser can't extract them. The fix was straightforward: disable thinking when tools are present.

I built a golden test suite with 274 scenarios across Python, React, Go, and mobile projects. Each scenario runs in a Docker sandbox and verifies actual file contents, not just model output.

Results with Qwen 3.5 9B: 60% verified correct overall. Python/Go tasks work well (68-82%). React/TypeScript is harder for a 9B model (28-47%).

https://github.com/kienbui1995/mc-code
