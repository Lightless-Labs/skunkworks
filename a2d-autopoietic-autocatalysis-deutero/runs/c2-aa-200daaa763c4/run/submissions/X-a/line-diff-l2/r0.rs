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
        text.split_terminator('\n').collect()
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

    let m = old_lines.len();
    let n = new_lines.len();
    let mut lcs = vec![vec![0usize; n + 1]; m + 1];

    for i in (0..m).rev() {
        for j in (0..n).rev() {
            lcs[i][j] = if old_lines[i] == new_lines[j] {
                1 + lcs[i + 1][j + 1]
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }

    let mut edits = Vec::with_capacity(m + n);
    let (mut i, mut j) = (0, 0);

    while i < m && j < n {
        if old_lines[i] == new_lines[j] {
            edits.push(Edit::Keep {
                old_index: i,
                new_index: j,
                line: old_lines[i].to_owned(),
            });
            i += 1;
            j += 1;
        } else {
            let delete_score = lcs[i + 1][j];
            let insert_score = lcs[i][j + 1];
            if delete_score > insert_score
                || (delete_score == insert_score && (i + j) % 2 == 0)
            {
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
    }

    while i < m {
        edits.push(Edit::Delete {
            old_index: i,
            line: old_lines[i].to_owned(),
        });
        i += 1;
    }
    while j < n {
        edits.push(Edit::Insert {
            new_index: j,
            line: new_lines[j].to_owned(),
        });
        j += 1;
    }

    let is_change = |edit: &Edit| !matches!(edit, Edit::Keep { .. });
    if !edits.iter().any(is_change) {
        return Ok(String::new());
    }

    let mut groups: Vec<(usize, usize)> = Vec::new();
    let mut k = 0;
    while k < edits.len() {
        if is_change(&edits[k]) {
            let start = k;
            while k + 1 < edits.len() && is_change(&edits[k + 1]) {
                k += 1;
            }
            groups.push((start, k + 1));
        }
        k += 1;
    }

    let merge_distance = 2 * context;
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for group in groups {
        if let Some(last) = merged.last_mut() {
            let between_keeps = group.0 - last.1;
            if between_keeps <= merge_distance {
                last.1 = group.1;
                continue;
            }
        }
        merged.push(group);
    }

    let mut output = String::from("--- before\n+++ after\n");

    for &(group_start, group_end) in &merged {
        let start = group_start.saturating_sub(context);
        let end = (group_end + context).min(edits.len());

        let mut old_before = 0usize;
        let mut new_before = 0usize;
        for edit in &edits[..start] {
            match edit {
                Edit::Keep { .. } | Edit::Delete { .. } => old_before += 1,
                Edit::Insert { .. } => {}
            }
            match edit {
                Edit::Keep { .. } | Edit::Insert { .. } => new_before += 1,
                Edit::Delete { .. } => {}
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
            "@@ -{},{} +{},{} @@\n",
            old_start, old_count, new_start, new_count
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