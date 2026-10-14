//! Append-only JSONL ledger: registrations, invocations, submissions, scores.

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
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

/// Verify sequence numbers and predecessor hashes, naming the first broken line.
/// Returns the record count and last line's hash (None for an empty file).
pub fn verify_chain(path: &Path) -> Result<(usize, Option<String>), String> {
    let file = File::open(path).map_err(|e| format!("line 1: {e}"))?;
    let mut count = 0usize;
    let mut last_hash: Option<String> = None;
    for (index, line) in BufReader::new(file).split(b'\n').enumerate() {
        let number = index + 1;
        let line = line.map_err(|e| format!("line {number}: {e}"))?;
        let record: Value =
            serde_json::from_slice(&line).map_err(|e| format!("line {number}: {e}"))?;
        if record.get("seq").and_then(Value::as_u64) != Some(index as u64) {
            return Err(format!("line {number}: invalid sequence number"));
        }
        let valid_previous = match last_hash.as_deref() {
            None => record.get("prev_sha256") == Some(&Value::Null),
            Some(hash) => record.get("prev_sha256").and_then(Value::as_str) == Some(hash),
        };
        if !valid_previous {
            return Err(format!("line {number}: invalid prev_sha256"));
        }
        last_hash = Some(sha256_hex(&line));
        count += 1;
    }
    Ok((count, last_hash))
}

pub struct Ledger {
    path: PathBuf,
    file: File,
    seq: u64,
    last_line_hash: Option<String>,
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
            last_line_hash: None,
        })
    }

    /// Reopen an existing ledger to continue its chain (deferred scoring).
    /// The chain is verified first.
    pub fn resume(path: &Path) -> Result<Self, String> {
        let (records, head) = verify_chain(path)?;
        let file = OpenOptions::new()
            .append(true)
            .open(path)
            .map_err(|e| format!("open {}: {e}", path.display()))?;
        Ok(Self {
            path: path.to_path_buf(),
            file,
            seq: records as u64,
            last_line_hash: head,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Hash of the last written line, excluding its newline.
    pub fn head(&self) -> Option<&str> {
        self.last_line_hash.as_deref()
    }

    /// Append one record of `kind`. `fields` must be a JSON object.
    pub fn append(&mut self, kind: &str, fields: Value) -> io::Result<()> {
        let mut record = json!({ "seq": self.seq, "kind": kind, "ts_ms": unix_ms() as u64 });
        if let (Some(target), Value::Object(extra)) = (record.as_object_mut(), fields) {
            target.extend(extra);
        }
        record["seq"] = json!(self.seq);
        record["prev_sha256"] = json!(self.last_line_hash);
        let mut line = serde_json::to_string(&record)?;
        let hash = sha256_hex(line.as_bytes());
        line.push('\n');
        self.file.write_all(line.as_bytes())?;
        self.file.sync_data()?;
        self.seq += 1;
        self.last_line_hash = Some(hash);
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
        assert_eq!(ledger.head(), None);
        assert_eq!(verify_chain(&path).unwrap(), (0, None));
        ledger
            .append("registration", json!({"experiment": "E00"}))
            .unwrap();
        ledger.append("score", json!({"passed": true})).unwrap();
        let contents = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<Value> = contents
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["seq"], 0);
        assert_eq!(lines[0]["kind"], "registration");
        assert_eq!(lines[1]["seq"], 1);
        assert_eq!(lines[1]["passed"], true);
        assert_eq!(lines[0]["prev_sha256"], Value::Null);
        let raw_lines: Vec<_> = contents.lines().collect();
        assert_eq!(lines[1]["prev_sha256"], sha256_hex(raw_lines[0].as_bytes()));
        let last_hash = sha256_hex(raw_lines[1].as_bytes());
        assert_eq!(ledger.head(), Some(last_hash.as_str()));
        assert_eq!(verify_chain(&path).unwrap(), (2, Some(last_hash)));
        assert!(Ledger::create(&path).is_err());
        std::fs::write(&path, contents.replace("E00", "E01")).unwrap();
        let error = verify_chain(&path).unwrap_err();
        assert!(
            error.contains("line 1") || error.contains("line 2"),
            "{error}"
        );
    }
}
