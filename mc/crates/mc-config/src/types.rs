use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
/// Configerror.
pub enum ConfigError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml parse error: {0}")]
    Parse(#[from] toml::de::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
/// Permissionmode.
pub enum PermissionMode {
    ReadOnly,
    #[default]
    WorkspaceWrite,
    FullAccess,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
/// Providerconfig.
pub struct ProviderConfig {
    pub api_key_env: String,
    pub max_retries: u32,
    pub base_url: Option<String>,
    pub host: Option<String>,
    pub format: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
/// Compactionconfig.
pub struct CompactionConfig {
    pub auto_compact_threshold: Option<f64>,
    pub preserve_recent_messages: Option<usize>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
/// Retryconfig.
pub struct RetryConfig {
    pub max_attempts: Option<u32>,
    pub initial_backoff_ms: Option<u64>,
    pub max_backoff_ms: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
/// Memoryconfig.
pub struct MemoryConfig {
    pub path: Option<String>,
    pub max_facts: Option<usize>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
/// Thinkingconfig.
pub struct ThinkingConfig {
    pub enabled: Option<bool>,
    pub budget_tokens: Option<u32>,
}

/// Raw TOML layer — all fields Optional so we can detect explicit overrides.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
/// Configlayer.
pub struct ConfigLayer {
    pub default: DefaultLayer,
    pub providers: BTreeMap<String, ProviderConfig>,
    pub compaction: CompactionConfig,
    pub retry: RetryConfig,
    pub memory: MemoryConfig,
    pub thinking: ThinkingConfig,
    pub context: ContextLayer,
    #[serde(default)]
    pub mcp_servers: Vec<McpServerConfig>,
    #[serde(default)]
    pub hooks: Vec<HookConfig>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
/// Defaultlayer.
pub struct DefaultLayer {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub max_tokens: Option<u32>,
    pub permission_mode: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
/// Contextlayer.
pub struct ContextLayer {
    pub instruction_files: Option<Vec<String>>,
    pub ignore_patterns: Option<Vec<String>>,
}

// Keep MagicCodeConfig as alias for backward compat
#[allow(dead_code)]
pub type MagicCodeConfig = ConfigLayer;

/// MCP server configuration.
#[derive(Debug, Clone, Deserialize)]
/// Mcpserverconfig.
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

/// Hook configuration.
#[derive(Debug, Clone, Deserialize)]
/// Hookconfig.
pub struct HookConfig {
    pub event: String,
    pub command: String,
    #[serde(default)]
    pub match_tools: Vec<String>,
}

/// Resolved runtime config after merging all layers.
#[derive(Debug, Clone)]
/// Runtimeconfig.
pub struct RuntimeConfig {
    pub provider: String,
    pub fallback_provider: Option<String>,
    pub fallback_model: Option<String>,
    pub model: String,
    pub max_tokens: u32,
    pub permission_mode: PermissionMode,
    pub provider_config: ProviderConfig,
    pub compaction_threshold: f64,
    pub compaction_preserve_recent: usize,
    pub retry_max_attempts: u32,
    pub retry_initial_backoff_ms: u64,
    pub retry_max_backoff_ms: u64,
    pub memory_path: String,
    pub memory_max_facts: usize,
    pub thinking_enabled: bool,
    pub thinking_budget_tokens: u32,
    pub instruction_files: Vec<String>,
    pub ignore_patterns: Vec<String>,
    pub mcp_servers: Vec<McpServerConfig>,
    pub hooks: Vec<HookConfig>,
    pub model_aliases: std::collections::HashMap<String, String>,
    pub protected_patterns: Vec<String>,
}

impl RuntimeConfig {
    #[must_use]
    #[allow(clippy::too_many_lines)]
    /// From layers.
    pub fn from_layers(layers: &[ConfigLayer]) -> Self {
        let mut provider: Option<String> = None;
        let mut model: Option<String> = None;
        let mut max_tokens: Option<u32> = None;
        let mut permission_mode: Option<String> = None;
        let mut providers: BTreeMap<String, ProviderConfig> = BTreeMap::new();
        let mut compact_threshold: Option<f64> = None;
        let mut compact_preserve: Option<usize> = None;
        let mut retry_max: Option<u32> = None;
        let mut retry_initial: Option<u64> = None;
        let mut retry_max_backoff: Option<u64> = None;
        let mut memory_path: Option<String> = None;
        let mut memory_max_facts: Option<usize> = None;
        let mut thinking_enabled: Option<bool> = None;
        let mut thinking_budget: Option<u32> = None;
        let mut instruction_files: Option<Vec<String>> = None;
        let mut ignore_patterns: Option<Vec<String>> = None;
        let mut mcp_servers: Vec<McpServerConfig> = Vec::new();
        let mut hooks: Vec<HookConfig> = Vec::new();

        for layer in layers {
            if let Some(ref v) = layer.default.provider {
                provider = Some(v.clone());
            }
            if let Some(ref v) = layer.default.model {
                model = Some(v.clone());
            }
            if let Some(v) = layer.default.max_tokens {
                max_tokens = Some(v);
            }
            if let Some(ref v) = layer.default.permission_mode {
                permission_mode = Some(v.clone());
            }
            for (name, cfg) in &layer.providers {
                providers.insert(name.clone(), cfg.clone());
            }
            if let Some(v) = layer.compaction.auto_compact_threshold {
                compact_threshold = Some(v);
            }
            if let Some(v) = layer.compaction.preserve_recent_messages {
                compact_preserve = Some(v);
            }
            if let Some(v) = layer.retry.max_attempts {
                retry_max = Some(v);
            }
            if let Some(v) = layer.retry.initial_backoff_ms {
                retry_initial = Some(v);
            }
            if let Some(v) = layer.retry.max_backoff_ms {
                retry_max_backoff = Some(v);
            }
            if let Some(ref v) = layer.memory.path {
                memory_path = Some(v.clone());
            }
            if let Some(v) = layer.memory.max_facts {
                memory_max_facts = Some(v);
            }
            if let Some(v) = layer.thinking.enabled {
                thinking_enabled = Some(v);
            }
            if let Some(v) = layer.thinking.budget_tokens {
                thinking_budget = Some(v);
            }
            if let Some(ref v) = layer.context.instruction_files {
                instruction_files = Some(v.clone());
            }
            if let Some(ref v) = layer.context.ignore_patterns {
                ignore_patterns = Some(v.clone());
            }
            mcp_servers.extend(layer.mcp_servers.clone());
            hooks.extend(layer.hooks.clone());
        }

        let resolved_provider = provider.unwrap_or_else(|| "anthropic".into());
        let provider_config = providers
            .get(&resolved_provider)
            .cloned()
            .unwrap_or_default();

        let perm = match permission_mode.as_deref() {
            Some("read-only") => PermissionMode::ReadOnly,
            Some("full-access") => PermissionMode::FullAccess,
            _ => PermissionMode::WorkspaceWrite,
        };

        Self {
            provider: resolved_provider,
            fallback_provider: None,
            fallback_model: None,
            model: model.unwrap_or_else(|| "claude-sonnet-4-20250514".into()),
            max_tokens: max_tokens.unwrap_or(8192),
            permission_mode: perm,
            provider_config,
            compaction_threshold: compact_threshold.unwrap_or(0.8),
            compaction_preserve_recent: compact_preserve.unwrap_or(4),
            retry_max_attempts: retry_max.unwrap_or(2),
            retry_initial_backoff_ms: retry_initial.unwrap_or(500),
            retry_max_backoff_ms: retry_max_backoff.unwrap_or(5000),
            memory_path: memory_path.unwrap_or_else(|| ".magic-code/memory.json".into()),
            memory_max_facts: memory_max_facts.unwrap_or(50),
            thinking_enabled: thinking_enabled.unwrap_or(false),
            thinking_budget_tokens: thinking_budget.unwrap_or(10_000),
            instruction_files: instruction_files.unwrap_or_default(),
            ignore_patterns: ignore_patterns.unwrap_or_default(),
            mcp_servers,
            hooks,
            model_aliases: std::collections::HashMap::new(),
            protected_patterns: Vec::new(),
        }
    }

    /// Validate config and return warnings for suspicious values.
    #[must_use]
    /// Validate.
    pub fn validate(&self) -> Vec<String> {
        let mut warnings = Vec::new();
        if self.max_tokens == 0 {
            warnings.push("max_tokens is 0, using default 8192".into());
        }
        if !(0.0..=1.0).contains(&self.compaction_threshold) {
            warnings.push(format!(
                "compaction_threshold {} out of range [0,1]",
                self.compaction_threshold
            ));
        }
        if self.compaction_preserve_recent == 0 {
            warnings.push("compaction_preserve_recent is 0, at least 1 recommended".into());
        }
        for mcp in &self.mcp_servers {
            if mcp.command.is_empty() {
                warnings.push(format!("MCP server '{}' has empty command", mcp.name));
            }
        }
        warnings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_when_no_layers() {
        let config = RuntimeConfig::from_layers(&[]);
        assert_eq!(config.provider, "anthropic");
        assert_eq!(config.model, "claude-sonnet-4-20250514");
        assert_eq!(config.max_tokens, 8192);
    }

    #[test]
    fn later_layer_overrides_earlier() {
        let global: ConfigLayer = toml::from_str(
            r#"
[default]
provider = "openai"
model = "gpt-4o"
"#,
        )
        .unwrap();
        let project: ConfigLayer = toml::from_str(
            r#"
[default]
model = "claude-sonnet-4-20250514"
"#,
        )
        .unwrap();
        let config = RuntimeConfig::from_layers(&[global, project]);
        assert_eq!(config.provider, "openai");
        assert_eq!(config.model, "claude-sonnet-4-20250514");
    }

    #[test]
    fn explicit_anthropic_overrides_openai() {
        let global: ConfigLayer = toml::from_str(
            r#"
[default]
provider = "openai"
"#,
        )
        .unwrap();
        let project: ConfigLayer = toml::from_str(
            r#"
[default]
provider = "anthropic"
"#,
        )
        .unwrap();
        let config = RuntimeConfig::from_layers(&[global, project]);
        assert_eq!(config.provider, "anthropic");
    }

    #[test]
    fn parses_minimal_config() {
        let layer: ConfigLayer = toml::from_str(
            r#"
[default]
provider = "anthropic"
"#,
        )
        .unwrap();
        assert_eq!(layer.default.provider.as_deref(), Some("anthropic"));
        assert!(layer.default.model.is_none());
    }

    #[test]
    fn parses_mcp_and_hooks() {
        let layer: ConfigLayer = toml::from_str(
            r#"
[[mcp_servers]]
name = "github"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]

[mcp_servers.env]
GITHUB_TOKEN = "xxx"

[[hooks]]
event = "pre_tool_call"
command = "echo $MC_TOOL_NAME"
match_tools = ["bash"]
"#,
        )
        .unwrap();
        assert_eq!(layer.mcp_servers.len(), 1);
        assert_eq!(layer.mcp_servers[0].name, "github");
        assert_eq!(layer.mcp_servers[0].args.len(), 2);
        assert_eq!(layer.mcp_servers[0].env.get("GITHUB_TOKEN").unwrap(), "xxx");
        assert_eq!(layer.hooks.len(), 1);
        assert_eq!(layer.hooks[0].event, "pre_tool_call");
        assert_eq!(layer.hooks[0].match_tools, vec!["bash"]);
    }
}
