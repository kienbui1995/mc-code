use std::fs::{self, OpenOptions};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

/// Persists per-turn cost entries to `usage.jsonl` and reads cumulative totals.
/// Persists per-turn usage to disk for cumulative cost tracking.
pub struct CostTracker {
    path: PathBuf,
    cached_in: u64,
    cached_out: u64,
    cached_cost: f64,
}

impl CostTracker {
    #[must_use]
    /// Default path.
    pub fn default_path() -> Option<PathBuf> {
        std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join(".local/share/magic-code/usage.jsonl"))
    }

    #[must_use]
    /// New.
    pub fn new(path: PathBuf) -> Self {
        // Load existing totals on init
        let (cached_in, cached_out, cached_cost) = Self::read_totals(&path);
        Self {
            path,
            cached_in,
            cached_out,
            cached_cost,
        }
    }

    /// Append a usage entry after each turn.
    pub fn record(&mut self, model: &str, input_tokens: u32, output_tokens: u32, cost: f64) {
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let line = serde_json::json!({
            "model": model,
            "in": input_tokens,
            "out": output_tokens,
            "cost": cost,
        });
        let Ok(mut f) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        else {
            return;
        };
        let _ = writeln!(f, "{line}");
        // Update in-memory cache
        self.cached_in += u64::from(input_tokens);
        self.cached_out += u64::from(output_tokens);
        self.cached_cost += cost;
    }

    /// Read all entries and return cumulative (`input_tokens`, `output_tokens`, cost).
    #[must_use]
    /// Cumulative.
    pub fn cumulative(&self) -> (u64, u64, f64) {
        (self.cached_in, self.cached_out, self.cached_cost)
    }

    fn read_totals(path: &Path) -> (u64, u64, f64) {
        let Ok(file) = fs::File::open(path) else {
            return (0, 0, 0.0);
        };
        let mut total_in: u64 = 0;
        let mut total_out: u64 = 0;
        let mut total_cost: f64 = 0.0;
        for line in std::io::BufReader::new(file).lines().map_while(Result::ok) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
                total_in += v["in"].as_u64().unwrap_or(0);
                total_out += v["out"].as_u64().unwrap_or(0);
                total_cost += v["cost"].as_f64().unwrap_or(0.0);
            }
        }
        (total_in, total_out, total_cost)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_read_cumulative() {
        let path = std::env::temp_dir().join(format!("mc-cost-{}.jsonl", std::process::id()));
        let mut tracker = CostTracker::new(path.clone());
        tracker.record("claude", 1000, 200, 0.005);
        tracker.record("claude", 500, 100, 0.002);
        let (i, o, c) = tracker.cumulative();
        assert_eq!(i, 1500);
        assert_eq!(o, 300);
        assert!((c - 0.007).abs() < 1e-9);
        fs::remove_file(path).ok();
    }
}
