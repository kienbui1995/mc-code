# Golden Tests

Reproducible test suite for evaluating mc-code with different LLM models.

## Results Summary

| Model | Platform Verified | Sessions | Best For |
|-------|:-----------------:|:--------:|----------|
| Qwen 3.5 9B (RTX 4070 Ti) | 59% | 6/10 | Quick tasks, Python |
| **Qwen 3.5 27B (A40)** | **88%** | **7/10** | **All-round self-hosted** |
| Gemini 2.5 Pro (cloud) | ~95% | N/A | Maximum quality |

## Structure

```
tests/golden/
├── fixtures/                    # Project templates (6 platforms)
│   ├── setup.sh                 # Rust todo-api
│   ├── python-webapp/setup.sh   # FastAPI
│   ├── react-webapp/setup.sh    # React/TypeScript
│   ├── go-webapp/setup.sh       # Go stdlib
│   ├── python-desktop/setup.sh  # Tkinter
│   └── react-mobile/setup.sh    # React Native
├── scenarios/
│   ├── scenarios.json           # 154 core scenarios (22 categories)
│   ├── platform-scenarios.json  # 120 platform scenarios (5 platforms)
│   └── session-scenarios.json   # 76 multi-turn session turns (10 sessions)
├── run.sh                       # Core test runner (--parallel N)
├── run-platform.sh              # Platform test runner (--parallel N, verification)
├── run-session.sh               # Multi-turn session runner
├── verify.py                    # Content verification (L1/L2)
├── compare.py                   # Cross-model comparison
└── results/                     # JSONL results + reports
    ├── REPORT-qwen3.5-9b.md
    └── REPORT-qwen3.5-27b.md
```

## Test Coverage: 350 test points

| Suite | Count | Type |
|-------|:-----:|------|
| Core scenarios | 154 | Single-turn, 22 categories |
| Platform scenarios | 120 | 5 platforms × 24, with L2 verification |
| Session scenarios | 76 | 10 multi-turn sessions (6-9 turns each) |

### Core categories (22)
tool_calling, single_edit, multi_file_edit, bug_fix, context_awareness, memory, idiomatic_rust, complex_task, error_recovery, write_tests, mcp_tools, mode_switching, subagent_parallel, debug_mode, advanced_tools, security, auto_skill, compaction, headless, edge_cases, selective_tools, browser_web

### Platforms (5)
Python FastAPI, React/TypeScript, Go stdlib, Python Tkinter, React Native

### Verification levels
- **L0**: Model produces output (tool calls + text)
- **L1**: Expected files exist
- **L2**: Files contain expected code patterns

## Usage

```bash
# Platform tests (all platforms, parallel)
./run-platform.sh --platform all \
  --model vllm/qwen3.5-27b \
  --base-url http://192.168.3.60:4000 \
  --api-key sk-xxx \
  --parallel 2

# Single platform
./run-platform.sh --platform python-webapp --model vllm/qwen3.5-27b ...

# Multi-turn sessions (all)
./run-session.sh --model vllm/qwen3.5-27b ...

# Single session
./run-session.sh --session plan-then-execute --model vllm/qwen3.5-27b ...

# Core tests
./run.sh --model vllm/qwen3.5-27b --parallel 3 ...

# Compare models
./compare.py results/
```

## Adding new scenarios

Platform scenarios: edit `scenarios/platform-scenarios.json`
```json
{"id": "PW25", "prompt": "your prompt here", "verify": {
  "file_exists": ["path/to/expected/file"],
  "file_contains": {"path/to/file": ["expected", "patterns"]},
  "file_not_contains": {"path/to/file": ["unwanted_pattern"]}
}}
```

Session scenarios: edit `scenarios/session-scenarios.json`
```json
{"fixture": "python-webapp", "turns": ["turn 1", "turn 2", ...], "verify": {...}}
```
