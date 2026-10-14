//! Task suites: directories of `<task>/task.json`, `public_tests.rs`,
//! `hidden_tests.rs`.
//!
//! Hidden tests are private to this crate. Arms receive a [`PublicTask`],
//! which has no hidden field; only the oracle reads [`Task::hidden_tests`].

use crate::ledger::sha256_hex;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

pub const TASK_FILE: &str = "task.json";
pub const PUBLIC_TESTS_FILE: &str = "public_tests.rs";
pub const HIDDEN_TESTS_FILE: &str = "hidden_tests.rs";

#[derive(Debug, Deserialize)]
struct TaskFile {
    id: String,
    family: String,
    requirements: String,
    #[serde(default = "default_test_timeout_secs")]
    test_timeout_secs: u64,
}

fn default_test_timeout_secs() -> u64 {
    30
}

/// A task as the controller holds it. `Debug` redacts the hidden tests.
#[derive(Clone)]
pub struct Task {
    pub id: String,
    pub family: String,
    pub requirements: String,
    pub public_tests: String,
    hidden_tests: String,
    pub test_timeout_secs: u64,
    dir: PathBuf,
}

/// What an arm (and every enzyme inside arm C) may see.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicTask {
    pub id: String,
    pub family: String,
    pub requirements: String,
    pub public_tests: String,
    pub test_timeout_secs: u64,
}

impl std::fmt::Debug for Task {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Task")
            .field("id", &self.id)
            .field("family", &self.family)
            .field("hidden_tests", &"<redacted>")
            .finish_non_exhaustive()
    }
}

impl Task {
    pub fn public(&self) -> PublicTask {
        PublicTask {
            id: self.id.clone(),
            family: self.family.clone(),
            requirements: self.requirements.clone(),
            public_tests: self.public_tests.clone(),
            test_timeout_secs: self.test_timeout_secs,
        }
    }

    pub(crate) fn hidden_tests(&self) -> &str {
        &self.hidden_tests
    }

    /// The task's directory (validation-only files live here).
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    #[cfg(test)]
    pub(crate) fn for_test(id: &str, family: &str, public: &str, hidden: &str) -> Self {
        Self {
            id: id.to_string(),
            family: family.to_string(),
            requirements: format!("Implement task {id}."),
            public_tests: public.to_string(),
            hidden_tests: hidden.to_string(),
            test_timeout_secs: 10,
            dir: PathBuf::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Suite {
    pub root: PathBuf,
    pub hash: String,
    pub tasks: Vec<Task>,
}

impl Suite {
    pub fn families(&self) -> usize {
        let mut families: Vec<&str> = self.tasks.iter().map(|t| t.family.as_str()).collect();
        families.sort_unstable();
        families.dedup();
        families.len()
    }
}

/// Load every `<dir>/<task>/task.json`, sorted by task id. When
/// `expected_hash` is given, the suite hash must match it.
pub fn load_suite(dir: &Path, expected_hash: Option<&str>) -> Result<Suite, String> {
    let hash = suite_hash(dir)?;
    if let Some(expected) = expected_hash
        && expected != hash
    {
        return Err(format!(
            "suite hash mismatch for {}: expected {expected}, found {hash}",
            dir.display()
        ));
    }
    let mut tasks = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| format!("read {}: {e}", dir.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.join(TASK_FILE).is_file() {
            tasks.push(load_task(&path)?);
        }
    }
    if tasks.is_empty() {
        return Err(format!("no tasks under {}", dir.display()));
    }
    tasks.sort_by(|a, b| a.id.cmp(&b.id));
    for pair in tasks.windows(2) {
        if pair[0].id == pair[1].id {
            return Err(format!("duplicate task id {}", pair[0].id));
        }
    }
    Ok(Suite {
        root: dir.to_path_buf(),
        hash,
        tasks,
    })
}

fn load_task(dir: &Path) -> Result<Task, String> {
    let read = |name: &str| fs::read_to_string(dir.join(name));
    let raw = read(TASK_FILE).map_err(|e| format!("{}: {e}", dir.display()))?;
    let file: TaskFile =
        serde_json::from_str(&raw).map_err(|e| format!("{}/{TASK_FILE}: {e}", dir.display()))?;
    if !is_safe_id(&file.id) || !is_safe_id(&file.family) {
        return Err(format!(
            "{}: task id and family must be [A-Za-z0-9._-]+ and not start with '.'",
            dir.display()
        ));
    }
    let hidden_tests = read(HIDDEN_TESTS_FILE)
        .map_err(|e| format!("{}/{HIDDEN_TESTS_FILE}: {e}", dir.display()))?;
    if count_tests(&hidden_tests) == 0 {
        return Err(format!(
            "{}: hidden tests contain no #[test]",
            dir.display()
        ));
    }
    Ok(Task {
        id: file.id,
        family: file.family,
        requirements: file.requirements,
        public_tests: read(PUBLIC_TESTS_FILE).unwrap_or_default(),
        hidden_tests,
        test_timeout_secs: file.test_timeout_secs,
        dir: dir.to_path_buf(),
    })
}

/// Ids become path components of run artifacts.
fn is_safe_id(id: &str) -> bool {
    !id.is_empty()
        && !id.starts_with('.')
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// Number of `#[test]` functions in a test source file.
pub fn count_tests(source: &str) -> usize {
    source.matches("#[test]").count()
}

/// SHA-256 over every file under `dir`, in sorted relative-path order:
/// `path NUL sha256(content) NUL` per file. Commit this, not the files,
/// for holdout sets.
pub fn suite_hash(dir: &Path) -> Result<String, String> {
    let mut files = Vec::new();
    collect_files(dir, dir, &mut files)?;
    files.sort();
    let mut hasher = Sha256::new();
    for rel in files {
        let bytes = fs::read(dir.join(&rel)).map_err(|e| format!("read {rel}: {e}"))?;
        hasher.update(rel.as_bytes());
        hasher.update([0]);
        hasher.update(sha256_hex(&bytes).as_bytes());
        hasher.update([0]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| format!("read {}: {e}", dir.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            collect_files(root, &path, out)?;
        } else {
            let rel = path
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            out.push(rel);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_task(root: &Path, id: &str, hidden: &str) {
        let dir = root.join(id);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(TASK_FILE),
            format!(r#"{{"id":"{id}","family":"fam","requirements":"Do {id}."}}"#),
        )
        .unwrap();
        fs::write(dir.join(HIDDEN_TESTS_FILE), hidden).unwrap();
        fs::write(dir.join(PUBLIC_TESTS_FILE), "#[test]\nfn visible() {}\n").unwrap();
    }

    #[test]
    fn loads_tasks_sorted_and_hides_hidden_tests_from_public_view() {
        let dir = tempfile::tempdir().unwrap();
        write_task(dir.path(), "b-task", "#[test]\nfn secret_b() {}\n");
        write_task(dir.path(), "a-task", "#[test]\nfn secret_a() {}\n");
        let suite = load_suite(dir.path(), None).unwrap();
        let ids: Vec<_> = suite.tasks.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["a-task", "b-task"]);
        let public = format!("{:?}", suite.tasks[0].public());
        assert!(!public.contains("secret_a"));
        assert!(public.contains("visible"));
        assert_eq!(suite.tasks[0].test_timeout_secs, 30);
    }

    #[test]
    fn suite_hash_is_stable_and_detects_hidden_test_edits() {
        let dir = tempfile::tempdir().unwrap();
        write_task(dir.path(), "a-task", "#[test]\nfn secret() {}\n");
        let first = suite_hash(dir.path()).unwrap();
        assert_eq!(first, suite_hash(dir.path()).unwrap());
        assert!(load_suite(dir.path(), Some(&first)).is_ok());
        fs::write(
            dir.path().join("a-task").join(HIDDEN_TESTS_FILE),
            "#[test]\nfn secret() { assert!(true); }\n",
        )
        .unwrap();
        let err = load_suite(dir.path(), Some(&first)).unwrap_err();
        assert!(err.contains("hash mismatch"), "{err}");
    }

    #[test]
    fn rejects_task_without_hidden_tests() {
        let dir = tempfile::tempdir().unwrap();
        write_task(dir.path(), "a-task", "fn helper() {}\n");
        assert!(load_suite(dir.path(), None).is_err());
    }
}
