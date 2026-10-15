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
    text.as_bytes().iter().position(|&b| b == b'\r')
}

fn build_script(old: &[&str], new: &[&str]) -> Vec<Edit> {
    let n = old.len();
    let m = new.len();
    let mut lcs = vec![vec![0usize; m + 1]; n + 1];

    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if old[i] == new[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }

    let mut edits = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
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

    while i < n {
        edits.push(Edit::Delete {
            old_index: i,
            line: old[i].to_owned(),
        });
        i += 1;
    }
    while j < m {
        edits.push(Edit::Insert {
            new_index: j,
            line: new[j].to_owned(),
        });
        j += 1;
    }

    edits
}

fn is_keep(edit: &Edit) -> bool {
    matches!(edit, Edit::Keep { .. })
}

fn row_line(edit: &Edit) -> &str {
    match edit {
        Edit::Keep { line, .. } | Edit::Delete { line, .. } | Edit::Insert { line, .. } => line,
    }
}

fn unified_diff(old: &str, new: &str, context: usize) -> Result<String, DiffError> {
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
    let changes: Vec<usize> = edits
        .iter()
        .enumerate()
        .filter_map(|(idx, edit)| (!is_keep(edit)).then_some(idx))
        .collect();

    if changes.is_empty() {
        return Ok(String::new());
    }

    let mut groups: Vec<(usize, usize)> = Vec::new();
    let mut start = changes[0];
    let mut end = start;
    for &idx in changes.iter().skip(1) {
        let keeps_between = edits[end + 1..idx].iter().filter(|e| is_keep(e)).count();
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
        let mut selected_start = group_start;
        let mut before = 0;
        while selected_start > 0 && before < context && is_keep(&edits[selected_start - 1]) {
            selected_start -= 1;
            before += 1;
        }

        let mut selected_end = group_end;
        let mut after = 0;
        while selected_end + 1 < edits.len()
            && after < context
            && is_keep(&edits[selected_end + 1])
        {
            selected_end += 1;
            after += 1;
        }

        let mut old_count = 0usize;
        let mut new_count = 0usize;
        let mut old_before = 0usize;
        let mut new_before = 0usize;
        for edit in &edits[..selected_start] {
            match edit {
                Edit::Keep { .. } => {
                    old_before += 1;
                    new_before += 1;
                }
                Edit::Delete { .. } => old_before += 1,
                Edit::Insert { .. } => new_before += 1,
            }
        }
        for edit in &edits[selected_start..=selected_end] {
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
        out.push_str(&format!(
            "@@ -{old_start},{old_count} +{new_start},{new_count} @@\n"
        ));

        for edit in &edits[selected_start..=selected_end] {
            let (prefix, line) = match edit {
                Edit::Keep { line, .. } => (' ', line.as_str()),
                Edit::Delete { line, .. } => ('-', line.as_str()),
                Edit::Insert { line, .. } => ('+', line.as_str()),
            };
            out.push(prefix);
            out.push_str(line);
            out.push('\n');
        }
    }

    Ok(out)
}

fn main() {}