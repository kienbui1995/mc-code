# Contributing to magic-code

## Setup

```bash
git clone https://github.com/kienbui1995/magic-code
cd magic-code/mc
cargo test --workspace
```

## Development

```bash
# Run with debug logging
cargo run -- -v "hello"

# Run TUI
cargo run

# Check everything
cargo clippy --workspace --all-targets
cargo test --workspace
cargo fmt --all -- --check
```

## Architecture

See `mc/README.md` for crate-level architecture.

Key principles:
- Each crate has clear boundaries enforced at compile time
- `mc-provider` and `mc-tools` have no dependency on each other
- `mc-core` orchestrates everything
- All tool execution is async
- All LLM communication is streaming

## Adding a new tool

1. Add implementation in `mc-tools/src/your_tool.rs`
2. Add `ToolSpec` in `mc-tools/src/spec.rs`
3. Add match arm in `mc-tools/src/registry.rs`
4. Add tests

## Adding a new provider

1. Add implementation in `mc-provider/src/your_provider.rs`
2. Export from `mc-provider/src/lib.rs`
3. Add `LlmProvider` impl in `mc-core/src/runtime.rs`
4. Add CLI dispatch in `mc-cli/src/main.rs`

## Pull Requests

- Run `cargo clippy --workspace` — zero warnings required
- Run `cargo test --workspace` — all tests must pass
- Keep PRs focused — one feature or fix per PR
