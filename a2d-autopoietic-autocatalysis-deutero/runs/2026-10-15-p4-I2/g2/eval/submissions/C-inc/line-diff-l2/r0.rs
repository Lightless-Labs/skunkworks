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

    let without_final_lf = &text[..text.len() - 1];
    if without_final_lf.is_empty() {
        return vec![""];
    }

    without_final_lf.split('\n').collect()
}

fn validate_side(text: &str, side: Side) -> Result<(), DiffError> {
    if let Some(byte) = text.as_bytes().iter().position(|&b| b == b'\r') {
        return Err(DiffError::CarriageReturn { side, byte });
    }
    if !text.is_empty() && !text.ends_with('\n') {
        return Err(DiffError::MissingFinalLf { side });
    }
    Ok(())
}

pub fn unified_diff(old: &str, new: &str, context: usize) -> Result<String, DiffError> {
    if context > 8 {
        return Err(DiffError::ContextTooLarge { context });
    }

    validate_side(old, Side::Old)?;
    validate_side(new, Side::New)?;

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

    let mut edits = Vec::new();
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

    let changed: Vec<bool> = edits
        .iter()
        .map(|edit| !matches!(edit, Edit::Keep { .. }))
        .collect();

    if !changed.iter().any(|&is_changed| is_changed) {
        return Ok(String::new());
    }

    let mut groups = Vec::<(usize, usize)>::new();
    let mut row = 0;
    while row < edits.len() {
        if !changed[row] {
            row += 1;
            continue;
        }
        let start = row;
        while row + 1 < edits.len() && changed[row + 1] {
            row += 1;
        }
        groups.push((start, row));
        row += 1;
    }

    let merge_distance = 2 * context;
    let mut merged = Vec::<(usize, usize)>::new();
    for group in groups {
        if let Some(last) = merged.last_mut() {
            let keeps_between = group.0 - last.1 - 1;
            if keeps_between <= merge_distance {
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
            start -= 1;
            if matches!(edits[start], Edit::Keep { .. }) {
                preceding += 1;
            }
        }

        let mut end = group_end;
        let mut following = 0;
        while end + 1 < edits.len() && following < context {
            end += 1;
            if matches!(edits[end], Edit::Keep { .. }) {
                following += 1;
            }
        }

        let (mut old_count, mut new_count) = (0usize, 0usize);
        for edit in &edits[start..=end] {
            match edit {
                Edit::Keep { .. } => {
                    old_count += 1;
                    new_count += 1;
                }
                Edit::Delete { .. } => old_count += 1,
                Edit::Insert { .. } => new_count += 1,
            }
        }

        let old_before = match &edits[start] {
            Edit::Keep { old_index, .. } | Edit::Delete { old_index, .. } => *old_index,
            Edit::Insert { .. } => {
                edits[..start]
                    .iter()
                    .filter(|edit| !matches!(edit, Edit::Insert { .. }))
                    .count()
            }
        };
        let new_before = match &edits[start] {
            Edit::Keep { new_index, .. } | Edit::Insert { new_index, .. } => *new_index,
            Edit::Delete { .. } => {
                edits[..start]
                    .iter()
                    .filter(|edit| !matches!(edit, Edit::Delete { .. }))
                    .count()
            }
        };
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
    }

    Ok(output)
}

fn main() {}