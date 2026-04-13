use std::path::PathBuf;

/// Persistent input history with up/down navigation.
pub struct InputHistory {
    entries: Vec<String>,
    cursor: usize, // points past end when not navigating
    max_entries: usize,
    path: Option<PathBuf>,
}

impl InputHistory {
    #[must_use]
    /// New.
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Vec::new(),
            cursor: 0,
            max_entries,
            path: None,
        }
    }

    /// Load history from file. Non-fatal if missing.
    #[must_use]
    /// Load from.
    pub fn load_from(path: PathBuf) -> Self {
        let entries: Vec<String> = std::fs::read_to_string(&path)
            .ok()
            .map(|s| {
                s.lines()
                    .filter(|l| !l.is_empty())
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();
        let cursor = entries.len();
        Self {
            entries,
            cursor,
            max_entries: 1000,
            path: Some(path),
        }
    }

    /// Push.
    pub fn push(&mut self, entry: &str) {
        let trimmed = entry.trim().to_string();
        if trimmed.is_empty() {
            return;
        }
        // Deduplicate consecutive
        if self.entries.last().is_some_and(|last| *last == trimmed) {
            self.cursor = self.entries.len();
            return;
        }
        self.entries.push(trimmed);
        if self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }
        self.cursor = self.entries.len();
        self.save();
    }

    /// Navigate up (older). Returns entry or None if at top.
    pub fn up(&mut self) -> Option<&str> {
        if self.cursor > 0 {
            self.cursor -= 1;
            Some(&self.entries[self.cursor])
        } else {
            None
        }
    }

    /// Navigate down (newer). Returns entry or None if at bottom.
    pub fn down(&mut self) -> Option<&str> {
        if self.cursor < self.entries.len().saturating_sub(1) {
            self.cursor += 1;
            Some(&self.entries[self.cursor])
        } else {
            self.cursor = self.entries.len();
            None
        }
    }

    /// Reset cursor to bottom (stop navigating).
    pub fn reset_cursor(&mut self) {
        self.cursor = self.entries.len();
    }

    #[must_use]
    /// Entries.
    pub fn entries(&self) -> &[String] {
        &self.entries
    }

    /// Search history backwards for a query. Returns matching entry.
    pub fn search(&self, query: &str) -> Option<&str> {
        let q = query.to_lowercase();
        self.entries
            .iter()
            .rev()
            .find(|e| e.to_lowercase().contains(&q))
            .map(|s| s.as_str())
    }

    fn save(&self) {
        if let Some(ref path) = self.path {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let content = self.entries.join("\n");
            let _ = std::fs::write(path, content);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_navigate() {
        let mut h = InputHistory::new(100);
        h.push("first");
        h.push("second");
        h.push("third");

        assert_eq!(h.up(), Some("third"));
        assert_eq!(h.up(), Some("second"));
        assert_eq!(h.up(), Some("first"));
        assert_eq!(h.up(), None);
        assert_eq!(h.down(), Some("second"));
        assert_eq!(h.down(), Some("third"));
        assert_eq!(h.down(), None); // past end
    }

    #[test]
    fn deduplicates_consecutive() {
        let mut h = InputHistory::new(100);
        h.push("same");
        h.push("same");
        assert_eq!(h.entries.len(), 1);
    }

    #[test]
    fn ignores_empty() {
        let mut h = InputHistory::new(100);
        h.push("");
        h.push("  ");
        assert!(h.entries.is_empty());
    }

    #[test]
    fn persistence_roundtrip() {
        let path = std::env::temp_dir().join(format!("mc-hist-{}", std::process::id()));
        {
            let mut h = InputHistory::load_from(path.clone());
            h.push("hello");
            h.push("world");
        }
        let h = InputHistory::load_from(path.clone());
        assert_eq!(h.entries.len(), 2);
        assert_eq!(h.entries[0], "hello");
        std::fs::remove_file(path).ok();
    }
}
