mod audit;
mod bash;
mod error;
mod file_ops;
mod hooks;
mod lsp;
mod mcp;
mod permissions;
mod plugin;
mod registry;
mod sandbox;
mod search;
mod spec;
mod web;

pub use audit::{AuditEntry, AuditLog};
pub use bash::BashTool;
pub use error::ToolError;
pub use file_ops::{EditFileTool, ReadFileTool, WriteFileTool};
pub use hooks::{Hook, HookEngine, HookEvent};
pub use lsp::{detect_language, LspClient};
pub use mcp::McpClient;
pub use permissions::{
    PermissionMode, PermissionOutcome, PermissionPolicy, PermissionPrompter, PermissionRequest,
};
pub use plugin::{discover_plugins, execute_plugin};
pub use registry::ToolRegistry;
pub use sandbox::Sandbox;
pub use search::{GlobSearchTool, GrepSearchTool};
pub use spec::ToolSpec;
pub use web::{WebFetchTool, WebSearchTool};
