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

    if let Some(byte) = old.as_bytes().iter().position(|&b| b == b'\r') {
        return Err(DiffError::CarriageReturn {
            side: Side::Old,
            byte,
        });
    }
    if let Some(byte) = new.as_bytes().iter().position(|&b| b == b'\r') {
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

    let mut edits = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if old_lines[i] == new_lines[j] {
            edits.push(Edit::Keep {
                old_index: i,
                new_index: j,
                line: old_lines[i].to_owned(),
            });
            i += 1;
            j += 1;
        } else if lcs[i + 1][j] > lcs[i][j + 1] {
            edits.push(Edit::Delete {
                old_index: i,
                line: old_lines[i].to_owned(),
            });
            i += 1;
        } else if lcs[i + 1][j] < lcs[i][j + 1] {
            edits.push(Edit::Insert {
                new_index: j,
                line: new_lines[j].to_owned(),
            });
            j += 1;
        } else if (i + j) % 2 == 0 {
            edits.push(Edit::Delete {
                old_index: i,
                line: old_lines[i].to_owned(),
            });
            i += 1;
        } else {
            edits.push(Edit::Insert {
                new_index: j,
                line: new_lines[j].to_owned(),
            });
            j += 1;
        }
    }

    while i < n {
        edits.push(Edit::Delete {
            old_index: i,
            line: old_lines[i].to_owned(),
        });
        i += 1;
    }
    while j < m {
        edits.push(Edit::Insert {
            new_index: j,
            line: new_lines[j].to_owned(),
        });
        j += 1;
    }

    if !edits.iter().any(|edit| !matches!(edit, Edit::Keep { .. })) {
        return Ok(String::new());
    }

    let mut groups = Vec::new();
    let mut row = 0;
    while row < edits.len() {
        if matches!(edits[row], Edit::Keep { .. }) {
            row += 1;
            continue;
        }
        let start = row;
        while row < edits.len() && !matches!(edits[row], Edit::Keep { .. }) {
            row += 1;
        }
        groups.push((start, row));
    }

    let mut hunks = Vec::new();
    let merge_gap = context * 2;
    let mut group_index = 0;
    while group_index < groups.len() {
        let mut start = groups[group_index].0;
        let mut end = groups[group_index].1;
        group_index += 1;

        while group_index < groups.len() {
            let gap = groups[group_index].0 - end;
            if gap > merge_gap {
                break;
            }
            end = groups[group_index].1;
            group_index += 1;
        }

        let hunk_start = start.saturating_sub(context);
        let hunk_end = (end + context).min(edits.len());

        // Extend context only to the nearest Keep rows; group boundaries are changed rows.
        start = hunk_start;
        let _ = start;
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