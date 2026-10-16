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
        text.strip_suffix('\n')
            .unwrap_or(text)
            .split('\n')
            .collect()
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

    let mut edits = Vec::new();
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
        } else if lcs[i + 1][j] > lcs[i][j + 1]
            || (lcs[i + 1][j] == lcs[i][j + 1] && (i + j) % 2 == 0)
        {
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

    let is_change = |edit: &Edit| !matches!(edit, Edit::Keep { .. });
    let mut groups = Vec::new();
    let mut row = 0;
    while row < edits.len() {
        if !is_change(&edits[row]) {
            row += 1;
            continue;
        }
        let start = row;
        while row < edits.len() && is_change(&edits[row]) {
            row += 1;
        }
        groups.push((start, row));
    }

    if groups.is_empty() {
        return Ok(String::new());
    }

    let merge_gap = 2 * context;
    let mut merged = Vec::new();
    for group in groups {
        if let Some(last) = merged.last_mut() {
            let last: &mut (usize, usize) = last;
            let gap = group.0 - last.1;
            if gap <= merge_gap {
                last.1 = group.1;
                continue;
            }
        }
        merged.push(group);
    }

    let mut output = String::from("--- before\n+++ after\n");
    for (group_start, group_end) in merged {
        let mut start = group_start;
        let mut preceding = 0;
        while start > 0 && preceding < context {
            if is_change(&edits[start - 1]) {
                break;
            }
            start -= 1;
            preceding += 1;
        }

        let mut end = group_end;
        let mut following = 0;
        while end < edits.len() && following < context {
            if is_change(&edits[end]) {
                break;
            }
            end += 1;
            following += 1;
        }

        let mut old_count = 0usize;
        let mut new_count = 0usize;
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