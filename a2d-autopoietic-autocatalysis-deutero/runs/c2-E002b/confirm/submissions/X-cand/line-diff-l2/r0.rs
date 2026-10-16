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

    let rows = old_lines.len() + 1;
    let cols = new_lines.len() + 1;
    let mut lcs = vec![vec![0usize; cols]; rows];

    for i in (0..old_lines.len()).rev() {
        for j in (0..new_lines.len()).rev() {
            lcs[i][j] = if old_lines[i] == new_lines[j] {
                1 + lcs[i + 1][j + 1]
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }

    let mut edits = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);

    while i < old_lines.len() && j < new_lines.len() {
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
            if delete_score > insert_score || (delete_score == insert_score && (i + j) % 2 == 0) {
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

    while i < old_lines.len() {
        edits.push(Edit::Delete {
            old_index: i,
            line: old_lines[i].to_owned(),
        });
        i += 1;
    }

    while j < new_lines.len() {
        edits.push(Edit::Insert {
            new_index: j,
            line: new_lines[j].to_owned(),
        });
        j += 1;
    }

    let has_changes = edits
        .iter()
        .any(|edit| !matches!(edit, Edit::Keep { .. }));
    if !has_changes {
        return Ok(String::new());
    }

    let mut groups = Vec::<(usize, usize)>::new();
    let mut pos = 0usize;
    while pos < edits.len() {
        if matches!(edits[pos], Edit::Keep { .. }) {
            pos += 1;
            continue;
        }

        let start = pos;
        while pos < edits.len() && !matches!(edits[pos], Edit::Keep { .. }) {
            pos += 1;
        }
        groups.push((start, pos));
    }

    let merge_gap = context * 2;
    let mut merged = Vec::<(usize, usize)>::new();
    for group in groups {
        if let Some(last) = merged.last_mut() {
            let keep_rows = group.0 - last.1;
            if keep_rows <= merge_gap {
                last.1 = group.1;
                continue;
            }
        }
        merged.push(group);
    }

    let mut hunks = Vec::<(usize, usize)>::new();
    for (start, end) in merged {
        let mut hunk_start = start;
        let mut hunk_end = end;

        let mut before = context;
        while hunk_start > 0
            && before > 0
            && matches!(edits[hunk_start - 1], Edit::Keep { .. })
        {
            hunk_start -= 1;
            before -= 1;
        }

        let mut after = context;
        while hunk_end < edits.len()
            && after > 0
            && matches!(edits[hunk_end], Edit::Keep { .. })
        {
            hunk_end += 1;
            after -= 1;
        }

        if let Some(last) = hunks.last_mut() {
            if hunk_start <= last.1 {
                last.1 = last.1.max(hunk_end);
                continue;
            }
        }
        hunks.push((hunk_start, hunk_end));
    }

    let mut output = String::from("--- before\n+++ after\n");
    for (start, end) in hunks {
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