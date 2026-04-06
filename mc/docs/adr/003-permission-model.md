# ADR-003: Permission Model

## Status
Accepted

## Context
An agentic coding assistant executes tools that can modify the filesystem, run shell commands, and access the network. Without guardrails, the LLM could accidentally delete files, run destructive commands, or exfiltrate data.

## Decision
Three permission modes, configurable per-project:

### Modes
- **read-only**: Only `read_file`, `glob_search`, `grep_search` allowed. All write/execute tools denied.
- **workspace-write** (default): Read tools auto-allowed. Write tools (`write_file`, `edit_file`) allowed within workspace. `bash` requires user prompt.
- **full-access**: All tools auto-allowed. No prompts.

### Per-tool overrides
Config can override the default mode for specific tools:
```toml
permission_mode = "workspace-write"
# But always allow bash for this project:
# [tool_permissions]
# bash = "allow"
```

### File protection
Sandbox enforces:
- All file operations must be within workspace root (path traversal blocked)
- Protected patterns: `.env`, `*.key`, `*.pem`, `.git/*`, `id_rsa*`
- Configurable via `protected_patterns` in config

### Prompt flow
When a tool requires prompting:
1. Runtime sends `PermissionPrompt` to TUI via channel
2. TUI displays prompt widget: `[Y]es / [N]o / [A]lways`
3. User response sent back via sync channel
4. "Always" adds tool to session allowlist

### Audit
All tool executions logged to `~/.local/share/magic-code/audit.jsonl`:
```json
{"tool":"bash","input":"rm -rf build","output_len":0,"error":false,"ms":12,"allowed":true}
```

## Consequences
- Users can safely use magic-code on production codebases with `read-only` mode
- Default `workspace-write` balances safety and productivity
- `full-access` available for trusted environments
- Audit log enables post-hoc review of all actions
- File protection prevents accidental modification of secrets/credentials
