//! Lineage Archive: git-backed persistence of germline mutations.
//!
//! Every accepted mutation is a commit. Every rejected mutation is logged.
//! Rollback to any prior state is always possible (Constitution Invariant 6).

use crate::germline::Germline;
use crate::metabolism::CycleReport;
use crate::types::EnzymeDef;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Git-backed lineage archive for germline state.
pub struct LineageArchive {
    root: PathBuf,
    germline_path: PathBuf,
}

impl LineageArchive {
    /// Initialize a lineage archive at the given root directory.
    /// Creates the directory and git repo if they don't exist.
    pub fn init(root: &Path) -> Result<Self, std::io::Error> {
        fs::create_dir_all(root)?;

        let germline_path = root.join("germline.json");

        // Init git if not already a repo
        if !root.join(".git").exists() {
            Command::new("git")
                .args(["init"])
                .current_dir(root)
                .output()?;
        }

        Ok(Self {
            root: root.to_path_buf(),
            germline_path,
        })
    }

    /// Persist the current germline state as a git commit.
    pub fn commit_germline(
        &self,
        germline: &Germline,
        report: &CycleReport,
    ) -> Result<String, std::io::Error> {
        let enzymes: Vec<&EnzymeDef> = germline.enzymes();
        let json = serde_json::to_string_pretty(&enzymes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        fs::write(&self.germline_path, &json)?;

        // Stage
        Command::new("git")
            .args(["add", "germline.json"])
            .current_dir(&self.root)
            .output()?;

        // Commit
        let fitness_str = report
            .fitness
            .as_ref()
            .map(|f| {
                format!(
                    ", Fitness {:.0}% ({}/{})",
                    f.fitness * 100.0,
                    f.passed,
                    f.total
                )
            })
            .unwrap_or_default();

        let message = format!(
            "Cycle {}: {} invocations, {} accepted, {} rejected, RAF {:.0}%{fitness_str}",
            report.cycle,
            report.invocations,
            report.accepted_mutations,
            report.rejected_mutations,
            germline.raf_status().coverage * 100.0,
        );

        let output = Command::new("git")
            .args(["commit", "-m", &message, "--allow-empty"])
            .current_dir(&self.root)
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let commit_hash = stdout.lines().next().unwrap_or("unknown").to_string();

        Ok(commit_hash)
    }

    /// Read the germline from the archive (latest committed state).
    pub fn read_germline(&self) -> Result<Vec<EnzymeDef>, std::io::Error> {
        let json = fs::read_to_string(&self.germline_path)?;
        let enzymes: Vec<EnzymeDef> = serde_json::from_str(&json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(enzymes)
    }

    /// List all commits (germline lineage).
    pub fn log(&self, max: usize) -> Result<Vec<String>, std::io::Error> {
        let output = Command::new("git")
            .args(["log", "--oneline", &format!("-{max}")])
            .current_dir(&self.root)
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.lines().map(|l| l.to_string()).collect())
    }

    /// Rollback to a specific commit.
    pub fn rollback(&self, commit: &str) -> Result<(), std::io::Error> {
        Command::new("git")
            .args(["checkout", commit, "--", "germline.json"])
            .current_dir(&self.root)
            .output()?;
        Ok(())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ArtifactType, EnzymeId};
    use std::collections::BTreeSet;

    fn test_enzyme(id: &str) -> EnzymeDef {
        EnzymeDef {
            id: EnzymeId::from(id),
            reactants: BTreeSet::from([ArtifactType::from("input")]),
            products: BTreeSet::from([ArtifactType::from("output")]),
            catalysts: BTreeSet::from([ArtifactType::from("input")]),
            ..Default::default()
        }
    }

    #[test]
    fn init_creates_git_repo() {
        let dir = tempfile::tempdir().unwrap();
        let archive = LineageArchive::init(dir.path()).unwrap();
        assert!(archive.root().join(".git").exists());
    }

    #[test]
    fn commit_and_read_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let archive = LineageArchive::init(dir.path()).unwrap();

        let germline = Germline::new(
            vec![test_enzyme("a"), test_enzyme("b")],
            BTreeSet::from([ArtifactType::from("input")]),
        );

        let report = CycleReport {
            cycle: 1,
            invocations: 3,
            accepted_mutations: 2,
            ..CycleReport::default()
        };

        // Configure git user for test
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "test"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        archive.commit_germline(&germline, &report).unwrap();

        let read_back = archive.read_germline().unwrap();
        assert_eq!(read_back.len(), 2);
        assert_eq!(read_back[0].id, EnzymeId::from("a"));
    }

    #[test]
    fn log_shows_commits() {
        let dir = tempfile::tempdir().unwrap();
        let archive = LineageArchive::init(dir.path()).unwrap();

        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "test"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        let germline = Germline::new(
            vec![test_enzyme("x")],
            BTreeSet::from([ArtifactType::from("input")]),
        );

        let report1 = CycleReport {
            cycle: 1,
            invocations: 2,
            ..CycleReport::default()
        };
        let report2 = CycleReport {
            cycle: 2,
            invocations: 5,
            ..CycleReport::default()
        };

        archive.commit_germline(&germline, &report1).unwrap();
        archive.commit_germline(&germline, &report2).unwrap();

        let log = archive.log(10).unwrap();
        assert_eq!(log.len(), 2);
    }
}
