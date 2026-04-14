# Configuration Reference

## Config file locations (priority order)
1. `.magic-code/config.toml` (project — highest)
2. `~/.config/magic-code/config.toml` (user)
3. Built-in defaults (lowest)

## All options

```toml
[default]
# LLM
model = "claude-sonnet-4-20250514"
max_tokens = 8192
provider = "anthropic"
base_url = ""                    # custom API endpoint
fallback_provider = ""           # secondary provider
fallback_model = ""              # secondary model

# Permissions
permission_mode = "auto"         # auto | allow | deny | prompt

# Context
compaction_threshold = 0.8       # compact at 80% context usage
compaction_preserve_recent = 4   # keep last 4 messages

# Notifications
notifications = true             # bell + desktop notifications
notification_webhook = ""        # Slack/Discord webhook URL

# Managed Agents
[managed_agents]
enabled = false
executor_model = "claude-haiku-3-5-20241022"
executor_max_turns = 5
max_concurrent = 3
budget_usd = 1.0
```

## Environment variables

| Variable | Description |
|----------|-------------|
| `ANTHROPIC_API_KEY` | Anthropic API key |
| `OPENAI_API_KEY` | OpenAI API key |
| `GEMINI_API_KEY` | Google Gemini API key |
| `GROQ_API_KEY` | Groq API key |
| `DEEPSEEK_API_KEY` | DeepSeek API key |
| `OPENROUTER_API_KEY` | OpenRouter API key |
| `XAI_API_KEY` | xAI (Grok) API key |

## CLI flags

```
--model <MODEL>          LLM model
--provider <PROVIDER>    Provider name
--max-tokens <N>         Max tokens per response
--resume                 Resume last session
--session-id <ID>        Resume specific session
--pipe                   Read from stdin
--json                   JSON output mode
--yes                    Auto-approve (CI/CD)
--trace                  Debug logging
--validate-config        Validate and exit
--max-budget-usd <N>     Cost limit
--max-turns <N>          Turn limit
--add-dir <DIR>          Grant access to extra directories
```
