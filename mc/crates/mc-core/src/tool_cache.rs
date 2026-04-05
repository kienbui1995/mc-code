use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::{Duration, Instant};

struct CacheEntry {
    output: String,
    created_at: Instant,
}

pub struct ToolCache {
    entries: HashMap<u64, CacheEntry>,
    cacheable_tools: HashSet<String>,
    ttl: Duration,
    max_entries: usize,
}

impl ToolCache {
    #[must_use]
    pub fn new(ttl_secs: u64, max_entries: usize) -> Self {
        let cacheable = ["glob_search", "grep_search", "read_file"]
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        Self {
            entries: HashMap::new(),
            cacheable_tools: cacheable,
            ttl: Duration::from_secs(ttl_secs),
            max_entries,
        }
    }

    #[must_use]
    pub fn get(&self, tool: &str, input: &serde_json::Value) -> Option<&str> {
        if !self.cacheable_tools.contains(tool) {
            return None;
        }
        let key = Self::hash_key(tool, input);
        let entry = self.entries.get(&key)?;
        if entry.created_at.elapsed() > self.ttl {
            return None;
        }
        Some(&entry.output)
    }

    pub fn put(&mut self, tool: &str, input: &serde_json::Value, output: String) {
        if !self.cacheable_tools.contains(tool) {
            return;
        }
        // Evict expired + enforce max
        self.evict_expired();
        if self.entries.len() >= self.max_entries {
            // Remove oldest entry
            if let Some(&oldest_key) = self
                .entries
                .iter()
                .min_by_key(|(_, v)| v.created_at)
                .map(|(k, _)| k)
            {
                self.entries.remove(&oldest_key);
            }
        }
        let key = Self::hash_key(tool, input);
        self.entries.insert(
            key,
            CacheEntry {
                output,
                created_at: Instant::now(),
            },
        );
    }

    /// Invalidate all cache entries. Called after `write_file`/`edit_file`/`bash`.
    pub fn invalidate_all(&mut self) {
        self.entries.clear();
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn hash_key(tool: &str, input: &serde_json::Value) -> u64 {
        let mut hasher = DefaultHasher::new();
        tool.hash(&mut hasher);
        input.to_string().hash(&mut hasher);
        hasher.finish()
    }

    fn evict_expired(&mut self) {
        let ttl = self.ttl;
        self.entries.retain(|_, v| v.created_at.elapsed() <= ttl);
    }
}

impl Default for ToolCache {
    fn default() -> Self {
        Self::new(30, 64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn cache_hit_miss() {
        let mut cache = ToolCache::new(30, 64);
        let input = json!({"pattern": "*.rs"});
        assert!(cache.get("glob_search", &input).is_none());
        cache.put("glob_search", &input, "file1.rs\nfile2.rs".into());
        assert_eq!(
            cache.get("glob_search", &input).unwrap(),
            "file1.rs\nfile2.rs"
        );
    }

    #[test]
    fn non_cacheable_tool_ignored() {
        let mut cache = ToolCache::new(30, 64);
        let input = json!({"command": "ls"});
        cache.put("bash", &input, "output".into());
        assert!(cache.get("bash", &input).is_none());
        assert!(cache.is_empty());
    }

    #[test]
    fn invalidate_clears_all() {
        let mut cache = ToolCache::new(30, 64);
        cache.put("read_file", &json!({"path": "a.rs"}), "aaa".into());
        cache.put("read_file", &json!({"path": "b.rs"}), "bbb".into());
        assert_eq!(cache.len(), 2);
        cache.invalidate_all();
        assert!(cache.is_empty());
    }

    #[test]
    fn max_entries_eviction() {
        let mut cache = ToolCache::new(30, 2);
        cache.put("read_file", &json!({"path": "a"}), "a".into());
        cache.put("read_file", &json!({"path": "b"}), "b".into());
        cache.put("read_file", &json!({"path": "c"}), "c".into());
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn ttl_expiry() {
        let mut cache = ToolCache::new(0, 64); // 0 second TTL
        let input = json!({"path": "x"});
        cache.put("read_file", &input, "data".into());
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert!(cache.get("read_file", &input).is_none());
    }
}
