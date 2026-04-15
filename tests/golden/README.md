# Golden Tests

Reproducible test suite for evaluating mc-code with different LLM models.

## Structure

```
tests/golden/
├── fixtures/setup.sh       # Creates todo-api project (deterministic)
├── scenarios/scenarios.json # 77 test scenarios, 22 categories
├── run.sh                  # Test runner (Docker sandbox)
├── compare.py              # Compare results across models/runs
└── results/                # JSONL output files
```

## Usage

```bash
# Run all scenarios
./run.sh --model vllm/qwen3.5-9b --base-url http://192.168.3.60:4000 --api-key sk-xxx

# Run specific category
./run.sh --model vllm/qwen3.5-9b --base-url ... --api-key ... --category bug_fix

# Compare results
./compare.py results/
```

## Categories (77 scenarios)

| Category | Count | Description |
|----------|:-----:|-------------|
| tool_calling | 5 | Basic tool selection and execution |
| single_edit | 3 | Read → edit single file |
| multi_file_edit | 2 | Changes across files |
| bug_fix | 3 | Find and fix bugs |
| context_awareness | 4 | CLAUDE.md, instructions, repo map |
| memory | 3 | Memory read/write across invocations |
| idiomatic_rust | 3 | Code quality and Rust idioms |
| complex_task | 3 | Multi-step planning |
| error_recovery | 3 | Graceful failure handling |
| write_tests | 3 | Test generation |
| mcp_tools | 4 | MCP server integration |
| mode_switching | 5 | Plan/agent/model mode switching |
| subagent_parallel | 5 | Subagent spawning, parallel execution |
| debug_mode | 3 | Hypothesis-driven 4-phase debugging |
| advanced_tools | 5 | batch_edit, apply_patch, codebase_search, edit_plan, lsp_query |
| security | 4 | Prompt injection, dangerous commands, permissions |
| auto_skill | 2 | Automatic skill generation |
| compaction | 3 | Context management under token pressure |
| headless | 4 | --json, --pipe, --ndjson, exit codes |
| edge_cases | 5 | Vietnamese, ambiguous, large files, undo, conflicts |
| selective_tools | 2 | Tier-based tool filtering |
| browser_web | 3 | Web search, fetch, browser |

## Output format

Each scenario produces a JSONL line:
```json
{
  "id": "SE01",
  "category": "single_edit",
  "model": "vllm/qwen3.5-9b",
  "tools": ["read_file", "edit_file"],
  "input_tokens": 5098,
  "output_tokens": 130,
  "iterations": 3,
  "duration_sec": 8,
  "files_changed": ["/workspace/src/model.rs"],
  "has_output": true
}
```

## Comparing models

```bash
# Run with different models
./run.sh --model vllm/qwen3.5-9b --api-key ... 
./run.sh --model claude-sonnet-4-5 --api-key ...

# Compare
./compare.py results/
```
