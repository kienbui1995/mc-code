use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};

use crate::error::ToolError;

/// Lightweight LSP client for code intelligence queries.
pub struct LspClient {
    child: Child,
    stdin: tokio::process::ChildStdin,
    stdout: BufReader<tokio::process::ChildStdout>,
    next_id: u64,
}

impl LspClient {
    /// Start an LSP server for the given language.
    pub async fn start(language: &str, root: &Path) -> Result<Self, ToolError> {
        let (cmd, args): (&str, &[&str]) = match language {
            "rust" => ("rust-analyzer", &[]),
            "typescript" | "javascript" => ("typescript-language-server", &["--stdio"]),
            "python" => ("pyright-langserver", &["--stdio"]),
            "go" => ("gopls", &["serve"]),
            _ => return Err(ToolError::ExecutionFailed(format!("no LSP for {language}"))),
        };

        let mut child = Command::new(cmd)
            .args(args)
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| ToolError::ExecutionFailed(format!("LSP start failed: {e}")))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| ToolError::ExecutionFailed("failed to open LSP stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ToolError::ExecutionFailed("failed to open LSP stdout".into()))?;

        let mut client = Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 1,
        };

        // Initialize
        let root_uri = format!("file://{}", root.display());
        let init = serde_json::json!({
            "jsonrpc": "2.0",
            "id": client.next_id(),
            "method": "initialize",
            "params": {
                "rootUri": root_uri,
                "capabilities": {}
            }
        });
        client.send(&init).await?;
        let _ = client.recv().await;

        Ok(client)
    }

    /// Query: go-to-definition, find-references, or hover.
    pub async fn query(
        &mut self,
        method: &str,
        file: &str,
        line: u32,
        col: u32,
    ) -> Result<String, ToolError> {
        let uri = if file.starts_with("file://") {
            file.to_string()
        } else {
            format!(
                "file://{}",
                std::path::Path::new(file)
                    .canonicalize()
                    .unwrap_or_else(|_| file.into())
                    .display()
            )
        };

        let params = serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": col }
        });

        let lsp_method = match method {
            "definition" | "goto" => "textDocument/definition",
            "references" | "refs" => "textDocument/references",
            "hover" => "textDocument/hover",
            _ => return Err(ToolError::InvalidInput(format!("unknown method: {method}"))),
        };

        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": self.next_id(),
            "method": lsp_method,
            "params": params
        });

        self.send(&req).await?;
        let resp = tokio::time::timeout(Duration::from_secs(10), self.recv())
            .await
            .map_err(|_| ToolError::Timeout(10_000))??;

        if let Some(result) = resp.get("result") {
            Ok(serde_json::to_string_pretty(result).unwrap_or_default())
        } else if let Some(err) = resp.get("error") {
            Err(ToolError::ExecutionFailed(err.to_string()))
        } else {
            Ok("(no result)".into())
        }
    }

    /// Shutdown.
    pub async fn shutdown(&mut self) {
        let _ = self.child.kill().await;
    }

    async fn send(&mut self, msg: &serde_json::Value) -> Result<(), ToolError> {
        let body =
            serde_json::to_string(msg).map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        self.stdin
            .write_all(header.as_bytes())
            .await
            .map_err(ToolError::Io)?;
        self.stdin
            .write_all(body.as_bytes())
            .await
            .map_err(ToolError::Io)?;
        self.stdin.flush().await.map_err(ToolError::Io)
    }

    async fn recv(&mut self) -> Result<serde_json::Value, ToolError> {
        // Read Content-Length header
        let mut header = String::new();
        loop {
            header.clear();
            self.stdout
                .read_line(&mut header)
                .await
                .map_err(ToolError::Io)?;
            if header.trim().is_empty() {
                break;
            }
        }
        // Read body (simplified — read until valid JSON)
        let mut line = String::new();
        loop {
            line.clear();
            let n = self
                .stdout
                .read_line(&mut line)
                .await
                .map_err(ToolError::Io)?;
            if n == 0 {
                return Err(ToolError::ExecutionFailed("LSP closed".into()));
            }
            if let Ok(v) = serde_json::from_str(line.trim()) {
                return Ok(v);
            }
        }
    }

    fn next_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

/// Detect language from file extension.
#[must_use]
/// Detect language.
pub fn detect_language(file: &str) -> Option<&'static str> {
    match file.rsplit('.').next()? {
        "rs" => Some("rust"),
        "ts" | "tsx" | "js" | "jsx" => Some("typescript"),
        "py" => Some("python"),
        "go" => Some("go"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_languages() {
        assert_eq!(detect_language("main.rs"), Some("rust"));
        assert_eq!(detect_language("app.tsx"), Some("typescript"));
        assert_eq!(detect_language("test.py"), Some("python"));
        assert_eq!(detect_language("main.go"), Some("go"));
        assert_eq!(detect_language("data.csv"), None);
    }
}
