use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn fake_sudoku_artifact_with_bad_solver() -> &'static str {
    r#"
fn parse(s: &str) -> [[u8; 9]; 9] {
    let mut grid = [[0u8; 9]; 9];
    for (idx, ch) in s.chars().take(81).enumerate() {
        grid[idx / 9][idx % 9] = ch.to_digit(10).unwrap_or(0) as u8;
    }
    grid
}

fn solve(_grid: [[u8; 9]; 9]) -> Option<[[u8; 9]; 9]> {
    Some([[1u8; 9]; 9])
}

fn validate(_grid: &[[u8; 9]; 9]) -> bool {
    true
}

fn main() {}

#[cfg(test)]
mod tests {
    #[test]
    fn local_smoke_passes() {
        assert!(true);
    }
}
"#
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock must be after epoch")
                .as_nanos()
        ));
        fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct TempArtifact {
    path: PathBuf,
}

impl TempArtifact {
    fn write(contents: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "a2d-bad-sudoku-artifact-{}-{}.rs",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock must be after epoch")
                .as_nanos()
        ));
        fs::write(&path, contents).expect("write fake artifact");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempArtifact {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[test]
fn score_artifact_path_uses_hidden_acceptance_and_exits_nonzero() {
    let artifact = TempArtifact::write(fake_sudoku_artifact_with_bad_solver());

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "score-artifact",
            "sudoku",
            artifact.path().to_str().unwrap(),
        ])
        .output()
        .expect("run score-artifact");

    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Fitness: 83% (5/6)"), "{stdout}");
    assert!(stdout.contains("✗ all_tests_pass"), "{stdout}");
    assert!(
        stdout.contains("Diagnostic: captured but not printed"),
        "{stdout}"
    );
    assert!(!stdout.contains("800000000003600000"));
}

#[test]
fn score_artifact_exports_fitness_evidence_before_nonzero_exit() {
    let artifact = TempArtifact::write(fake_sudoku_artifact_with_bad_solver());
    let export_dir = TempDir::new("a2d-score-artifact-evidence");

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "score-artifact",
            "sudoku",
            artifact.path().to_str().unwrap(),
        ])
        .env("A2D_FITNESS_EVIDENCE_EXPORT_DIR", export_dir.path())
        .output()
        .expect("run score-artifact");

    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Fitness evidence:"), "{stdout}");

    let evidence_path = export_dir
        .path()
        .join("baseline-sudoku-solver-cycle-0-fitness-evidence.json");
    let evidence_bytes = fs::read(&evidence_path).expect("evidence exported before exit");
    let evidence: serde_json::Value =
        serde_json::from_slice(&evidence_bytes).expect("evidence is JSON");

    assert_eq!(evidence["schema_version"], "a2d.fitness-evidence.v1");
    assert_eq!(evidence["actual_tests_evaluated"], true);
    assert_eq!(evidence["cycle"], 0);
    assert_eq!(evidence["non_regressing"], true);
    assert_eq!(evidence["fitness"], serde_json::json!(5.0 / 6.0));
    assert_eq!(
        evidence["failed_cases"],
        serde_json::json!(["all_tests_pass"])
    );
}

fn assert_exported_provenance(evidence: &serde_json::Value) {
    assert_eq!(evidence["schema_version"], "a2d.fitness-evidence.v1");
    assert_eq!(evidence["source_diff_scope"], "crates");
    assert!(
        evidence["source_tree_dirty"].as_bool().is_some(),
        "{evidence}"
    );
    assert!(
        evidence["source_revision"]
            .as_str()
            .is_some_and(|revision| !revision.is_empty()),
        "{evidence}"
    );
    assert!(
        evidence["source_diff_hash"]
            .as_str()
            .is_some_and(|hash| hash.len() == 40),
        "{evidence}"
    );
}

#[test]
fn score_artifact_export_provenance_works_from_crate_subdirectory() {
    let artifact = TempArtifact::write(fake_sudoku_artifact_with_bad_solver());
    let export_dir = TempDir::new("a2d-score-artifact-nested-evidence");

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "score-artifact",
            "sudoku",
            artifact.path().to_str().unwrap(),
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("A2D_FITNESS_EVIDENCE_EXPORT_DIR", export_dir.path())
        .output()
        .expect("run score-artifact from crate subdirectory");

    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Fitness evidence:"), "{stdout}");

    let evidence_path = export_dir
        .path()
        .join("baseline-sudoku-solver-cycle-0-fitness-evidence.json");
    let evidence_bytes = fs::read(&evidence_path).expect("evidence exported before exit");
    let evidence: serde_json::Value =
        serde_json::from_slice(&evidence_bytes).expect("evidence is JSON");

    assert_exported_provenance(&evidence);
}

#[test]
fn score_artifact_export_provenance_works_from_parent_repo_root() {
    let artifact = TempArtifact::write(fake_sudoku_artifact_with_bad_solver());
    let export_dir = TempDir::new("a2d-score-artifact-parent-root-evidence");
    let parent_repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("crate must live under parent repo root");

    let output = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args([
            "score-artifact",
            "sudoku",
            artifact.path().to_str().unwrap(),
        ])
        .current_dir(parent_repo_root)
        .env("A2D_FITNESS_EVIDENCE_EXPORT_DIR", export_dir.path())
        .output()
        .expect("run score-artifact from parent repo root");

    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Fitness evidence:"), "{stdout}");

    let evidence_path = export_dir
        .path()
        .join("baseline-sudoku-solver-cycle-0-fitness-evidence.json");
    let evidence_bytes = fs::read(&evidence_path).expect("evidence exported before exit");
    let evidence: serde_json::Value =
        serde_json::from_slice(&evidence_bytes).expect("evidence is JSON");

    assert_exported_provenance(&evidence);
}

#[test]
fn score_artifact_stdin_uses_hidden_acceptance_and_exits_nonzero() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_a2d"))
        .args(["score-artifact", "sudoku", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn score-artifact");
    child
        .stdin
        .as_mut()
        .expect("stdin must be piped")
        .write_all(fake_sudoku_artifact_with_bad_solver().as_bytes())
        .expect("write artifact to stdin");

    let output = child.wait_with_output().expect("wait for score-artifact");

    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Fitness: 83% (5/6)"), "{stdout}");
    assert!(stdout.contains("✗ all_tests_pass"), "{stdout}");
    assert!(!stdout.contains("800000000003600000"));
}
