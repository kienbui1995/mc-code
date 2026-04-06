use std::fs;
use std::path::{Path, PathBuf};

struct FileSnapshot {
    path: PathBuf,
    original: Option<String>, // None = file didn't exist (was created)
}

struct TurnSnapshot {
    files: Vec<FileSnapshot>,
}

/// Undomanager.
pub struct UndoManager {
    current: Vec<FileSnapshot>,
    turns: Vec<TurnSnapshot>,
    max_turns: usize,
}

impl UndoManager {
    #[must_use]
    /// New.
    pub fn new(max_turns: usize) -> Self {
        Self {
            current: Vec::new(),
            turns: Vec::new(),
            max_turns,
        }
    }

    /// Snapshot a file before it gets modified. Call before `write_file`/`edit_file`.
    pub fn snapshot_before_write(&mut self, path: &Path) {
        // Don't snapshot same file twice in one turn
        if self.current.iter().any(|s| s.path == path) {
            return;
        }
        let original = fs::read_to_string(path).ok();
        self.current.push(FileSnapshot {
            path: path.to_path_buf(),
            original,
        });
    }

    /// Finalize current turn's snapshots.
    pub fn end_turn(&mut self) {
        if self.current.is_empty() {
            return;
        }
        let snap = TurnSnapshot {
            files: std::mem::take(&mut self.current),
        };
        self.turns.push(snap);
        while self.turns.len() > self.max_turns {
            self.turns.remove(0);
        }
    }

    /// Revert the last turn's file changes. Returns list of reverted paths.
    pub fn undo_last_turn(&mut self) -> Result<Vec<String>, std::io::Error> {
        let snap = self
            .turns
            .pop()
            .ok_or_else(|| std::io::Error::other("Nothing to undo"))?;
        let mut reverted = Vec::new();
        for file in &snap.files {
            match &file.original {
                Some(content) => fs::write(&file.path, content)?,
                None => {
                    let _ = fs::remove_file(&file.path);
                }
            }
            reverted.push(file.path.display().to_string());
        }
        Ok(reverted)
    }

    #[must_use]
    /// Can undo.
    pub fn can_undo(&self) -> bool {
        !self.turns.is_empty()
    }

    #[must_use]
    /// Turns available.
    pub fn turns_available(&self) -> usize {
        self.turns.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir() -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static C: AtomicU32 = AtomicU32::new(0);
        let d = std::env::temp_dir().join(format!(
            "mc-undo-{}-{}",
            std::process::id(),
            C.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn undo_reverts_edit() {
        let dir = tmp_dir();
        let file = dir.join("test.txt");
        fs::write(&file, "original").unwrap();

        let mut mgr = UndoManager::new(10);
        mgr.snapshot_before_write(&file);
        fs::write(&file, "modified").unwrap();
        mgr.end_turn();

        assert!(mgr.can_undo());
        let reverted = mgr.undo_last_turn().unwrap();
        assert_eq!(reverted.len(), 1);
        assert_eq!(fs::read_to_string(&file).unwrap(), "original");

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn undo_removes_created_file() {
        let dir = tmp_dir();
        let file = dir.join("new.txt");

        let mut mgr = UndoManager::new(10);
        mgr.snapshot_before_write(&file); // file doesn't exist yet
        fs::write(&file, "created").unwrap();
        mgr.end_turn();

        mgr.undo_last_turn().unwrap();
        assert!(!file.exists());

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn no_undo_when_empty() {
        let mut mgr = UndoManager::new(10);
        assert!(!mgr.can_undo());
        assert!(mgr.undo_last_turn().is_err());
    }

    #[test]
    fn max_turns_eviction() {
        let mut mgr = UndoManager::new(2);
        let dir = tmp_dir();
        for i in 0..3 {
            let f = dir.join(format!("{i}.txt"));
            fs::write(&f, "x").unwrap();
            mgr.snapshot_before_write(&f);
            mgr.end_turn();
        }
        assert_eq!(mgr.turns_available(), 2);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn dedup_same_file_in_turn() {
        let dir = tmp_dir();
        let file = dir.join("dup.txt");
        fs::write(&file, "v1").unwrap();

        let mut mgr = UndoManager::new(10);
        mgr.snapshot_before_write(&file);
        mgr.snapshot_before_write(&file); // duplicate
        mgr.end_turn();

        assert_eq!(mgr.turns.last().unwrap().files.len(), 1);
        fs::remove_dir_all(dir).ok();
    }
}
