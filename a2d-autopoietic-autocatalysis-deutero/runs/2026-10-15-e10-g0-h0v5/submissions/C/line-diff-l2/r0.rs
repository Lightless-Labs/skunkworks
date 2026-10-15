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

fn first_cr(text: &str) -> Option<usize> {
    text.as_bytes().iter().position(|&byte| byte == b'\r')
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

fn validated_lines<'a>(text: &'a str, side: Side) -> Result<Vec<&'a str>, DiffError> {
    if let Some(byte) = first_cr(text) {
        return Err(DiffError::CarriageReturn { side, byte });
    }
    if !text.is_empty() && !text.ends_with('\n') {
        return Err(DiffError::MissingFinalLf { side });
    }
    Ok(split_lines(text))
}

pub fn unified_diff(old: &str, new: &str, context: usize) -> Result<String, DiffError> {
    if context > 8 {
        return Err(DiffError::ContextTooLarge { context });
    }

    let old_lines = validated_lines(old, Side::Old)?;
    let new_lines = validated_lines(new, Side::New)?;

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
    let (mut i, mut j) = (0usize, 0usize);
    while i < m && j < n {
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

    let changed_positions: Vec<usize> = edits
        .iter()
        .enumerate()
        .filter_map(|(index, edit)| {
            if matches!(edit, Edit::Keep { .. }) {
                None
            } else {
                Some(index)
            }
        })
        .collect();

    if changed_positions.is_empty() {
        return Ok(String::new());
    }

    let merge_gap = 2 * context;
    let mut groups: Vec<(usize, usize)> = Vec::new();
    let mut start = changed_positions[0];
    let mut end = start;

    for &position in changed_positions.iter().skip(1) {
        let keeps_between = position - end - 1;
        if keeps_between <= merge_gap {
            end = position;
        } else {
            groups.push((start, end));
            start = position;
            end = position;
        }
    }
    groups.push((start, end));

    let mut output = String::from("--- before\n+++ after\n");
    let mut selected_ranges = Vec::new();
    for (group_start, group_end) in groups {
        let mut begin = group_start;
        let mut before = context;
        while begin > 0 && before > 0 {
            if matches!(edits[begin - 1], Edit::Keep { .. }) {
                begin -= 1;
                before -= 1;
            } else {
                break;
            }
        }

        let mut finish = group_end;
        let mut after = context;
        while finish + 1 < edits.len() && after > 0 {
            if matches!(edits[finish + 1], Edit::Keep { .. }) {
                finish += 1;
                after -= 1;
            } else {
                break;
            }
        }
        selected_ranges.push((begin, finish));
    }

    for (begin, finish) in selected_ranges {
        let old_before = edits[..begin]
            .iter()
            .filter(|edit| !matches!(edit, Edit::Insert { .. }))
            .count();
        let new_before = edits[..begin]
            .iter()
            .filter(|edit| !matches!(edit, Edit::Delete { .. }))
            .count();
        let old_count = edits[begin..=finish]
            .iter()
            .filter(|edit| !matches!(edit, Edit::Insert { .. }))
            .count();
        let new_count = edits[begin..=finish]
            .iter()
            .filter(|edit| !matches!(edit, Edit::Delete { .. }))
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

        for edit in &edits[begin..=finish] {
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