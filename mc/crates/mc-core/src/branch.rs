use std::fs;
use std::path::PathBuf;

use crate::session::Session;

pub struct BranchInfo {
    pub id: String,
    pub parent: Option<String>,
    pub message_count: usize,
    pub fork_point: Option<usize>,
}

pub struct BranchManager {
    branches_dir: PathBuf,
    max_branches: usize,
}

impl BranchManager {
    #[must_use]
    pub fn new(branches_dir: PathBuf, max_branches: usize) -> Self {
        Self {
            branches_dir,
            max_branches,
        }
    }

    /// Fork a session at the given message index. Returns new session with branch metadata.
    #[must_use]
    pub fn fork(&self, session: &Session, at_index: usize) -> Session {
        let branch_id = format!("fork-{}", self.next_id());
        let parent = session.branch_id.clone().unwrap_or_else(|| "main".into());
        let end = at_index.min(session.messages.len());
        let mut forked = Session {
            messages: session.messages[..end].to_vec(),
            input_tokens: session.input_tokens,
            output_tokens: session.output_tokens,
            branch_id: Some(branch_id),
            parent_branch: Some(parent),
            fork_point: Some(end),
        };
        // Recalculate tokens for forked subset (approximate)
        forked.input_tokens = 0;
        forked.output_tokens = 0;
        forked
    }

    pub fn save_branch(&self, session: &Session) -> Result<(), std::io::Error> {
        let id = session.branch_id.as_deref().unwrap_or("main");
        fs::create_dir_all(&self.branches_dir)?;
        let path = self.branches_dir.join(format!("{id}.json"));
        session.save(&path)
    }

    pub fn load_branch(&self, id: &str) -> Result<Session, std::io::Error> {
        let path = self.branches_dir.join(format!("{id}.json"));
        Session::load(&path)
    }

    #[must_use]
    pub fn list_branches(&self) -> Vec<BranchInfo> {
        let mut branches = Vec::new();
        let Ok(entries) = fs::read_dir(&self.branches_dir) else {
            return branches;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                if let Ok(session) = Session::load(&path) {
                    let id = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    branches.push(BranchInfo {
                        id,
                        parent: session.parent_branch,
                        message_count: session.messages.len(),
                        fork_point: session.fork_point,
                    });
                }
            }
        }
        branches
    }

    pub fn delete_branch(&self, id: &str) -> Result<(), std::io::Error> {
        let path = self.branches_dir.join(format!("{id}.json"));
        fs::remove_file(path)
    }

    #[must_use]
    pub fn at_capacity(&self) -> bool {
        self.list_branches().len() >= self.max_branches
    }

    fn next_id(&self) -> usize {
        self.list_branches().len() + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::ConversationMessage;

    fn tmp_dir() -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static C: AtomicU32 = AtomicU32::new(0);
        let d = std::env::temp_dir().join(format!(
            "mc-branch-{}-{}",
            std::process::id(),
            C.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn fork_creates_subset() {
        let mut session = Session::default();
        for i in 0..6 {
            session
                .messages
                .push(ConversationMessage::user(format!("msg {i}")));
        }
        let mgr = BranchManager::new(tmp_dir(), 5);
        let forked = mgr.fork(&session, 3);
        assert_eq!(forked.messages.len(), 3);
        assert!(forked.branch_id.as_ref().unwrap().starts_with("fork-"));
        assert_eq!(forked.parent_branch.as_deref(), Some("main"));
        assert_eq!(forked.fork_point, Some(3));
    }

    #[test]
    fn save_load_roundtrip() {
        let dir = tmp_dir();
        let mgr = BranchManager::new(dir.clone(), 5);
        let session = Session {
            branch_id: Some("test-branch".into()),
            messages: vec![ConversationMessage::user("hello")],
            ..Session::default()
        };
        mgr.save_branch(&session).unwrap();

        let loaded = mgr.load_branch("test-branch").unwrap();
        assert_eq!(loaded.messages.len(), 1);
        assert_eq!(loaded.branch_id.as_deref(), Some("test-branch"));
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn list_and_delete() {
        let dir = tmp_dir();
        let mgr = BranchManager::new(dir.clone(), 5);

        let s1 = Session {
            branch_id: Some("b1".into()),
            ..Session::default()
        };
        mgr.save_branch(&s1).unwrap();

        let s2 = Session {
            branch_id: Some("b2".into()),
            ..Session::default()
        };
        mgr.save_branch(&s2).unwrap();

        assert_eq!(mgr.list_branches().len(), 2);
        mgr.delete_branch("b1").unwrap();
        assert_eq!(mgr.list_branches().len(), 1);
        fs::remove_dir_all(dir).ok();
    }
}
