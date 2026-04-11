use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Fact.
pub struct Fact {
    pub key: String,
    pub value: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct MemoryFile {
    facts: Vec<Fact>,
}

/// Memorystore.
pub struct MemoryStore {
    facts: Vec<Fact>,
    path: PathBuf,
    max_facts: usize,
}

impl MemoryStore {
    #[must_use]
    /// Load.
    pub fn load(path: &Path, max_facts: usize) -> Self {
        let facts = fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str::<MemoryFile>(&s).ok())
            .map_or_else(Vec::new, |f| f.facts);
        Self {
            facts,
            path: path.to_path_buf(),
            max_facts,
        }
    }

    /// Save.
    pub fn save(&self) -> Result<(), std::io::Error> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = MemoryFile {
            facts: self.facts.clone(),
        };
        let json = serde_json::to_string_pretty(&file)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        fs::write(&self.path, json)
    }

    #[must_use]
    /// Get.
    pub fn get(&self, key: &str) -> Option<&Fact> {
        self.facts.iter().find(|f| f.key == key)
    }

    /// Set.
    pub fn set(&mut self, key: &str, value: &str) {
        let ts = epoch_secs();
        if let Some(f) = self.facts.iter_mut().find(|f| f.key == key) {
            f.value = value.to_string();
            f.updated_at = ts;
        } else {
            self.facts.push(Fact {
                key: key.into(),
                value: value.into(),
                updated_at: ts,
            });
            self.evict();
        }
    }

    /// Delete.
    pub fn delete(&mut self, key: &str) -> bool {
        let len = self.facts.len();
        self.facts.retain(|f| f.key != key);
        self.facts.len() < len
    }

    #[must_use]
    /// All.
    pub fn all(&self) -> &[Fact] {
        &self.facts
    }

    #[must_use]
    /// Len.
    pub fn len(&self) -> usize {
        self.facts.len()
    }

    #[must_use]
    /// Is empty.
    pub fn is_empty(&self) -> bool {
        self.facts.is_empty()
    }

    /// Format facts for injection into system prompt. Returns "" if empty.
    #[must_use]
    /// To prompt section.
    pub fn to_prompt_section(&self) -> String {
        if self.facts.is_empty() {
            return String::new();
        }
        let lines: Vec<String> = self
            .facts
            .iter()
            .map(|f| format!("- {}: {}", f.key, f.value))
            .collect();
        format!("\n\n## Project Memory\n{}", lines.join("\n"))
    }

    /// Handle `memory_read` tool call. Returns JSON output.
    #[must_use]
    /// Handle read.
    pub fn handle_read(&self, input: &serde_json::Value) -> String {
        if let Some(key) = input.get("key").and_then(|v| v.as_str()) {
            match self.get(key) {
                Some(f) => serde_json::json!({"key": f.key, "value": f.value}).to_string(),
                None => format!("No fact found for key: {key}"),
            }
        } else {
            serde_json::to_string(&self.facts).unwrap_or_else(|_| "[]".into())
        }
    }

    /// Handle `memory_write` tool call. Returns confirmation.
    pub fn handle_write(&mut self, input: &serde_json::Value) -> String {
        let key = input.get("key").and_then(|v| v.as_str()).unwrap_or("");
        let value = input.get("value").and_then(|v| v.as_str()).unwrap_or("");
        if key.is_empty() {
            return "Error: key is required".into();
        }
        if let Some(delete) = input.get("delete").and_then(serde_json::Value::as_bool) {
            if delete {
                return if self.delete(key) {
                    format!("Deleted: {key}")
                } else {
                    format!("Key not found: {key}")
                };
            }
        }
        self.set(key, value);
        format!("Saved: {key} = {value}")
    }

    fn evict(&mut self) {
        while self.facts.len() > self.max_facts {
            // Remove oldest by updated_at
            if let Some(idx) = self
                .facts
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| a.updated_at.cmp(&b.updated_at))
                .map(|(i, _)| i)
            {
                self.facts.remove(idx);
            }
        }
    }
}

fn epoch_secs() -> String {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_path() -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("mc-mem-{}-{n}.json", std::process::id()))
    }

    #[test]
    fn set_get_delete() {
        let mut store = MemoryStore::load(&tmp_path(), 50);
        store.set("lang", "rust");
        assert_eq!(store.get("lang").unwrap().value, "rust");
        store.set("lang", "python");
        assert_eq!(store.get("lang").unwrap().value, "python");
        assert!(store.delete("lang"));
        assert!(store.get("lang").is_none());
        assert!(!store.delete("nonexistent"));
    }

    #[test]
    fn save_load_roundtrip() {
        let path = tmp_path();
        let mut store = MemoryStore::load(&path, 50);
        store.set("framework", "actix");
        store.save().unwrap();
        let loaded = MemoryStore::load(&path, 50);
        assert_eq!(loaded.get("framework").unwrap().value, "actix");
        fs::remove_file(&path).ok();
    }

    #[test]
    fn eviction() {
        let mut store = MemoryStore::load(&tmp_path(), 3);
        store.set("a", "1");
        store.set("b", "2");
        store.set("c", "3");
        store.set("d", "4");
        assert_eq!(store.len(), 3);
        // oldest evicted
        assert!(store.get("a").is_none());
    }

    #[test]
    fn to_prompt_section_empty() {
        let store = MemoryStore::load(&tmp_path(), 50);
        assert_eq!(store.to_prompt_section(), "");
    }

    #[test]
    fn to_prompt_section_with_facts() {
        let mut store = MemoryStore::load(&tmp_path(), 50);
        store.set("test_runner", "pytest");
        let section = store.to_prompt_section();
        assert!(section.contains("## Project Memory"));
        assert!(section.contains("test_runner: pytest"));
    }

    #[test]
    fn handle_read_write() {
        let mut store = MemoryStore::load(&tmp_path(), 50);
        let out = store.handle_write(&serde_json::json!({"key": "db", "value": "postgres"}));
        assert!(out.contains("Saved"));
        let out = store.handle_read(&serde_json::json!({"key": "db"}));
        assert!(out.contains("postgres"));
        let out = store.handle_read(&serde_json::json!({}));
        assert!(out.contains("db"));
    }
}

    #[test]
    fn handle_write_delete() {
        let path = std::env::temp_dir().join(format!("mc-mem-del-{}", std::process::id()));
        let mut store = MemoryStore::load(&path, 100);
        store.handle_write(&serde_json::json!({"key": "temp", "value": "data"}));
        assert!(store.handle_read(&serde_json::json!({"key": "temp"})).contains("data"));
        store.handle_write(&serde_json::json!({"key": "temp", "delete": true}));
        assert!(!store.handle_read(&serde_json::json!({"key": "temp"})).contains("data"));
        std::fs::remove_file(path).ok();
    }
