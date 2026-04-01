# ADR-002: Async Streaming Pipeline

## Status

Proposed

## Context

Current providers collect all SSE events into `Vec<ProviderEvent>` before returning.
This means the TUI cannot display tokens as they arrive — the user sees nothing until
the entire response is complete. For a production coding agent, this is unacceptable.

Options:

1. **Callback-based** (current `&mut dyn FnMut(&ProviderEvent)`) — works but couples
   provider to consumer, makes cancellation hard, cannot be used across thread boundaries
   without `Send` bounds on the closure.

2. **Channel-based** (`mpsc::UnboundedSender<ProviderEvent>`) — decouples producer and
   consumer, works across tasks, easy cancellation via dropping sender. But loses
   backpressure.

3. **`Stream` trait** (`Pin<Box<dyn Stream<Item = Result<ProviderEvent, ProviderError>> + Send>>`)
   — idiomatic async Rust, composable with `StreamExt` combinators, supports backpressure.
   Requires `async-stream` or manual `Poll` impl.

4. **Hybrid: Provider returns `Stream`, runtime bridges to channel for TUI** — best of
   both worlds. Provider stays composable, TUI gets simple `recv()`.

## Decision

Option 4: Hybrid approach.

- `LlmProvider` trait returns `Pin<Box<dyn Stream<Item = ...> + Send>>`
- `ConversationRuntime` consumes the stream internally, forwards events to
  `mpsc::UnboundedSender<UiMessage>` for TUI consumption
- `CancellationToken` (from `tokio-util`) passed to runtime, checked between iterations
  and between tool executions

## Consequences

- Providers become truly streaming — each SSE chunk yields immediately
- TUI renders tokens as they arrive (time-to-first-token matches LLM latency)
- Cancel is clean: token cancels the stream + any running tool
- New dependency: `async-stream`, `futures-core`, `tokio-util`
- All provider implementations must be updated (AnthropicProvider, GenericProvider)
- `ToolRegistry::execute` must become async to support cancellation
