# magic-code development tasks
# Usage: just <recipe>

set working-directory := "mc"

# Run all checks (what CI does)
check: fmt clippy test

# Run tests
test:
    cargo test --workspace

# Run clippy
clippy:
    cargo clippy --workspace --all-targets

# Check formatting
fmt:
    cargo fmt --all -- --check

# Auto-fix formatting
fmt-fix:
    cargo fmt --all

# Build release binary
build:
    cargo build --release

# Run TUI in debug mode
run *ARGS:
    cargo run -- {{ARGS}}

# Run with verbose logging
run-verbose *ARGS:
    cargo run -- -v {{ARGS}}

# Clean build artifacts
clean:
    cargo clean

# Watch tests (requires cargo-watch)
watch-test:
    cargo watch -x 'test --workspace'

# Count lines of code (requires tokei)
loc:
    tokei crates/
