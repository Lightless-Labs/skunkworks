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
