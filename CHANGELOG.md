# Changelog

## v0.1.0 (2025-04-01)

Initial release.

### Features
- Multi-provider support: Anthropic, OpenAI, Gemini, Ollama, LiteLLM, any OpenAI-compatible
- True async streaming — tokens render as they arrive
- 7 built-in tools: bash, read_file, write_file, edit_file, glob_search, grep_search, subagent
- TUI with syntax highlighting (syntect), markdown rendering, scroll, input history
- MCP server support via TOML config
- LLM-based smart context compaction
- Permission modes: read-only, workspace-write, full-access
- Audit log for all tool executions
- Session save/load/resume
- Cost tracking with live status bar estimate
- Pre/post tool call hooks
- Workspace sandboxing
- Cancel with Ctrl+C (per-turn, not app-level)
- Pipe mode for non-interactive use
- Config layering: global → project → local
- Project context auto-discovery (git status, stack detection, instruction files)
