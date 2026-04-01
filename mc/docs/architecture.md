# magic-code Architecture

## Crate Map

```
mc-cli          Binary crate. CLI arg parsing, entrypoint, wires everything together.
                Error strategy: anyhow
                Depends on: mc-tui, mc-core, mc-provider, mc-tools, mc-config

mc-tui          TUI layer built on ratatui + crossterm.
                Rendering, input handling, layout, syntax highlighting.
                Depends on: mc-core

mc-core         Conversation runtime, agent loop, session management,
                compaction, hooks, subagent spawning.
                Depends on: mc-provider, mc-tools, mc-config

mc-provider     LLM provider implementations (Anthropic, OpenAI, Gemini, Ollama).
                Streaming SSE, retry logic, request/response conversion.
                Error strategy: thiserror (ProviderError)
                No internal dependencies.

mc-tools        Tool execution (bash, file ops, glob, grep, MCP client).
                Permission checking, diff generation.
                Error strategy: thiserror (ToolError)
                No internal dependencies.

mc-config       Configuration loading (TOML), project context discovery,
                instruction file reading, stack detection.
                Error strategy: thiserror (ConfigError)
                No internal dependencies.
```

## Dependency Graph

```
mc-cli
├── mc-tui
│   └── mc-core
│       ├── mc-provider
│       ├── mc-tools
│       └── mc-config
├── mc-core (direct for non-TUI mode)
├── mc-provider (direct for provider construction)
├── mc-tools (direct for tool registration)
└── mc-config (direct for config loading)
```

## Data Flow

```
User Input
    │
    ▼
mc-cli (parse args, load config)
    │
    ▼
mc-tui (event loop, render)  ◄──── or stdout for non-interactive mode
    │
    ▼
mc-core::ConversationRuntime
    │
    ├──► mc-provider::stream(request) ──► LLM API ──► ProviderEvent stream
    │
    ├──► mc-tools::execute(tool_name, input) ──► tool output
    │       │
    │       └──► mc-tools::PermissionPolicy::authorize() ──► allow/deny/prompt
    │
    ├──► mc-core::Compaction (when context nears limit)
    │
    └──► mc-core::Session (persist/restore)
```

## Key Design Decisions

1. **Concrete providers first, trait extraction later** — We implement
   AnthropicProvider concretely in Task 1. The LlmProvider trait will be
   extracted when we add OpenAI in Task 8, ensuring the trait is shaped
   by real requirements rather than speculation.

2. **Library crates use thiserror, binary uses anyhow** — Standard Rust
   pattern. Library errors are typed and matchable. The CLI converts
   everything to anyhow for display.

3. **Config is TOML, not JSON** — More human-friendly for a tool that
   developers edit by hand. Layered merge: global → project → local.

4. **TUI is ratatui, not raw crossterm** — ratatui provides widget
   abstractions, layout engine, and TestBackend for testing. Worth the
   dependency.
