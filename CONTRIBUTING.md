# Contributing to magic-code

## Quick Start

```bash
git clone https://github.com/kienbui1995/mc-code.git
cd mc-code/mc
cargo test --workspace
cargo run -- "hello"
```

## Development

```bash
just check          # fmt + clippy + test (what CI runs)
just run "prompt"   # run in debug mode
just build          # release build
```

## Architecture

6 crates, strict dependency rules:

```
mc-cli → mc-tui, mc-core, mc-provider, mc-tools, mc-config
mc-core → mc-provider, mc-tools, mc-config
mc-tui, mc-provider, mc-tools, mc-config → standalone
```

- `mc-provider` and `mc-tools` must NEVER depend on each other
- `mc-tui` must NEVER depend on mc-core/mc-provider/mc-tools
- Business logic goes in `mc-core`, not `mc-tui`

## Adding Features

- **New tool**: `mc-tools/src/{tool}.rs` → spec in `spec.rs` → dispatch in `registry.rs`
- **New provider**: `mc-provider/src/{provider}.rs` → `LlmProvider` impl in `runtime.rs`
- **New slash command**: `mc-tui/src/app.rs` (add to `PendingCommand` enum) → handle in `mc-cli/src/main.rs`
- **New core feature**: `mc-core/src/{feature}.rs` → integrate in `runtime.rs`

## Code Style

- `unsafe` is forbidden
- Clippy pedantic enabled, zero warnings required
- `#[must_use]` on public functions returning values
- `thiserror` for errors, `anyhow` only in binary crate
- All async uses `tokio`

## CI

- `RUSTFLAGS=-Dwarnings` — all warnings are errors
- Format, clippy, test, release build
- Cross-compile: Linux x86_64, macOS x86_64 + ARM64

## License

MIT
