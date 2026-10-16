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
    let (n, m) = (old_lines.len(), new_lines.len());

    if n > 128 || m > 128 {
        return Err(DiffError::TooLarge {
            old_len: n,
            new_len: m,
        });
    }

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
        } else if lcs[i + 1][j] > lcs[i][j + 1]
            || (lcs[i + 1][j] == lcs[i][j + 1] && (i + j) % 2 == 0)
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

    let changed: Vec<usize> = edits
        .iter()
        .enumerate()
        .filter_map(|(idx, edit)| {
            if matches!(edit, Edit::Keep { .. }) {
                None
            } else {
                Some(idx)
            }
        })
        .collect();

    if changed.is_empty() {
        return Ok(String::new());
    }

    let mut groups: Vec<(usize, usize)> = Vec::new();
    let mut start = changed[0];
    let mut end = start;
    for &idx in changed.iter().skip(1) {
        let keeps_between = edits[end + 1..idx]
            .iter()
            .filter(|e| matches!(e, Edit::Keep { .. }))
            .count();
        if keeps_between <= 2 * context {
            end = idx;
        } else {
            groups.push((start, end));
            start = idx;
            end = idx;
        }
    }
    groups.push((start, end));

    let mut out = String::from("--- before\n+++ after\n");
    for (group_start, group_end) in groups {
        let mut hunk_start = group_start;
        let mut preceding = 0;
        while hunk_start > 0 && preceding < context {
            if matches!(edits[hunk_start - 1], Edit::Keep { .. }) {
                hunk_start -= 1;
                preceding += 1;
            } else {
                break;
            }
        }

        let mut hunk_end = group_end + 1;
        let mut following = 0;
        while hunk_end < edits.len() && following < context {
            if matches!(edits[hunk_end], Edit::Keep { .. }) {
                hunk_end += 1;
                following += 1;
            } else {
                break;
            }
        }

        let old_count = edits[hunk_start..hunk_end]
            .iter()
            .filter(|e| matches!(e, Edit::Keep { .. } | Edit::Delete { .. }))
            .count();
        let new_count = edits[hunk_start..hunk_end]
            .iter()
            .filter(|e| matches!(e, Edit::Keep { .. } | Edit::Insert { .. }))
            .count();

        let old_consumed = edits[..hunk_start]
            .iter()
            .filter(|e| matches!(e, Edit::Keep { .. } | Edit::Delete { .. }))
            .count();
        let new_consumed = edits[..hunk_start]
            .iter()
            .filter(|e| matches!(e, Edit::Keep { .. } | Edit::Insert { .. }))
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

        out.push_str(&format!(
            "@@ -{old_start},{old_count} +{new_start},{new_count} @@\n"
        ));
        for edit in &edits[hunk_start..hunk_end] {
            match edit {
                Edit::Keep { line, .. } => {
                    out.push(' ');
                    out.push_str(line);
                    out.push('\n');
                }
                Edit::Delete { line, .. } => {
                    out.push('-');
                    out.push_str(line);
                    out.push('\n');
                }
                Edit::Insert { line, .. } => {
                    out.push('+');
                    out.push_str(line);
                    out.push('\n');
                }
            }
        }
    }

    Ok(out)
}