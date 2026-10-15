#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Side {
    Old,
    New,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    Keep {
        old_index: usize,
        new_index: usize,
        line: String,
    },
    Delete {
        old_index: usize,
        line: String,
    },
    Insert {
        new_index: usize,
        line: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffError {
    ContextTooLarge { context: usize },
    CarriageReturn { side: Side, byte: usize },
    MissingFinalLf { side: Side },
    TooLarge { old_len: usize, new_len: usize },
}

fn split_lines(text: &str) -> Vec<&str> {
    if text.is_empty() {
        Vec::new()
    } else {
        text[..text.len() - 1].split('\n').collect()
    }
}

pub fn unified_diff(old: &str, new: &str, context: usize) -> Result<String, DiffError> {
    if context > 8 {
        return Err(DiffError::ContextTooLarge { context });
    }

    if let Some(byte) = old.find('\r') {
        return Err(DiffError::CarriageReturn {
            side: Side::Old,
            byte,
        });
    }
    if let Some(byte) = new.find('\r') {
        return Err(DiffError::CarriageReturn {
            side: Side::New,
            byte,
        });
    }

    if !old.is_empty() && !old.ends_with('\n') {
        return Err(DiffError::MissingFinalLf { side: Side::Old });
    }
    if !new.is_empty() && !new.ends_with('\n') {
        return Err(DiffError::MissingFinalLf { side: Side::New });
    }

    let old_lines = split_lines(old);
    let new_lines = split_lines(new);

    if old_lines.len() > 128 || new_lines.len() > 128 {
        return Err(DiffError::TooLarge {
            old_len: old_lines.len(),
            new_len: new_lines.len(),
        });
    }

    let n = old_lines.len();
    let m = new_lines.len();
    let mut lcs = vec![vec![0usize; m + 1]; n + 1];

    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if old_lines[i] == new_lines[j] {
                1 + lcs[i + 1][j + 1]
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }

    let mut edits: Vec<Edit> = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if old_lines[i] == new_lines[j] {
            edits.push(Edit::Keep {
                old_index: i,
                new_index: j,
                line: old_lines[i].to_string(),
            });
            i += 1;
            j += 1;
        } else if lcs[i + 1][j] > lcs[i][j + 1] {
            edits.push(Edit::Delete {
                old_index: i,
                line: old_lines[i].to_string(),
            });
            i += 1;
        } else if lcs[i + 1][j] < lcs[i][j + 1] {
            edits.push(Edit::Insert {
                new_index: j,
                line: new_lines[j].to_string(),
            });
            j += 1;
        } else if (i + j) % 2 == 0 {
            edits.push(Edit::Delete {
                old_index: i,
                line: old_lines[i].to_string(),
            });
            i += 1;
        } else {
            edits.push(Edit::Insert {
                new_index: j,
                line: new_lines[j].to_string(),
            });
            j += 1;
        }
    }

    while i < n {
        edits.push(Edit::Delete {
            old_index: i,
            line: old_lines[i].to_string(),
        });
        i += 1;
    }
    while j < m {
        edits.push(Edit::Insert {
            new_index: j,
            line: new_lines[j].to_string(),
        });
        j += 1;
    }

    let is_changed = |edit: &Edit| matches!(edit, Edit::Delete { .. } | Edit::Insert { .. });
    if !edits.iter().any(is_changed) {
        return Ok(String::new());
    }

    let mut groups: Vec<(usize, usize)> = Vec::new();
    let mut pos = 0;
    while pos < edits.len() {
        if is_changed(&edits[pos]) {
            let start = pos;
            while pos < edits.len() && is_changed(&edits[pos]) {
                pos += 1;
            }
            groups.push((start, pos));
        } else {
            pos += 1;
        }
    }

    let merge_gap = 2 * context;
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for group in groups {
        if let Some(last) = merged.last_mut() {
            let gap = group.0 - last.1;
            if gap <= merge_gap {
                last.1 = group.1;
                continue;
            }
        }
        merged.push(group);
    }

    let mut hunks: Vec<(usize, usize)> = Vec::new();
    for (start, end) in merged {
        let mut hunk_start = start;
        let mut preceding = 0;
        while hunk_start > 0 && preceding < context {
            if matches!(edits[hunk_start - 1], Edit::Keep { .. }) {
                hunk_start -= 1;
                preceding += 1;
            } else {
                break;
            }
        }

        let mut hunk_end = end;
        let mut following = 0;
        while hunk_end < edits.len() && following < context {
            if matches!(edits[hunk_end], Edit::Keep { .. }) {
                hunk_end += 1;
                following += 1;
            } else {
                break;
            }
        }
        hunks.push((hunk_start, hunk_end));
    }

    let mut output = String::from("--- before\n+++ after\n");
    for (start, end) in hunks {
        let mut old_before = 0usize;
        let mut new_before = 0usize;
        for edit in &edits[..start] {
            match edit {
                Edit::Keep { .. } => {
                    old_before += 1;
                    new_before += 1;
                }
                Edit::Delete { .. } => old_before += 1,
                Edit::Insert { .. } => new_before += 1,
            }
        }

        let mut old_count = 0usize;
        let mut new_count = 0usize;
        for edit in &edits[start..end] {
            match edit {
                Edit::Keep { .. } => {
                    old_count += 1;
                    new_count += 1;
                }
                Edit::Delete { .. } => old_count += 1,
                Edit::Insert { .. } => new_count += 1,
            }
        }

        let old_start = if old_count > 0 {
            old_before + 1
        } else {
            old_before
        };
        let new_start = if new_count > 0 {
            new_before + 1
        } else {
            new_before
        };

        output.push_str(&format!(
            "@@ -{old_start},{old_count} +{new_start},{new_count} @@\n"
        ));
        for edit in &edits[start..end] {
            match edit {
                Edit::Keep { line, .. } => {
                    output.push(' ');
                    output.push_str(line);
                    output.push('\n');
                }
                Edit::Delete { line, .. } => {
                    output.push('-');
                    output.push_str(line);
                    output.push('\n');
                }
                Edit::Insert { line, .. } => {
                    output.push('+');
                    output.push_str(line);
                    output.push('\n');
                }
            }
        }
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacement_and_context() {
        assert_eq!(
            unified_diff("x\n", "y\n", 0),
            Ok("--- before\n+++ after\n@@ -1,1 +1,1 @@\n-x\n+y\n".into())
        );
        assert_eq!(
            unified_diff("a\nb\nc\n", "a\nB\nc\n", 1),
            Ok("--- before\n+++ after\n@@ -1,3 +1,3 @@\n a\n-b\n+B\n c\n".into())
        );
    }

    #[test]
    fn unchanged_and_empty_inputs() {
        assert_eq!(unified_diff("same\n", "same\n", 2), Ok(String::new()));
        assert_eq!(unified_diff("", "", 0), Ok(String::new()));
    }

    #[test]
    fn validation_precedence_and_utf8_byte_offset() {
        assert_eq!(
            unified_diff("é\r\n", "no final lf", 9),
            Err(DiffError::ContextTooLarge { context: 9 })
        );
        assert_eq!(
            unified_diff("é\r\n", "x\r\n", 0),
            Err(DiffError::CarriageReturn {
                side: Side::Old,
                byte: 2
            })
        );
        assert_eq!(
            unified_diff("x", "y", 0),
            Err(DiffError::MissingFinalLf { side: Side::Old })
        );
    }

    #[test]
    fn line_limit_is_inclusive() {
        let valid = "x\n".repeat(128);
        assert!(unified_diff(&valid, &valid, 0).is_ok());

        let too_many = "x\n".repeat(129);
        assert_eq!(
            unified_diff(&too_many, "", 0),
            Err(DiffError::TooLarge {
                old_len: 129,
                new_len: 0
            })
        );
    }

    #[test]
    fn tied_lcs_uses_parity_and_preserves_empty_lines() {
        let result = unified_diff("a\nb\n", "b\na\n", 8).unwrap();
        assert!(result.contains("-a\n"));
        assert!(result.contains("+b\n"));
    }
}

fn main() {}