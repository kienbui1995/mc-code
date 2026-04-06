# magic-code development tasks

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

# === Sandbox Testing ===

# Build and run in Docker sandbox
sandbox *ARGS:
    cargo build --release
    docker build -f Dockerfile.sandbox -t mc-sandbox .
    docker run --rm -it \
        -e MC_PROVIDER="${MC_PROVIDER:-litellm}" \
        -e MC_BASE_URL="${MC_BASE_URL}" \
        -e MC_API_KEY="${MC_API_KEY}" \
        -e ANTHROPIC_API_KEY="${ANTHROPIC_API_KEY}" \
        mc-sandbox {{ARGS}}

# Run full pre-release check
pre-release: fmt clippy test build
    @echo "✅ Pre-release checks passed"
    @echo "Binary: $(ls -lh target/release/magic-code | awk '{print $5}')"
    @echo "Tests: $(cargo test --workspace -q 2>&1 | grep 'test result' | awk '{sum+=$4} END {print sum}')"

# Run smoke test in temp sandbox
smoke-test:
    #!/bin/bash
    set -e
    SANDBOX=$(mktemp -d /tmp/mc-smoke-XXXXXX)
    cd "$SANDBOX" && git init -q
    echo "pub fn main() { println!(\"hello\"); }" > main.rs
    MC="../{{justfile_directory()}}/target/release/magic-code"
    echo "=== Smoke test: version ===" && $MC --version
    echo "=== Smoke test: help ===" && $MC --help | head -5
    echo "=== Smoke test: completions ===" && $MC --completions bash > /dev/null
    rm -rf "$SANDBOX"
    echo "✅ Smoke test passed"
