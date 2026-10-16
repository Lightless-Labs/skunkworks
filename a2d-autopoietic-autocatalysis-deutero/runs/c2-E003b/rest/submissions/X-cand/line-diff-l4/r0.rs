#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeSide {
    Base,
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    Mark,
    PreferLeft,
    PreferRight,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeError {
    CarriageReturn { side: MergeSide, byte: usize },
    MissingFinalLf { side: MergeSide },
    TooLarge {
        base_len: usize,
        left_len: usize,
        right_len: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub ordinal: usize,
    pub base_start: usize,
    pub base_len: usize,
    pub output_start: usize,
    pub output_len: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeResult {
    pub text: String,
    pub conflicts: usize,
    pub regions: Vec<Conflict>,
}

#[derive(Debug, Clone)]
struct Change {
    start: usize,
    end: usize,
    replacement: Vec<String>,
    side: MergeSide,
}

#[derive(Debug, Clone)]
struct Component {
    start: usize,
    end: usize,
    changes: Vec<Change>,
}

fn split_lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    text.strip_suffix('\n')
        .unwrap()
        .split('\n')
        .map(str::to_owned)
        .collect()
}

fn validate_cr(text: &str, side: MergeSide) -> Result<(), MergeError> {
    if let Some(byte) = text.as_bytes().iter().position(|&b| b == b'\r') {
        return Err(MergeError::CarriageReturn { side, byte });
    }
    Ok(())
}

fn lcs_table(old: &[String], new: &[String]) -> Vec<Vec<usize>> {
    let mut lcs = vec![vec![0; new.len() + 1]; old.len() + 1];
    for i in (0..old.len()).rev() {
        for j in (0..new.len()).rev() {
            lcs[i][j] = if old[i] == new[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    lcs
}

fn diff(old: &[String], new: &[String], side: MergeSide) -> Vec<Change> {
    let lcs = lcs_table(old, new);
    let (mut i, mut j, mut base_pos) = (0, 0, 0);
    let mut changes = Vec::new();
    let mut active_start: Option<usize> = None;
    let mut deletes = 0;
    let mut replacement = Vec::new();

    let flush = |changes: &mut Vec<Change>,
                 active_start: &mut Option<usize>,
                 deletes: &mut usize,
                 replacement: &mut Vec<String>| {
        if let Some(start) = active_start.take() {
            changes.push(Change {
                start,
                end: start + *deletes,
                replacement: std::mem::take(replacement),
                side: side.clone(),
            });
            *deletes = 0;
        }
    };

    while i < old.len() || j < new.len() {
        if i < old.len() && j < new.len() && old[i] == new[j] {
            flush(
                &mut changes,
                &mut active_start,
                &mut deletes,
                &mut replacement,
            );
            i += 1;
            j += 1;
            base_pos += 1;
        } else if i == old.len() {
            active_start.get_or_insert(base_pos);
            replacement.push(new[j].clone());
            j += 1;
        } else if j == new.len() {
            active_start.get_or_insert(base_pos);
            deletes += 1;
            i += 1;
            base_pos += 1;
        } else {
            let delete_score = lcs[i + 1][j];
            let insert_score = lcs[i][j + 1];
            let choose_delete = if delete_score > insert_score {
                true
            } else if insert_score > delete_score {
                false
            } else {
                (i + j) % 2 == 0
            };

            active_start.get_or_insert(base_pos);
            if choose_delete {
                deletes += 1;
                i += 1;
                base_pos += 1;
            } else {
                replacement.push(new[j].clone());
                j += 1;
            }
        }
    }

    flush(
        &mut changes,
        &mut active_start,
        &mut deletes,
        &mut replacement,
    );
    changes
}

fn interact(a: &Change, b: &Change) -> bool {
    let a_positive = a.start < a.end;
    let b_positive = b.start < b.end;

    match (a_positive, b_positive) {
        (true, true) => a.start.max(b.start) < a.end.min(b.end),
        (false, false) => a.start == b.start,
        (false, true) => b.start < a.start && a.start < b.end,
        (true, false) => a.start < b.start && b.start < a.end,
    }
}

fn components(changes: Vec<Change>) -> Vec<Component> {
    let n = changes.len();
    let mut seen = vec![false; n];
    let mut result = Vec::new();

    for root in 0..n {
        if seen[root] {
            continue;
        }
        seen[root] = true;
        let mut stack = vec![root];
        let mut members = Vec::new();

        while let Some(i) = stack.pop() {
            members.push(changes[i].clone());
            for j in 0..n {
                if !seen[j] && interact(&changes[i], &changes[j]) {
                    seen[j] = true;
                    stack.push(j);
                }
            }
        }

        let start = members.iter().map(|c| c.start).min().unwrap();
        let end = members.iter().map(|c| c.end).max().unwrap();
        members.sort_by_key(|c| (c.start, c.end));
        result.push(Component {
            start,
            end,
            changes: members,
        });
    }

    result.sort_by_key(|c| (c.start, usize::from(c.start < c.end), c.end));
    result
}

fn project(base: &[String], component: &Component, side: MergeSide) -> Vec<String> {
    let mut selected: Vec<&Change> = component
        .changes
        .iter()
        .filter(|c| c.side == side)
        .collect();
    selected.sort_by_key(|c| (c.start, c.end));

    let mut output = Vec::new();
    let mut cursor = component.start;

    for change in selected {
        if change.start > cursor {
            output.extend(base[cursor..change.start].iter().cloned());
        }
        output.extend(change.replacement.iter().cloned());
        cursor = change.end;
    }

    if cursor < component.end {
        output.extend(base[cursor..component.end].iter().cloned());
    }
    output
}

fn push_line(output: &mut Vec<String>, line: impl Into<String>) {
    output.push(line.into());
}

pub fn merge_three_way(
    base: &str,
    left: &str,
    right: &str,
    resolution: Resolution,
) -> Result<MergeResult, MergeError> {
    validate_cr(base, MergeSide::Base)?;
    validate_cr(left, MergeSide::Left)?;
    validate_cr(right, MergeSide::Right)?;

    for (text, side) in [
        (base, MergeSide::Base),
        (left, MergeSide::Left),
        (right, MergeSide::Right),
    ] {
        if !text.is_empty() && !text.ends_with('\n') {
            return Err(MergeError::MissingFinalLf { side });
        }
    }

    let base_lines = split_lines(base);
    let left_lines = split_lines(left);
    let right_lines = split_lines(right);

    if base_lines.len() > 128 || left_lines.len() > 128 || right_lines.len() > 128 {
        return Err(MergeError::TooLarge {
            base_len: base_lines.len(),
            left_len: left_lines.len(),
            right_len: right_lines.len(),
        });
    }

    let mut changes = diff(&base_lines, &left_lines, MergeSide::Left);
    changes.extend(diff(&base_lines, &right_lines, MergeSide::Right));
    let components = components(changes);

    let mut output = Vec::new();
    let mut conflicts = Vec::new();
    let mut base_cursor = 0;

    for component in components {
        if base_cursor < component.start {
            output.extend(base_lines[base_cursor..component.start].iter().cloned());
        }

        let base_projection = base_lines[component.start..component.end].to_vec();
        let left_projection = project(&base_lines, &component, MergeSide::Left);
        let right_projection = project(&base_lines, &component, MergeSide::Right);

        if left_projection == right_projection {
            output.extend(left_projection);
        } else if left_projection == base_projection {
            output.extend(right_projection);
        } else if right_projection == base_projection {
            output.extend(left_projection);
        } else {
            match resolution {
                Resolution::PreferLeft => output.extend(left_projection),
                Resolution::PreferRight => output.extend(right_projection),
                Resolution::Mark => {
                    let ordinal = conflicts.len() + 1;
                    let output_start = output.len();

                    push_line(&mut output, format!("<<<<<<< LEFT {ordinal}"));
                    output.extend(left_projection.iter().cloned());

                    let base_len = component.end - component.start;
                    let base_display_start = if base_len > 0 {
                        component.start + 1
                    } else {
                        component.start
                    };
                    push_line(
                        &mut output,
                        format!("||||||| BASE {base_display_start},{base_len}"),
                    );
                    output.extend(base_projection.iter().cloned());
                    push_line(&mut output, "=======");
                    output.extend(right_projection.iter().cloned());
                    push_line(&mut output, format!(">>>>>>> RIGHT {ordinal}"));

                    conflicts.push(Conflict {
                        ordinal,
                        base_start: component.start,
                        base_len,
                        output_start,
                        output_len: output.len() - output_start,
                    });
                }
            }
        }
        base_cursor = component.end;
    }

    if base_cursor < base_lines.len() {
        output.extend(base_lines[base_cursor..].iter().cloned());
    }

    let mut text = String::new();
    for line in &output {
        text.push_str(line);
        text.push('\n');
    }

    Ok(MergeResult {
        text,
        conflicts: conflicts.len(),
        regions: conflicts,
    })
}