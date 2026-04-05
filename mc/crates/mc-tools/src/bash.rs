use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

use crate::error::ToolError;

pub struct BashTool;

impl BashTool {
    pub async fn execute(command: &str, timeout: Option<Duration>) -> Result<String, ToolError> {
        let timeout = timeout.unwrap_or(Duration::from_secs(120));

        let result = tokio::time::timeout(timeout, async {
            let output = Command::new("sh")
                .arg("-c")
                .arg(command)
                .output()
                .await
                .map_err(ToolError::Io)?;
            Ok::<_, ToolError>(output)
        })
        .await;

        let output = match result {
            Ok(inner) => inner?,
            Err(_) => return Err(ToolError::Timeout(timeout.as_millis() as u64)),
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let mut result = String::new();
        if !stdout.is_empty() {
            result.push_str(&stdout);
        }
        if !stderr.is_empty() {
            if !result.is_empty() {
                result.push('\n');
            }
            result.push_str("STDERR: ");
            result.push_str(&stderr);
        }
        Ok(result)
    }

    /// Execute a command and stream stdout/stderr lines through the sender as they arrive.
    /// Returns the full collected output when the process completes.
    pub async fn execute_streaming(
        command: &str,
        timeout: Option<Duration>,
        output_tx: &mpsc::UnboundedSender<String>,
    ) -> Result<String, ToolError> {
        let timeout = timeout.unwrap_or(Duration::from_secs(120));

        let result = tokio::time::timeout(timeout, async {
            let mut child = Command::new("sh")
                .arg("-c")
                .arg(command)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(ToolError::Io)?;

            let stdout = child.stdout.take().expect("stdout piped");
            let stderr = child.stderr.take().expect("stderr piped");

            let mut stdout_reader = BufReader::new(stdout).lines();
            let mut stderr_reader = BufReader::new(stderr).lines();
            let mut collected = String::new();
            let mut stderr_buf = String::new();
            let mut stderr_done = false;

            loop {
                tokio::select! {
                    line = stdout_reader.next_line() => {
                        if let Some(l) = line.map_err(ToolError::Io)? {
                            let _ = output_tx.send(format!("{l}\n"));
                            collected.push_str(&l);
                            collected.push('\n');
                        } else {
                            // stdout closed, drain remaining stderr then wait
                            if !stderr_done {
                                while let Ok(Some(l)) = stderr_reader.next_line().await {
                                    let line = format!("STDERR: {l}\n");
                                    let _ = output_tx.send(line.clone());
                                    stderr_buf.push_str(&line);
                                }
                            }
                            break;
                        }
                    }
                    line = stderr_reader.next_line(), if !stderr_done => {
                        match line {
                            Ok(Some(l)) => {
                                let line = format!("STDERR: {l}\n");
                                let _ = output_tx.send(line.clone());
                                stderr_buf.push_str(&line);
                            }
                            _ => { stderr_done = true; }
                        }
                    }
                }
            }

            let _ = child.wait().await;

            if !stderr_buf.is_empty() {
                if !collected.is_empty() && !collected.ends_with('\n') {
                    collected.push('\n');
                }
                collected.push_str(&stderr_buf);
            }

            // Trim trailing newline for consistency with non-streaming execute
            if collected.ends_with('\n') {
                collected.pop();
            }

            Ok::<_, ToolError>(collected)
        })
        .await;

        match result {
            Ok(inner) => inner,
            Err(_) => Err(ToolError::Timeout(timeout.as_millis() as u64)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn runs_simple_command() {
        let out = BashTool::execute("printf hello", None).await.unwrap();
        assert_eq!(out, "hello");
    }

    #[tokio::test]
    async fn captures_stderr() {
        let out = BashTool::execute("echo err >&2", None).await.unwrap();
        assert!(out.contains("STDERR:"));
        assert!(out.contains("err"));
    }

    #[tokio::test]
    async fn timeout_works() {
        let err = BashTool::execute("sleep 10", Some(Duration::from_millis(50)))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::Timeout(_)));
    }

    #[tokio::test]
    async fn streaming_captures_lines() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let out = BashTool::execute_streaming("echo line1; echo line2", None, &tx)
            .await
            .unwrap();
        drop(tx);
        assert!(out.contains("line1"));
        assert!(out.contains("line2"));
        let mut chunks = Vec::new();
        while let Some(c) = rx.recv().await {
            chunks.push(c);
        }
        assert!(chunks.len() >= 2);
    }

    #[tokio::test]
    async fn streaming_captures_stderr() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let out = BashTool::execute_streaming("echo err >&2", None, &tx)
            .await
            .unwrap();
        drop(tx);
        assert!(out.contains("STDERR:"));
        let mut chunks = Vec::new();
        while let Some(c) = rx.recv().await {
            chunks.push(c);
        }
        assert!(chunks.iter().any(|c| c.contains("STDERR:")));
    }

    #[tokio::test]
    async fn streaming_timeout_works() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let err = BashTool::execute_streaming("sleep 10", Some(Duration::from_millis(50)), &tx)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::Timeout(_)));
    }

    #[tokio::test]
    async fn streaming_mixed_stdout_stderr() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let out = BashTool::execute_streaming(
            "echo out1; echo err1 >&2; echo out2",
            None,
            &tx,
        )
        .await
        .unwrap();
        drop(tx);
        assert!(out.contains("out1"));
        assert!(out.contains("out2"));
        assert!(out.contains("STDERR: err1"));
        let mut chunks = Vec::new();
        while let Some(c) = rx.recv().await {
            chunks.push(c);
        }
        assert!(chunks.len() >= 3);
    }
}
