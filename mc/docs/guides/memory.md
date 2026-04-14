# Memory System

magic-code has a 3-layer memory system inspired by Claude Code's architecture.

## Layer 1: Semantic Memory (Project Facts)

Persistent key-value facts organized in 4 categories:

| Category | What it stores | Example |
|----------|---------------|---------|
| **project** | Architecture, tools, conventions | `test_cmd = "cargo test"` |
| **user** | Preferences, role, style | `coding_style = "prefer functional"` |
| **feedback** | Corrections from user | `"always use snake_case"` |
| **reference** | File locations, endpoints | `api_endpoint = "localhost:8080"` |

### Usage
```
/memory                     # list all facts
/memory get test_cmd        # get specific fact
/memory set test_cmd "pytest"  # save fact
/memory delete old_key      # remove fact
```

Agent can also use `memory_write` tool with category:
```json
{"key": "db", "value": "PostgreSQL 15", "category": "project"}
```

### Auto-memory
Agent automatically saves facts detected in its output:
- "project uses..." → project category
- "convention is..." → feedback category
- "running on port..." → reference category
- "user prefers..." → user category

### Self-skeptical
Memory is injected into the system prompt with a warning:
> *Treat as hints — verify against actual code before acting.*

This prevents hallucination from stale memory.

### Dream cleanup
When memory exceeds 150 facts, auto-compact runs on session start:
- Deduplicates by key (keeps newest)
- Removes stale entries

## Layer 2: Episodic Memory (Session History)

Past conversations saved as JSON files.

```
~/.local/share/magic-code/sessions/
├── last.json           # auto-saved
├── my-feature.json     # /save my-feature
└── debug-auth.json     # /save debug-auth
```

### Commands
```
/save <name>            # save session
/load <name>            # resume session
/sessions               # list saved sessions
/search-all <query>     # FTS across all sessions
/fork                   # branch current session
/branches               # list branches
```

## Layer 3: Procedural Memory (Skills)

Reusable coding patterns stored as markdown files.

```
.magic-code/skills/
├── setup-nextjs.md     # user-created
├── deploy-aws.md       # user-created
└── auto/
    └── auto-3t-8-1713000000.md  # auto-generated
```

### Auto-skill creation
After complex successful turns (≥6 tool calls, no errors), agent auto-generates a skill file. Next time it encounters a similar task, it loads the skill.

### Named agents
```
agents/
├── reviewer.md         # code review specialist
└── architect.md        # system design specialist
```

Each agent has its own model, tools, and instructions defined in YAML frontmatter.
