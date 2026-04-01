# ADR-003: Permission Model

## Status

Proposed

## Context

Current state: `PermissionPolicy` supports Allow/Deny/Prompt modes per tool, but:
- CLI always uses `PermissionMode::Allow` (no safety)
- Prompt mode passes `None` as prompter → always Deny
- Config has `permission_mode` (ReadOnly/WorkspaceWrite/FullAccess) but it's not wired
- No workspace sandboxing — tools can operate on any path

For a production tool that executes arbitrary shell commands, this is a security gap.

## Decision

Three-layer permission model:

### Layer 1: Config-driven mode (coarse)

```
read-only        → bash: Deny, write_file: Deny, edit_file: Deny, others: Allow
workspace-write  → bash: Prompt, write/edit inside cwd: Allow, outside: Deny
full-access      → All: Allow (opt-in, must be explicit)
```

Default: `workspace-write`

### Layer 2: Per-tool override (fine)

Config can override any tool:
```toml
[permissions]
default = "workspace-write"

[permissions.tools.bash]
mode = "prompt"

[permissions.tools.read_file]
mode = "allow"
```

### Layer 3: Workspace sandbox (path-based)

All file operations validated against project root:
- Resolve symlinks before checking
- Block path traversal (`../`)
- Allow explicit additional paths via config

### TUI Integration

When Prompt mode triggers, runtime sends `UiMessage::PermissionRequest` to TUI.
TUI shows modal popup. User response sent back via oneshot channel.

```rust
enum UiMessage {
    // ...existing...
    PermissionRequest {
        tool: String,
        summary: String,
        respond: oneshot::Sender<PermissionOutcome>,
    },
}
```

## Consequences

- Default behavior becomes safe (workspace-write)
- Users must opt-in to full-access
- TUI gains interactive permission prompt
- Slight latency for prompted tools (user must respond)
- Workspace sandbox prevents accidental damage outside project
