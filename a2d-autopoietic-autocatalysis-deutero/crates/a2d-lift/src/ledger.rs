//! Append-only JSONL ledger: registrations, invocations, submissions, scores.

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default()
}

pub struct Ledger {
    path: PathBuf,
    file: File,
    seq: u64,
}

impl Ledger {
    /// Create a new ledger. Refuses to reuse an existing file: one ledger per run.
    pub fn create(path: &Path) -> io::Result<Self> {
        let file = OpenOptions::new()
            .append(true)
            .create_new(true)
            .open(path)?;
        Ok(Self {
            path: path.to_path_buf(),
            file,
            seq: 0,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Append one record of `kind`. `fields` must be a JSON object.
    pub fn append(&mut self, kind: &str, fields: Value) -> io::Result<()> {
        let mut record = json!({ "seq": self.seq, "kind": kind, "ts_ms": unix_ms() as u64 });
        if let (Some(target), Value::Object(extra)) = (record.as_object_mut(), fields) {
            target.extend(extra);
        }
        let mut line = serde_json::to_string(&record)?;
        line.push('\n');
        self.file.write_all(line.as_bytes())?;
        self.file.sync_data()?;
        self.seq += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_numbered_records_and_refuses_reuse() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.jsonl");
        let mut ledger = Ledger::create(&path).unwrap();
        ledger
            .append("registration", json!({"experiment": "E00"}))
            .unwrap();
        ledger.append("score", json!({"passed": true})).unwrap();
        let lines: Vec<Value> = std::fs::read_to_string(&path)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["seq"], 0);
        assert_eq!(lines[0]["kind"], "registration");
        assert_eq!(lines[1]["seq"], 1);
        assert_eq!(lines[1]["passed"], true);
        assert!(Ledger::create(&path).is_err());
    }
}
