use std::sync::atomic::{AtomicU64, Ordering};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};

use crate::error::ToolError;
use crate::spec::ToolSpec;

/// Async MCP client using stdio transport with persistent connection.
pub struct McpClient {
    child: Child,
    stdin: tokio::process::ChildStdin,
    stdout: BufReader<tokio::process::ChildStdout>,
    next_id: AtomicU64,
    pub server_name: String,
    command: String,
    args: Vec<String>,
    env: Vec<(String, String)>,
}

impl McpClient {
    /// Connect to an MCP server, perform initialize handshake.
    pub async fn connect(
        name: &str,
        command: &str,
        args: &[String],
        env: &[(String, String)],
    ) -> Result<Self, ToolError> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());
        for (k, v) in env {
            cmd.env(k, v);
        }

        let mut child = cmd.spawn().map_err(ToolError::Io)?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| ToolError::ExecutionFailed("failed to open MCP stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ToolError::ExecutionFailed("failed to open MCP stdout".into()))?;

        let mut client = Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: AtomicU64::new(1),
            server_name: name.to_string(),
            command: command.to_string(),
            args: args.to_vec(),
            env: env.to_vec(),
        };

        // Initialize handshake
        let init = serde_json::json!({
            "jsonrpc": "2.0",
            "id": client.next_id(),
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "magic-code", "version": "0.1.0"}
            }
        });
        client.send(&init).await?;
        // Read initialize response (discard)
        client.recv().await?;

        Ok(client)
    }

    /// Discover tools from the MCP server.
    pub async fn discover_tools(&mut self) -> Result<Vec<ToolSpec>, ToolError> {
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": self.next_id(),
            "method": "tools/list",
            "params": {}
        });
        self.send(&req).await?;
        let resp = self.recv().await?;

        let mut tools = Vec::new();
        if let Some(result) = resp.get("result") {
            if let Some(tool_list) = result.get("tools").and_then(|t| t.as_array()) {
                for tool in tool_list {
                    let name = tool
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("unknown");
                    let desc = tool
                        .get("description")
                        .and_then(|d| d.as_str())
                        .unwrap_or("");
                    let schema = tool
                        .get("inputSchema")
                        .cloned()
                        .unwrap_or(serde_json::json!({"type": "object"}));
                    tools.push(ToolSpec {
                        name: format!("mcp_{}_{}", self.server_name, name),
                        description: format!("[MCP:{}] {}", self.server_name, desc),
                        input_schema: schema,
                    });
                }
            }
        }
        Ok(tools)
    }

    /// Reconnect to the MCP server by re-spawning the process and re-doing the handshake.
    pub async fn reconnect(&mut self) -> Result<(), ToolError> {
        // Best-effort kill old process
        let _ = self.child.kill().await;

        let mut cmd = Command::new(&self.command);
        cmd.args(&self.args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());
        for (k, v) in &self.env {
            cmd.env(k, v);
        }

        let mut child = cmd.spawn().map_err(ToolError::Io)?;
        self.stdin = child
            .stdin
            .take()
            .ok_or_else(|| ToolError::ExecutionFailed("failed to open MCP stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ToolError::ExecutionFailed("failed to open MCP stdout".into()))?;
        self.stdout = BufReader::new(stdout);
        self.child = child;
        self.next_id = AtomicU64::new(1);

        // Re-do initialize handshake
        let init = serde_json::json!({
            "jsonrpc": "2.0",
            "id": self.next_id(),
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "magic-code", "version": "0.1.0"}
            }
        });
        self.send(&init).await?;
        self.recv().await?;

        Ok(())
    }

    /// Call a tool on the MCP server (with timeout).
    pub async fn call_tool(
        &mut self,
        name: &str,
        arguments: &serde_json::Value,
    ) -> Result<String, ToolError> {
        // Auto-reconnect if the server process has died
        if !self.is_alive() {
            self.reconnect().await?;
        }

        let id = self.next_id();
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {"name": name, "arguments": arguments}
        });
        self.send(&req).await?;

        let resp = tokio::time::timeout(std::time::Duration::from_secs(60), self.recv())
            .await
            .map_err(|_| ToolError::Timeout(60_000))??;
        if let Some(result) = resp.get("result") {
            Ok(result.to_string())
        } else if let Some(error) = resp.get("error") {
            Err(ToolError::ExecutionFailed(error.to_string()))
        } else {
            Err(ToolError::ExecutionFailed("empty MCP response".into()))
        }
    }

    /// Check if the MCP server process is still alive.
    #[must_use]
    /// Is alive.
    pub fn is_alive(&mut self) -> bool {
        self.child.try_wait().ok().flatten().is_none()
    }

    /// Gracefully shut down the MCP server.
    pub async fn disconnect(&mut self) {
        let _ = self.child.kill().await;
    }

    async fn send(&mut self, msg: &serde_json::Value) -> Result<(), ToolError> {
        let line = format!("{msg}\n");
        self.stdin
            .write_all(line.as_bytes())
            .await
            .map_err(ToolError::Io)?;
        self.stdin.flush().await.map_err(ToolError::Io)
    }

    async fn recv(&mut self) -> Result<serde_json::Value, ToolError> {
        self.recv_with_timeout(std::time::Duration::from_secs(30)).await
    }

    async fn recv_with_timeout(&mut self, timeout: std::time::Duration) -> Result<serde_json::Value, ToolError> {
        let mut line = String::new();
        // Read lines until we get valid JSON (skip empty lines)
        loop {
            line.clear();
            let n = tokio::time::timeout(timeout, self.stdout.read_line(&mut line))
                .await
                .map_err(|_| ToolError::Timeout(timeout.as_millis() as u64))?
                .map_err(ToolError::Io)?;
            if n == 0 {
                return Err(ToolError::ExecutionFailed(
                    "MCP server closed stdout".into(),
                ));
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Ok(v) = serde_json::from_str(trimmed) {
                return Ok(v);
            }
        }
    }

    fn next_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        // Best-effort kill on drop
        let _ = self.child.start_kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn connect_fails_for_bad_command() {
        let result = McpClient::connect("test", "nonexistent_binary_xyz", &[], &[]).await;
        assert!(result.is_err());
    }
}
