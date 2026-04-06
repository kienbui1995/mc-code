use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

const MAX_LOG_SIZE: u64 = 10 * 1024 * 1024; // 10MB

/// Append-only audit log for tool executions.
pub struct AuditLog {
    path: PathBuf,
}

/// Auditentry.
pub struct AuditEntry {
    pub tool: String,
    pub input_summary: String,
    pub output_len: usize,
    pub is_error: bool,
    pub duration_ms: u64,
    pub allowed: bool,
}

impl AuditLog {
    #[must_use]
    /// New.
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Default location: `~/.local/share/magic-code/audit.jsonl`
    #[must_use]
    /// Default path.
    pub fn default_path() -> Option<PathBuf> {
        std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join(".local/share/magic-code/audit.jsonl"))
    }

    /// Log.
    pub fn log(&self, entry: &AuditEntry) {
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        // Rotate if too large
        if let Ok(meta) = fs::metadata(&self.path) {
            if meta.len() > MAX_LOG_SIZE {
                let _ = fs::remove_file(self.path.with_extension("jsonl.2"));
                let _ = fs::rename(
                    self.path.with_extension("jsonl.1"),
                    self.path.with_extension("jsonl.2"),
                );
                let _ = fs::rename(&self.path, self.path.with_extension("jsonl.1"));
            }
        }
        let line = serde_json::json!({
            "tool": entry.tool,
            "input": truncate(&entry.input_summary, 200),
            "output_len": entry.output_len,
            "error": entry.is_error,
            "ms": entry.duration_ms,
            "allowed": entry.allowed,
        });
        let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        else {
            return;
        };
        let _ = writeln!(file, "{line}");
    }

    /// Start a timer, returns an `Instant` for measuring duration.
    #[must_use]
    /// Start timer.
    pub fn start_timer() -> Instant {
        Instant::now()
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_audit_entry() {
        let path = std::env::temp_dir().join(format!("mc-audit-{}.jsonl", std::process::id()));
        let log = AuditLog::new(path.clone());
        log.log(&AuditEntry {
            tool: "bash".into(),
            input_summary: "ls -la".into(),
            output_len: 100,
            is_error: false,
            duration_ms: 12,
            allowed: true,
        });
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("\"tool\":\"bash\""));
        assert!(content.contains("\"ms\":12"));
        fs::remove_file(path).ok();
    }
}
