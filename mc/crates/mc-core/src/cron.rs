use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct CronTrigger {
    pub name: String,
    pub schedule_secs: u64,
    pub prompt: String,
    pub enabled: bool,
    pub last_run: Option<std::time::Instant>,
}

/// Simple interval-based scheduler (not full cron expressions).
/// Triggers fire prompts at fixed intervals.
pub struct CronManager {
    triggers: HashMap<String, CronTrigger>,
}

impl CronManager {
    #[must_use]
    pub fn new() -> Self {
        Self {
            triggers: HashMap::new(),
        }
    }

    /// Add a trigger. schedule_secs is the interval in seconds.
    pub fn add(&mut self, name: &str, schedule_secs: u64, prompt: &str) {
        self.triggers.insert(
            name.to_string(),
            CronTrigger {
                name: name.to_string(),
                schedule_secs,
                prompt: prompt.to_string(),
                enabled: true,
                last_run: None,
            },
        );
    }

    /// Remove a trigger by name.
    pub fn remove(&mut self, name: &str) -> bool {
        self.triggers.remove(name).is_some()
    }

    /// List all triggers.
    #[must_use]
    pub fn list(&self) -> Vec<&CronTrigger> {
        self.triggers.values().collect()
    }

    /// Check which triggers are due to fire. Returns prompts to execute.
    pub fn tick(&mut self) -> Vec<String> {
        let now = std::time::Instant::now();
        let mut due = Vec::new();
        for trigger in self.triggers.values_mut() {
            if !trigger.enabled {
                continue;
            }
            let should_fire = match trigger.last_run {
                None => true,
                Some(last) => now.duration_since(last).as_secs() >= trigger.schedule_secs.max(1),
            };
            if should_fire {
                trigger.last_run = Some(now);
                due.push(trigger.prompt.clone());
            }
        }
        due
    }

    /// Enable/disable a trigger.
    pub fn set_enabled(&mut self, name: &str, enabled: bool) -> bool {
        if let Some(t) = self.triggers.get_mut(name) {
            t.enabled = enabled;
            true
        } else {
            false
        }
    }
}

impl Default for CronManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_tick() {
        let mut mgr = CronManager::new();
        mgr.add("test", 0, "do something"); // 0 = fire immediately
        let due = mgr.tick();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0], "do something");
        // Second tick should not fire (just ran)
        let due2 = mgr.tick();
        assert!(due2.is_empty());
    }

    #[test]
    fn remove_trigger() {
        let mut mgr = CronManager::new();
        mgr.add("t1", 60, "prompt");
        assert_eq!(mgr.list().len(), 1);
        assert!(mgr.remove("t1"));
        assert!(mgr.list().is_empty());
    }

    #[test]
    fn disable_trigger() {
        let mut mgr = CronManager::new();
        mgr.add("t1", 0, "prompt");
        mgr.set_enabled("t1", false);
        let due = mgr.tick();
        assert!(due.is_empty());
    }
}

    #[test]
    fn list_triggers() {
        let mut mgr = CronManager::new();
        mgr.add("every5", 300, "check status");
        mgr.add("hourly", 3600, "report");
        let list = mgr.list();
        assert_eq!(list.len(), 2);
    }
