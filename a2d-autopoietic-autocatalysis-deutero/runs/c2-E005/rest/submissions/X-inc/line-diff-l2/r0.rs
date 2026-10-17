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
    text[..text.len() - 1].split('\n').collect()
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

    let changed: Vec<bool> = edits
        .iter()
        .map(|edit| !matches!(edit, Edit::Keep { .. }))
        .collect();
    if !changed.iter().any(|&value| value) {
        return Ok(String::new());
    }

    let mut groups = Vec::new();
    let mut pos = 0;
    while pos < edits.len() {
        if !changed[pos] {
            pos += 1;
            continue;
        }
        let start = pos;
        while pos + 1 < edits.len() && changed[pos + 1] {
            pos += 1;
        }
        groups.push((start, pos));
        pos += 1;
    }

    let merge_gap = 2 * context;
    let mut merged = Vec::new();
    let mut current = groups[0];
    for &(start, end) in groups.iter().skip(1) {
        let keep_gap = edits[current.1 + 1..start]
            .iter()
            .filter(|edit| matches!(edit, Edit::Keep { .. }))
            .count();
        if keep_gap <= merge_gap {
            current.1 = end;
        } else {
            merged.push(current);
            current = (start, end);
        }
    }
    merged.push(current);

    let mut output = String::from("--- before\n+++ after\n");
    let mut previous_end = None;

    for (group_start, group_end) in merged {
        let mut start = group_start;
        let mut preceding = 0;
        while start > 0 && preceding < context {
            if matches!(edits[start - 1], Edit::Keep { .. }) {
                preceding += 1;
            }
            start -= 1;
        }

        let mut end = group_end;
        let mut following = 0;
        while end + 1 < edits.len() && following < context {
            end += 1;
            if matches!(edits[end], Edit::Keep { .. }) {
                following += 1;
            }
        }

        if let Some(prev) = previous_end {
            if start <= prev {
                start = prev + 1;
            }
        }

        let old_count = edits[start..=end]
            .iter()
            .filter(|edit| matches!(edit, Edit::Keep { .. } | Edit::Delete { .. }))
            .count();
        let new_count = edits[start..=end]
            .iter()
            .filter(|edit| matches!(edit, Edit::Keep { .. } | Edit::Insert { .. }))
            .count();

        let old_before = edits[..start]
            .iter()
            .filter(|edit| matches!(edit, Edit::Keep { .. } | Edit::Delete { .. }))
            .count();
        let new_before = edits[..start]
            .iter()
            .filter(|edit| matches!(edit, Edit::Keep { .. } | Edit::Insert { .. }))
            .count();

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

        for edit in &edits[start..=end] {
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
        previous_end = Some(end);
    }

    Ok(output)
}