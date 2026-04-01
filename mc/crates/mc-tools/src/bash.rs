use std::time::Duration;

use tokio::process::Command;

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
}
