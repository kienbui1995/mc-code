# Golden Tests

Reproducible test suite for evaluating mc-code with different LLM models.

## Structure

```
tests/golden/
├── fixtures/setup.sh       # Creates todo-api project (deterministic)
├── scenarios/scenarios.json # 30 test scenarios, 9 categories
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

## Categories (30 scenarios)

| Category | Count | Tests |
|----------|-------|-------|
| tool_calling | 5 | Basic tool selection |
| single_edit | 3 | Read → edit single file |
| multi_file_edit | 2 | Changes across files |
| bug_fix | 3 | Find and fix bugs |
| context_awareness | 4 | CLAUDE.md, instructions, repo map |
| memory | 3 | Memory read/write |
| idiomatic_rust | 3 | Code quality |
| complex_task | 3 | Multi-step planning |
| error_recovery | 3 | Graceful failure handling |
| write_tests | 3 | Test generation |

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
