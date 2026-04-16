# Golden Test Report: Qwen 3.5 27B (Optimized)

**Date:** 2026-04-16
**Model:** Qwen 3.5 27B AWQ (QuantTrio/Qwen3.5-27B-AWQ)
**Backend:** vLLM on A40, qwen3_coder parser, speculative decoding
**Context:** 256K tokens
**mc-code tier:** 2 (strong) — 25 tools
**mc-code version:** 1.7.0+

## Summary

| Test Suite | Pass | Total | Rate |
|-----------|:----:|:-----:|:----:|
| Platform scenarios (verified) | ~97 | 110 | **~88%** |
| Multi-turn sessions (verified) | 7 | 10 | **70%** |

## Optimization Journey

### Tier promotion: 4 → 2
Qwen 27B+ promoted from tier 4 (10 tools, restricted prompt) to tier 2 (25 tools, autonomous prompt).

| Metric | Tier 4 | Tier 2 | Change |
|--------|:------:|:------:|:------:|
| Tools available | 10 | 25 | +15 |
| edit_plan | ❌ | ✅ | Planning |
| batch_edit | ❌ | ✅ | Multi-file |
| subagent | ❌ | ✅ | Delegation |
| debug | ❌ | ✅ | Debugging |
| Platform avg | 59% | **88%** | **+29%** |

### Auto-continue for read-only turns
Model sometimes reads files then stops without editing. New feature: when a turn ends with only read tools, mc-code injects "Now continue and make the changes."

| Before | After |
|:------:|:-----:|
| 11 read-only failures | 4 fixed, 7 already pass |

### Verify rule improvements
Relaxed 13 verify patterns to reduce false negatives (model implements correctly but with different keywords).

## Platform Results (verified, 27B tier 2)

| Platform | Verified | Rate | vs 9B |
|----------|:--------:|:----:|:-----:|
| Python FastAPI | ~19/22 | **86%** | +17% |
| Python Desktop | ~21/23 | **91%** | +9% |
| Go Web API | ~20/23 | **87%** | +19% |
| React Web | ~17/21 | **81%** | +53% |
| React Native | ~18/21 | **86%** | +39% |

### Strengths (27B)
- Uses `edit_plan` before complex changes
- Uses `batch_edit` for multi-file renames
- Uses `task_create` for tracking work
- Generates 3-10x more output tokens (better explanations)
- Handles TypeScript/JSX significantly better than 9B

### Remaining weaknesses
- Drag-and-drop implementation (complex UI patterns)
- Some file creation still uses bash instead of write_file (~5%)
- Debug session: fixes one bug but misses second

## Multi-turn Sessions

| Session | Turns | Max Context | Duration | Verify |
|---------|:-----:|:-----------:|:--------:|:------:|
| python-webapp-feature | 9 | 20K | 215s | ✅ |
| python-webapp-refactor | 8 | 56K | 678s | ✅ |
| react-webapp-feature | 8 | 15K | 208s | ✅ |
| react-webapp-refactor | 7 | — | timeout | — |
| go-webapp-feature | 9 | — | timeout | — |
| go-webapp-quality | 7 | 35K | 434s | ✅ |
| python-desktop-feature | 8 | 35K | 373s | ✅ |
| react-mobile-feature | 8 | — | timeout | — |
| debug-session | 6 | 15K | 173s | ❌ |
| plan-then-execute | 6 | 17K | 197s | ✅ |

### Session observations
- Context grows ~3-5K per turn (vs ~1-2K for 9B)
- Max observed: 56K tokens (python-webapp-refactor, 8 turns)
- 256K context = ~50+ turns before compaction needed
- Timeouts on complex sessions due to 27B inference speed

## Model Comparison

| | Qwen 9B | **Qwen 27B** | Gemini 2.5 Pro |
|--|:-------:|:------------:|:--------------:|
| Cost | Free (4070 Ti) | **Free (A40)** | Pay |
| Platform avg | 59% | **88%** | ~95% |
| Sessions | 6/10 | **7/10** | N/A |
| React | 28-47% | **81-86%** | 90-100% |
| Go | 68% | **87%** | 96% |
| Python | 69-82% | **86-91%** | 95-96% |
| Tools | 10 | **25** | 25 |
| Speed | ~10s | ~35s | ~15s |
| Context | 256K | **256K** | 1M |

## Configuration

```bash
vllm serve QuantTrio/Qwen3.5-27B-AWQ \
    --port 8300 \
    --quantization awq_marlin \
    --gpu-memory-utilization 0.95 \
    --max-model-len 262144 \
    --enable-prefix-caching \
    --language-model-only \
    --trust-remote-code \
    --reasoning-parser qwen3 \
    --enable-auto-tool-choice \
    --tool-call-parser qwen3_coder \
    --speculative-config '{"method":"qwen3_next_mtp","num_speculative_tokens":2}' \
    --served-model-name qwen3.5-27b
```

## Key Technical Decisions

1. **enable_thinking=false for tool calls** — Qwen team recommended. vLLM parser can't extract tool calls from reasoning blocks.
2. **Tier 2 for 27B+** — 27B is capable enough for autonomous operation with 25 tools.
3. **Auto-continue** — Nudges model to complete task when it stops after reading.
4. **qwen3_coder parser** — Only parser compatible with Qwen 3.5 tool call format.
