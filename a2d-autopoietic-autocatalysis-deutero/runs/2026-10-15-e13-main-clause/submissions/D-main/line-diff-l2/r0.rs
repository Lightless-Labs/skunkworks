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

fn first_cr(text: &str) -> Option<usize> {
    text.as_bytes().iter().position(|&byte| byte == b'\r')
}

fn build_script(old: &[&str], new: &[&str]) -> Vec<Edit> {
    let rows = old.len() + 1;
    let cols = new.len() + 1;
    let mut lcs = vec![vec![0usize; cols]; rows];

    for i in (0..old.len()).rev() {
        for j in (0..new.len()).rev() {
            lcs[i][j] = if old[i] == new[j] {
                1 + lcs[i + 1][j + 1]
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }

    let mut edits = Vec::new();
    let (mut i, mut j) = (0, 0);

    while i < old.len() && j < new.len() {
        if old[i] == new[j] {
            edits.push(Edit::Keep {
                old_index: i,
                new_index: j,
                line: old[i].to_owned(),
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
                    line: old[i].to_owned(),
                });
                i += 1;
            } else {
                edits.push(Edit::Insert {
                    new_index: j,
                    line: new[j].to_owned(),
                });
                j += 1;
            }
        }
    }

    while i < old.len() {
        edits.push(Edit::Delete {
            old_index: i,
            line: old[i].to_owned(),
        });
        i += 1;
    }
    while j < new.len() {
        edits.push(Edit::Insert {
            new_index: j,
            line: new[j].to_owned(),
        });
        j += 1;
    }

    edits
}

fn is_change(edit: &Edit) -> bool {
    !matches!(edit, Edit::Keep { .. })
}

fn consumed_counts(edits: &[Edit]) -> (usize, usize) {
    let mut old_count = 0;
    let mut new_count = 0;
    for edit in edits {
        match edit {
            Edit::Keep { .. } => {
                old_count += 1;
                new_count += 1;
            }
            Edit::Delete { .. } => old_count += 1,
            Edit::Insert { .. } => new_count += 1,
        }
    }
    (old_count, new_count)
}

fn render_hunk(output: &mut String, edits: &[Edit]) {
    let (old_count, new_count) = consumed_counts(edits);
    let old_before = edits
        .first()
        .map(|edit| match edit {
            Edit::Keep { old_index, .. } | Edit::Delete { old_index, .. } => *old_index,
            Edit::Insert { .. } => {
                // The first edit may be an insertion; derive the position from prior rows
                // in the caller's complete script instead.
                0
            }
        })
        .unwrap_or(0);
    let new_before = edits
        .first()
        .map(|edit| match edit {
            Edit::Keep { new_index, .. } | Edit::Insert { new_index, .. } => *new_index,
            Edit::Delete { .. } => 0,
        })
        .unwrap_or(0);

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

    for edit in edits {
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

pub fn unified_diff(old: &str, new: &str, context: usize) -> Result<String, DiffError> {
    if context > 8 {
        return Err(DiffError::ContextTooLarge { context });
    }
    if let Some(byte) = first_cr(old) {
        return Err(DiffError::CarriageReturn {
            side: Side::Old,
            byte,
        });
    }
    if let Some(byte) = first_cr(new) {
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

    let edits = build_script(&old_lines, &new_lines);
    if !edits.iter().any(is_change) {
        return Ok(String::new());
    }

    let mut groups = Vec::new();
    let mut cursor = 0;
    while cursor < edits.len() {
        if !is_change(&edits[cursor]) {
            cursor += 1;
            continue;
        }
        let start = cursor;
        while cursor < edits.len() && is_change(&edits[cursor]) {
            cursor += 1;
        }
        groups.push((start, cursor));
    }

    let merge_distance = 2 * context;
    let mut merged = Vec::new();
    for (start, end) in groups {
        if let Some(last) = merged.last_mut() {
            let (last_start, last_end): &mut (usize, usize) = last;
            let keeps_between = edits[*last_end..start]
                .iter()
                .filter(|edit| matches!(edit, Edit::Keep { .. }))
                .count();
            if keeps_between <= merge_distance {
                *last_end = end;
                continue;
            }
            let _ = last_start;
        }
        merged.push((start, end));
    }

    let mut output = String::from("--- before\n+++ after\n");
    for (start, end) in merged {
        let mut hunk_start = start;
        let mut hunk_end = end;

        let mut preceding = 0;
        while hunk_start > 0 && preceding < context {
            hunk_start -= 1;
            if matches!(edits[hunk_start], Edit::Keep { .. }) {
                preceding += 1;
            }
        }

        let mut following = 0;
        while hunk_end < edits.len() && following < context {
            if matches!(edits[hunk_end], Edit::Keep { .. }) {
                following += 1;
            }
            hunk_end += 1;
        }

        let old_before = edits[..hunk_start]
            .iter()
            .filter(|edit| !matches!(edit, Edit::Insert { .. }))
            .count();
        let new_before = edits[..hunk_start]
            .iter()
            .filter(|edit| !matches!(edit, Edit::Delete { .. }))
            .count();

        let hunk = &edits[hunk_start..hunk_end];
        let (old_count, new_count) = consumed_counts(hunk);
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
        for edit in hunk {
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