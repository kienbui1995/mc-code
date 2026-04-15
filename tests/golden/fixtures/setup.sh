#!/bin/bash
# Golden test fixture: todo-api project
# Usage: ./setup.sh /path/to/workspace
set -e
WS="${1:?Usage: setup.sh <workspace_dir>}"
rm -rf "$WS" && mkdir -p "$WS"/{src,.magic-code,tests}

cat > "$WS/Cargo.toml" << 'EOF'
[package]
name = "todo-api"
version = "0.2.0"
edition = "2021"
EOF

cat > "$WS/CLAUDE.md" << 'EOF'
# todo-api
## Conventions
- No external dependencies (stdlib only)
- All public functions must have doc comments
- Use Result<T, TodoError> for fallible operations
- Tests go in tests/ directory, not inline
- Variable names in snake_case, types in PascalCase
## Architecture: model.rs (data) → store.rs (logic) → main.rs (CLI)
EOF

cat > "$WS/.magic-code/instructions.md" << 'EOF'
- Always run cargo check after editing Rust files
- Prefer edit_file over write_file for existing files
- Read a file before editing it
- When adding a new function, also add a test for it
EOF

cat > "$WS/src/model.rs" << 'EOF'
use std::sync::atomic::{AtomicU64, Ordering};
use std::fmt;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority { Low, Medium, High }

impl fmt::Display for Priority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Low => write!(f, "low"),
            Self::Medium => write!(f, "medium"),
            Self::High => write!(f, "high"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Todo {
    pub id: u64,
    pub title: String,
    pub completed: bool,
    pub priority: Priority,
    pub tags: Vec<String>,
}

impl Todo {
    pub fn new(title: String, priority: Priority) -> Self {
        Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            title, completed: false, priority, tags: Vec::new(),
        }
    }
    pub fn toggle(&mut self) { self.completed = !self.completed; }
    pub fn add_tag(&mut self, tag: String) {
        if !self.tags.contains(&tag) { self.tags.push(tag); }
    }
}

impl fmt::Display for Todo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = if self.completed { "✓" } else { "○" };
        write!(f, "[{}] {} ({}) #{}", s, self.title, self.priority, self.id)
    }
}
EOF

cat > "$WS/src/store.rs" << 'EOF'
use crate::model::{Todo, Priority};
use std::collections::HashMap;

pub struct TodoStore {
    todos: HashMap<u64, Todo>,
}

impl TodoStore {
    pub fn new() -> Self { Self { todos: HashMap::new() } }

    pub fn add(&mut self, title: String, priority: Priority) -> u64 {
        let todo = Todo::new(title, priority);
        let id = todo.id;
        self.todos.insert(id, todo);
        id
    }

    pub fn get(&self, id: u64) -> Option<&Todo> { self.todos.get(&id) }
    pub fn get_mut(&mut self, id: u64) -> Option<&mut Todo> { self.todos.get_mut(&id) }

    pub fn list(&self) -> Vec<&Todo> {
        let mut v: Vec<_> = self.todos.values().collect();
        v.sort_by_key(|t| t.id);
        v
    }

    pub fn delete(&mut self, id: u64) -> bool { self.todos.remove(&id).is_some() }

    pub fn search(&self, query: &str) -> Vec<&Todo> {
        let q = query.to_lowercase();
        self.todos.values().filter(|t| t.title.to_lowercase().contains(&q)).collect()
    }

    pub fn by_priority(&self, p: Priority) -> Vec<&Todo> {
        self.todos.values().filter(|t| t.priority == p).collect()
    }

    pub fn stats(&self) -> (usize, usize) {
        let total = self.todos.len();
        let done = self.todos.values().filter(|t| t.completed).count();
        (total, done)
    }
}
EOF

cat > "$WS/src/main.rs" << 'EOF'
mod model;
mod store;

use model::Priority;
use store::TodoStore;

fn main() {
    let mut store = TodoStore::new();
    store.add("Buy groceries".to_string(), Priority::High);
    store.add("Write docs".to_string(), Priority::Medium);
    store.add("Clean desk".to_string(), Priority::Low);

    println!("=== Todos ===");
    for t in store.list() { println!("  {t}"); }

    if let Some(t) = store.get_mut(1) { t.toggle(); }
    let (total, done) = store.stats();
    println!("\n{done}/{total} completed");
}
EOF

cat > "$WS/tests/store_test.rs" << 'EOF'
use todo_api::model::Priority;
use todo_api::store::TodoStore;

#[test]
fn test_add_and_get() {
    let mut s = TodoStore::new();
    let id = s.add("Test".into(), Priority::Medium);
    assert!(s.get(id).is_some());
}

#[test]
fn test_delete() {
    let mut s = TodoStore::new();
    let id = s.add("Test".into(), Priority::Low);
    assert!(s.delete(id));
    assert!(!s.delete(id));
}

#[test]
fn test_search() {
    let mut s = TodoStore::new();
    s.add("Buy milk".into(), Priority::High);
    s.add("Buy eggs".into(), Priority::Medium);
    s.add("Clean".into(), Priority::Low);
    assert_eq!(s.search("buy").len(), 2);
}
EOF

echo "Fixture ready: $(find "$WS" -name '*.rs' -o -name '*.toml' -o -name '*.md' | wc -l) files"
