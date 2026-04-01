use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml parse error: {0}")]
    Parse(#[from] toml::de::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PermissionMode {
    ReadOnly,
    #[default]
    WorkspaceWrite,
    FullAccess,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ProviderConfig {
    pub api_key_env: String,
    pub max_retries: u32,
    pub base_url: Option<String>,
    pub host: Option<String>,
    pub format: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct CompactionConfig {
    pub auto_compact_threshold: Option<f64>,
    pub preserve_recent_messages: Option<usize>,
}

/// Raw TOML layer — all fields Optional so we can detect explicit overrides.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ConfigLayer {
    pub default: DefaultLayer,
    pub providers: BTreeMap<String, ProviderConfig>,
    pub compaction: CompactionConfig,
    pub context: ContextLayer,
    #[serde(default)]
    pub mcp_servers: Vec<McpServerConfig>,
    #[serde(default)]
    pub hooks: Vec<HookConfig>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct DefaultLayer {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub max_tokens: Option<u32>,
    pub permission_mode: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ContextLayer {
    pub instruction_files: Option<Vec<String>>,
    pub ignore_patterns: Option<Vec<String>>,
}

// Keep MagicCodeConfig as alias for backward compat
#[allow(dead_code)]
pub type MagicCodeConfig = ConfigLayer;

/// MCP server configuration.
#[derive(Debug, Clone, Deserialize)]
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
pub struct HookConfig {
    pub event: String,
    pub command: String,
    #[serde(default)]
    pub match_tools: Vec<String>,
}

/// Resolved runtime config after merging all layers.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub provider: String,
    pub model: String,
    pub max_tokens: u32,
    pub permission_mode: PermissionMode,
    pub provider_config: ProviderConfig,
    pub compaction_threshold: f64,
    pub compaction_preserve_recent: usize,
    pub instruction_files: Vec<String>,
    pub ignore_patterns: Vec<String>,
    pub mcp_servers: Vec<McpServerConfig>,
    pub hooks: Vec<HookConfig>,
}

impl RuntimeConfig {
    #[must_use]
    pub fn from_layers(layers: &[ConfigLayer]) -> Self {
        let mut provider: Option<String> = None;
        let mut model: Option<String> = None;
        let mut max_tokens: Option<u32> = None;
        let mut permission_mode: Option<String> = None;
        let mut providers: BTreeMap<String, ProviderConfig> = BTreeMap::new();
        let mut compact_threshold: Option<f64> = None;
        let mut compact_preserve: Option<usize> = None;
        let mut instruction_files: Option<Vec<String>> = None;
        let mut ignore_patterns: Option<Vec<String>> = None;
        let mut mcp_servers: Vec<McpServerConfig> = Vec::new();
        let mut hooks: Vec<HookConfig> = Vec::new();

        for layer in layers {
            if let Some(ref v) = layer.default.provider { provider = Some(v.clone()); }
            if let Some(ref v) = layer.default.model { model = Some(v.clone()); }
            if let Some(v) = layer.default.max_tokens { max_tokens = Some(v); }
            if let Some(ref v) = layer.default.permission_mode { permission_mode = Some(v.clone()); }
            for (name, cfg) in &layer.providers {
                providers.insert(name.clone(), cfg.clone());
            }
            if let Some(v) = layer.compaction.auto_compact_threshold { compact_threshold = Some(v); }
            if let Some(v) = layer.compaction.preserve_recent_messages { compact_preserve = Some(v); }
            if let Some(ref v) = layer.context.instruction_files { instruction_files = Some(v.clone()); }
            if let Some(ref v) = layer.context.ignore_patterns { ignore_patterns = Some(v.clone()); }
            mcp_servers.extend(layer.mcp_servers.clone());
            hooks.extend(layer.hooks.clone());
        }

        let resolved_provider = provider.unwrap_or_else(|| "anthropic".into());
        let provider_config = providers.get(&resolved_provider).cloned().unwrap_or_default();

        let perm = match permission_mode.as_deref() {
            Some("read-only") => PermissionMode::ReadOnly,
            Some("full-access") => PermissionMode::FullAccess,
            _ => PermissionMode::WorkspaceWrite,
        };

        Self {
            provider: resolved_provider,
            model: model.unwrap_or_else(|| "claude-sonnet-4-20250514".into()),
            max_tokens: max_tokens.unwrap_or(8192),
            permission_mode: perm,
            provider_config,
            compaction_threshold: compact_threshold.unwrap_or(0.8),
            compaction_preserve_recent: compact_preserve.unwrap_or(4),
            instruction_files: instruction_files.unwrap_or_default(),
            ignore_patterns: ignore_patterns.unwrap_or_default(),
            mcp_servers,
            hooks,
        }
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
        let global: ConfigLayer = toml::from_str(r#"
[default]
provider = "openai"
model = "gpt-4o"
"#).unwrap();
        let project: ConfigLayer = toml::from_str(r#"
[default]
model = "claude-sonnet-4-20250514"
"#).unwrap();
        let config = RuntimeConfig::from_layers(&[global, project]);
        assert_eq!(config.provider, "openai");
        assert_eq!(config.model, "claude-sonnet-4-20250514");
    }

    #[test]
    fn explicit_anthropic_overrides_openai() {
        let global: ConfigLayer = toml::from_str(r#"
[default]
provider = "openai"
"#).unwrap();
        let project: ConfigLayer = toml::from_str(r#"
[default]
provider = "anthropic"
"#).unwrap();
        let config = RuntimeConfig::from_layers(&[global, project]);
        assert_eq!(config.provider, "anthropic");
    }

    #[test]
    fn parses_minimal_config() {
        let layer: ConfigLayer = toml::from_str(r#"
[default]
provider = "anthropic"
"#).unwrap();
        assert_eq!(layer.default.provider.as_deref(), Some("anthropic"));
        assert!(layer.default.model.is_none());
    }

    #[test]
    fn parses_mcp_and_hooks() {
        let layer: ConfigLayer = toml::from_str(r#"
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
"#).unwrap();
        assert_eq!(layer.mcp_servers.len(), 1);
        assert_eq!(layer.mcp_servers[0].name, "github");
        assert_eq!(layer.mcp_servers[0].args.len(), 2);
        assert_eq!(layer.mcp_servers[0].env.get("GITHUB_TOKEN").unwrap(), "xxx");
        assert_eq!(layer.hooks.len(), 1);
        assert_eq!(layer.hooks[0].event, "pre_tool_call");
        assert_eq!(layer.hooks[0].match_tools, vec!["bash"]);
    }
}
