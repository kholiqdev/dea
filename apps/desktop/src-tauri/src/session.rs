//! Which project each window shows. Pure bookkeeping, so it can be tested
//! without windows; the watchers live next to it in `AppState`.

use std::collections::HashMap;
use std::path::PathBuf;

/// The project of one window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub root: PathBuf,
    pub scope: String,
    /// 0 for the first window of the project, 1 for the second, …
    pub slot: u8,
    pub active_environment: Option<String>,
    /// The project folder is gone.
    pub missing: bool,
}

/// Why a window cannot write to its project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    NoProject,
    Missing,
}

#[derive(Debug, Default)]
pub struct Sessions {
    by_window: HashMap<String, Session>,
}

impl Sessions {
    /// Gives `window` the project at `root` and returns its slot: the
    /// lowest one no other window of the same project uses.
    pub fn bind(&mut self, window: &str, root: PathBuf, scope: String) -> u8 {
        self.by_window.remove(window);
        let used: Vec<u8> = self
            .by_window
            .values()
            .filter(|s| s.scope == scope)
            .map(|s| s.slot)
            .collect();
        let slot = (0..=u8::MAX)
            .find(|n| !used.contains(n))
            .expect("fewer than 256 windows of one project");
        self.by_window.insert(
            window.to_string(),
            Session {
                root,
                scope,
                slot,
                active_environment: None,
                missing: false,
            },
        );
        slot
    }

    pub fn unbind(&mut self, window: &str) -> Option<Session> {
        self.by_window.remove(window)
    }

    pub fn get(&self, window: &str) -> Option<&Session> {
        self.by_window.get(window)
    }

    pub fn set_environment(&mut self, window: &str, name: Option<String>) {
        if let Some(s) = self.by_window.get_mut(window) {
            s.active_environment = name;
        }
    }

    /// How many windows of project `scope` use `environment` for requests.
    pub fn windows_using(&self, scope: &str, environment: &str) -> u32 {
        let n = self
            .by_window
            .values()
            .filter(|s| s.scope == scope && s.active_environment.as_deref() == Some(environment))
            .count();
        u32::try_from(n).unwrap_or(u32::MAX)
    }

    /// Notes whether the window's project folder is gone; returns it.
    pub fn refresh_missing(&mut self, window: &str) -> bool {
        let Some(s) = self.by_window.get_mut(window) else {
            return false;
        };
        s.missing = !s.root.join("dea.yaml").is_file();
        s.missing
    }

    /// The window's session, when its project can be written to. The
    /// folder is checked again: a move sends no event for the root.
    pub fn writable(&mut self, window: &str) -> Result<&Session, Refusal> {
        if !self.by_window.contains_key(window) {
            return Err(Refusal::NoProject);
        }
        if self.refresh_missing(window) {
            return Err(Refusal::Missing);
        }
        self.by_window.get(window).ok_or(Refusal::NoProject)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn bind(s: &mut Sessions, window: &str, scope: &str) -> u8 {
        s.bind(window, PathBuf::from("/p"), scope.into())
    }

    #[test]
    fn the_second_window_of_a_project_gets_slot_one() {
        let mut s = Sessions::default();
        assert_eq!(bind(&mut s, "main", "p"), 0);
        assert_eq!(bind(&mut s, "project-1", "p"), 1);
        assert_eq!(bind(&mut s, "project-2", "q"), 0);
    }

    #[test]
    fn a_freed_slot_is_reused() {
        let mut s = Sessions::default();
        bind(&mut s, "main", "p");
        bind(&mut s, "project-1", "p");
        s.unbind("main");
        assert_eq!(bind(&mut s, "project-2", "p"), 0);
        // Rebinding a window to the same project keeps one slot for it.
        assert_eq!(bind(&mut s, "project-2", "p"), 0);
    }

    #[test]
    fn windows_using_counts_only_that_scope_and_environment() {
        let mut s = Sessions::default();
        bind(&mut s, "a", "p");
        bind(&mut s, "b", "p");
        bind(&mut s, "c", "q");
        s.set_environment("a", Some("staging".into()));
        s.set_environment("b", Some("staging".into()));
        s.set_environment("c", Some("staging".into()));
        assert_eq!(s.windows_using("p", "staging"), 2);
        s.set_environment("b", Some("local".into()));
        assert_eq!(s.windows_using("p", "staging"), 1);
        assert_eq!(s.windows_using("p", "production"), 0);
    }

    #[test]
    fn a_removed_project_folder_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("harbor");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("dea.yaml"), "name: harbor\nschemaVersion: 1\n").unwrap();
        let mut s = Sessions::default();
        s.bind("main", root.clone(), "p".into());
        assert!(!s.refresh_missing("main"));
        assert!(s.writable("main").is_ok());

        fs::remove_dir_all(&root).unwrap();
        assert!(s.refresh_missing("main"));
        assert_eq!(s.writable("main"), Err(Refusal::Missing));
        assert_eq!(s.writable("other"), Err(Refusal::NoProject));
    }

    #[test]
    fn writable_notices_a_folder_that_went_away_and_came_back() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("p");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("dea.yaml"), "name: p\nschemaVersion: 1\n").unwrap();
        let mut s = Sessions::default();
        s.bind("main", root.clone(), "p".into());

        // Moved away (Finder sends no event for the watched root).
        let moved = dir.path().join("moved");
        fs::rename(&root, &moved).unwrap();
        assert_eq!(s.writable("main").err(), Some(Refusal::Missing));

        fs::rename(&moved, &root).unwrap();
        assert!(s.writable("main").is_ok());
    }
}
