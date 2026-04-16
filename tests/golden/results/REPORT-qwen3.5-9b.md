# Golden Test Report: vllm/qwen3.5-9b

**Date:** 2026-04-16
**Model:** Qwen 3.5 9B AWQ (QuantTrio/Qwen3.5-9B-AWQ)
**Backend:** vLLM with qwen3_coder parser, thinking disabled for tool calls
**Hardware:** RTX 4070 Ti, 256K context
**mc-code version:** 1.7.0

## Summary

| Test Suite | Pass | Total | Rate |
|-----------|:----:|:-----:|:----:|
| Platform scenarios (verified) | 66 | 110 | **60%** |
| Multi-turn sessions (verified) | 6 | 10 | **60%** |

## Platform Scenarios (single-turn, with L2 verification)

| Platform | Verified | Total | Rate |
|----------|:--------:|:-----:|:----:|
| Python FastAPI | 16 | 23 | **69%** |
| Python Tkinter | 19 | 23 | **82%** |
| Go stdlib | 15 | 22 | **68%** |
| React/TypeScript | 6 | 21 | **28%** |
| React Native | 10 | 21 | **47%** |

### Strengths
- Single file edits (read → edit): ~90%
- Bug fixes with clear descriptions: ~85%
- Python code generation: 69-82%
- Memory read/write: 100%
- Tool selection: correct tool chosen ~80% of time

### Weaknesses
- File creation: model sometimes uses bash instead of write_file (~13% failure)
- Complex TypeScript/JSX: 28-47%
- Abstract patterns (ABC, generics): ~50%
- Text generation: minimal output tokens, mostly tool actions

### Common failure patterns
1. `bash,bash,bash,bash` instead of `write_file` for new files
2. Read file but stop without editing (multi-turn incomplete)
3. Edit with wrong old_string (whitespace mismatch)

## Multi-turn Sessions (shared context, --batch mode)

| Session | Turns | Duration | Max Context | Verify |
|---------|:-----:|:--------:|:-----------:|:------:|
| python-webapp-feature | 9 | ~90s | 24K | ✅ |
| python-webapp-refactor | 8 | 113s | 22K | ❌ |
| react-webapp-feature | 8 | 183s | 20K | ❌ |
| react-webapp-refactor | 7 | 154s | 23K | ✅ |
| go-webapp-feature | 9 | 212s | 30K | ✅ |
| go-webapp-quality | 7 | 109s | 21K | ✅ |
| python-desktop-feature | 8 | 164s | 31K | ❌ |
| react-mobile-feature | 8 | 464s | 48K | ✅ |
| debug-session | 6 | 81s | 11K | ❌ |
| plan-then-execute | 6 | 90s | 19K | ✅ |

### Context growth
- Turn 1: ~5K tokens (system + tools + first prompt)
- Turn 5: ~10-15K tokens
- Turn 9: ~25-30K tokens
- Max observed: 48K (react-mobile, 8 turns)
- No compaction triggered (256K window, max usage 19%)

### Multi-turn observations
- Session history correctly accumulates across turns
- Model references earlier edits without re-reading files
- plan-then-execute pattern works: plan in turn 1-2, implement in 3-5
- Failures mostly in complex refactoring (ABC, error modules)

## Prompt Tuning Impact

| Change | Before | After |
|--------|:------:|:-----:|
| "bash ONLY for tests/git" | 19% file creation | **87%** |
| "use write_file for new files" | 0% React components | **100%** |
| "NEVER stop after reading" | 25% complex edits | **75%** |
| enable_thinking=false for tools | 0% multi-turn | **100%** |

## Configuration

```bash
vllm serve QuantTrio/Qwen3.5-9B-AWQ \
    --port 8300 \
    --gpu-memory-utilization 0.95 \
    --max-model-len 262144 \
    --quantization awq_marlin \
    --language-model-only \
    --enable-prefix-caching \
    --trust-remote-code \
    --reasoning-parser qwen3 \
    --enable-auto-tool-choice \
    --tool-call-parser qwen3_coder \
    --served-model-name qwen3.5-9b
```

mc-code sends `chat_template_kwargs: {"enable_thinking": false}` when tools are present.
