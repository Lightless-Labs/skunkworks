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

    let text = text.strip_suffix('\n').unwrap_or(text);
    text.split('\n').collect()
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
    let (mut i, mut j) = (0, 0);

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
            let delete = if delete_score > insert_score {
                true
            } else if delete_score < insert_score {
                false
            } else {
                (i + j) % 2 == 0
            };

            if delete {
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

    let mut groups = Vec::new();
    let mut index = 0;

    while index < edits.len() {
        if matches!(edits[index], Edit::Keep { .. }) {
            index += 1;
            continue;
        }

        let start = index;
        while index < edits.len() && !matches!(edits[index], Edit::Keep { .. }) {
            index += 1;
        }
        groups.push((start, index));
    }

    let merge_distance = 2 * context;
    let mut merged = Vec::new();

    for group in groups {
        if let Some(last) = merged.last_mut() {
            let last: &mut (usize, usize) = last;
            let keep_count = edits[last.1..group.0]
                .iter()
                .filter(|edit| matches!(edit, Edit::Keep { .. }))
                .count();

            if keep_count <= merge_distance {
                last.1 = group.1;
                continue;
            }
        }
        merged.push(group);
    }

    let mut hunks = Vec::new();

    for (start, end) in merged {
        let hunk_start = start.saturating_sub(context);
        let hunk_end = (end + context).min(edits.len());

        let old_count = edits[hunk_start..hunk_end]
            .iter()
            .filter(|edit| matches!(edit, Edit::Keep { .. } | Edit::Delete { .. }))
            .count();
        let new_count = edits[hunk_start..hunk_end]
            .iter()
            .filter(|edit| matches!(edit, Edit::Keep { .. } | Edit::Insert { .. }))
            .count();

        let old_consumed = edits[..hunk_start]
            .iter()
            .filter(|edit| matches!(edit, Edit::Keep { .. } | Edit::Delete { .. }))
            .count();
        let new_consumed = edits[..hunk_start]
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

        hunks.push((hunk_start, hunk_end, old_start, old_count, new_start, new_count));
    }

    let mut output = String::from("--- before\n+++ after\n");

    for (start, end, old_start, old_count, new_start, new_count) in hunks {
        output.push_str(&format!(
            "@@ -{old_start},{old_count} +{new_start},{new_count} @@\n"
        ));

        for edit in &edits[start..end] {
            match edit {
                Edit::Keep { line, .. } => {
                    output.push(' ');
                    output.push_str(line);
                }
                Edit::Delete { line, .. } => {
                    output.push('-');
                    output.push_str(line);
                }
                Edit::Insert { line, .. } => {
                    output.push('+');
                    output.push_str(line);
                }
            }
            output.push('\n');
        }
    }

    Ok(output)
}

fn main() {}