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
        return Vec::new();
    }
    text.strip_suffix('\n')
        .unwrap_or(text)
        .split('\n')
        .collect()
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

    let is_change = |edit: &Edit| !matches!(edit, Edit::Keep { .. });
    if !edits.iter().any(is_change) {
        return Ok(String::new());
    }

    let mut groups = Vec::<(usize, usize)>::new();
    let mut pos = 0;
    while pos < edits.len() {
        if is_change(&edits[pos]) {
            let start = pos;
            while pos < edits.len() && is_change(&edits[pos]) {
                pos += 1;
            }
            groups.push((start, pos));
        } else {
            pos += 1;
        }
    }

    let merge_distance = 2 * context;
    let mut merged = Vec::<(usize, usize)>::new();
    for group in groups {
        if let Some(last) = merged.last_mut() {
            let keeps_between = group.0 - last.1;
            if keeps_between <= merge_distance {
                last.1 = group.1;
                continue;
            }
        }
        merged.push(group);
    }

    let mut output = String::from("--- before\n+++ after\n");
    for (group_start, group_end) in merged {
        let start = group_start.saturating_sub(context);
        let end = (group_end + context).min(edits.len());

        let old_count = edits[start..end]
            .iter()
            .filter(|edit| matches!(edit, Edit::Keep { .. } | Edit::Delete { .. }))
            .count();
        let new_count = edits[start..end]
            .iter()
            .filter(|edit| matches!(edit, Edit::Keep { .. } | Edit::Insert { .. }))
            .count();

        let old_consumed = edits[..start]
            .iter()
            .filter(|edit| matches!(edit, Edit::Keep { .. } | Edit::Delete { .. }))
            .count();
        let new_consumed = edits[..start]
            .iter()
            .filter(|edit| matches!(edit, Edit::Keep { .. } | Edit::Insert { .. }))
            .count();

        let old_start = if old_count > 0 {
            old_consumed + 1
        } else {
            old_consumed
        };
        let new_start = if new_count > 0 {
            new_consumed + 1
        } else {
            new_consumed
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