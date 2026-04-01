# ADR-001: Monorepo Workspace Structure

## Status

Accepted

## Context

magic-code is a TUI agentic AI coding agent with multiple concerns:
configuration, LLM providers, tool execution, conversation runtime, and
terminal UI. We need to decide how to organize the codebase.

Options considered:

1. **Single crate** — simplest, but poor separation of concerns, slow
   compile times as project grows, hard for contributors to navigate.

2. **Monorepo workspace** — multiple crates in one repo, shared CI,
   single version, clear boundaries. Used by ripgrep, bat, delta, and
   most successful Rust CLI tools.

3. **Multi-repo** — maximum isolation, but coordination overhead for
   cross-cutting changes, version synchronization pain, harder CI.

## Decision

Monorepo Cargo workspace with 6 crates:

- `mc-cli` — binary, entrypoint
- `mc-tui` — terminal UI
- `mc-core` — conversation runtime
- `mc-provider` — LLM providers
- `mc-tools` — tool execution
- `mc-config` — configuration

## Consequences

- Single `cargo test --workspace` runs all tests
- Shared `Cargo.lock` ensures dependency consistency
- Crate boundaries enforce separation of concerns at compile time
- Contributors can work on one crate without understanding all others
- Workspace-level lints and formatting ensure consistency
