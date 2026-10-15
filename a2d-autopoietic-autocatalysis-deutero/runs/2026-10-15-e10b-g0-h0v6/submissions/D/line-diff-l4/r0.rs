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
    CarriageReturn {
        side: MergeSide,
        byte: usize,
    },
    MissingFinalLf {
        side: MergeSide,
    },
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

#[derive(Debug, Clone, Copy)]
enum Op {
    Keep,
    Delete,
    Insert,
}

fn validate_input(input: &str, side: MergeSide) -> Result<Vec<String>, MergeError> {
    if let Some(byte) = input.as_bytes().iter().position(|&b| b == b'\r') {
        return Err(MergeError::CarriageReturn { side, byte });
    }

    if !input.is_empty() && !input.ends_with('\n') {
        return Err(MergeError::MissingFinalLf { side });
    }

    if input.is_empty() {
        return Ok(Vec::new());
    }

    Ok(input[..input.len() - 1]
        .split('\n')
        .map(str::to_owned)
        .collect())
}

fn canonical_changes(old: &[String], new: &[String], side: MergeSide) -> Vec<Change> {
    let m = old.len();
    let n = new.len();
    let width = n + 1;
    let mut lcs = vec![0usize; (m + 1) * (n + 1)];

    for i in (0..m).rev() {
        for j in (0..n).rev() {
            lcs[i * width + j] = if old[i] == new[j] {
                1 + lcs[(i + 1) * width + j + 1]
            } else {
                lcs[(i + 1) * width + j].max(lcs[i * width + j + 1])
            };
        }
    }

    let mut i = 0;
    let mut j = 0;
    let mut base_consumed = 0;
    let mut in_change = false;
    let mut change_start = 0;
    let mut deletes = 0;
    let mut replacement = Vec::new();
    let mut changes = Vec::new();

    let finish_change = |changes: &mut Vec<Change>,
                         in_change: &mut bool,
                         change_start: usize,
                         deletes: &mut usize,
                         replacement: &mut Vec<String>| {
        if *in_change {
            changes.push(Change {
                start: change_start,
                end: change_start + *deletes,
                replacement: std::mem::take(replacement),
                side: side.clone(),
            });
            *deletes = 0;
            *in_change = false;
        }
    };

    while i < m || j < n {
        if i < m && j < n && old[i] == new[j] {
            finish_change(
                &mut changes,
                &mut in_change,
                change_start,
                &mut deletes,
                &mut replacement,
            );
            i += 1;
            j += 1;
            base_consumed += 1;
            continue;
        }

        if i == m {
            if !in_change {
                in_change = true;
                change_start = base_consumed;
            }
            replacement.push(new[j].clone());
            j += 1;
            continue;
        }

        if j == n {
            if !in_change {
                in_change = true;
                change_start = base_consumed;
            }
            i += 1;
            base_consumed += 1;
            deletes += 1;
            continue;
        }

        let delete_len = lcs[(i + 1) * width + j];
        let insert_len = lcs[i * width + j + 1];
        let choose_delete = if delete_len > insert_len {
            true
        } else if insert_len > delete_len {
            false
        } else {
            (i + j) % 2 == 0
        };

        if !in_change {
            in_change = true;
            change_start = base_consumed;
        }

        if choose_delete {
            i += 1;
            base_consumed += 1;
            deletes += 1;
        } else {
            replacement.push(new[j].clone());
            j += 1;
        }
    }

    finish_change(
        &mut changes,
        &mut in_change,
        change_start,
        &mut deletes,
        &mut replacement,
    );

    changes
}

fn interacts(a: &Change, b: &Change) -> bool {
    let a_zero = a.start == a.end;
    let b_zero = b.start == b.end;

    match (a_zero, b_zero) {
        (false, false) => a.start.max(b.start) < a.end.min(b.end),
        (true, true) => a.start == b.start,
        (true, false) => b.start < a.start && a.start < b.end,
        (false, true) => a.start < b.start && b.start < a.end,
    }
}

fn build_components(changes: Vec<Change>) -> Vec<Component> {
    let count = changes.len();
    let mut visited = vec![false; count];
    let mut components = Vec::new();

    for root in 0..count {
        if visited[root] {
            continue;
        }

        visited[root] = true;
        let mut stack = vec![root];
        let mut members = Vec::new();

        while let Some(index) = stack.pop() {
            members.push(changes[index].clone());
            for other in 0..count {
                if !visited[other] && interacts(&changes[index], &changes[other]) {
                    visited[other] = true;
                    stack.push(other);
                }
            }
        }

        let start = members.iter().map(|c| c.start).min().unwrap_or(0);
        let end = members.iter().map(|c| c.end).max().unwrap_or(start);
        members.sort_by_key(|c| (c.start, c.end));
        components.push(Component {
            start,
            end,
            changes: members,
        });
    }

    components.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then_with(|| (a.start != a.end).cmp(&(b.start != b.end)))
            .then_with(|| a.end.cmp(&b.end))
    });
    components
}

fn project(
    base: &[String],
    component: &Component,
    side: MergeSide,
) -> Vec<String> {
    let mut selected: Vec<&Change> = component
        .changes
        .iter()
        .filter(|change| change.side == side)
        .collect();
    selected.sort_by_key(|change| (change.start, change.end));

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

fn append_lines(output: &mut Vec<String>, lines: &[String]) {
    output.extend(lines.iter().cloned());
}

pub fn merge_three_way(
    base: &str,
    left: &str,
    right: &str,
    resolution: Resolution,
) -> Result<MergeResult, MergeError> {
    let base_lines = validate_input(base, MergeSide::Base)?;
    let left_lines = validate_input(left, MergeSide::Left)?;
    let right_lines = validate_input(right, MergeSide::Right)?;

    if base_lines.len() > 128 || left_lines.len() > 128 || right_lines.len() > 128 {
        return Err(MergeError::TooLarge {
            base_len: base_lines.len(),
            left_len: left_lines.len(),
            right_len: right_lines.len(),
        });
    }

    let mut changes = canonical_changes(&base_lines, &left_lines, MergeSide::Left);
    changes.extend(canonical_changes(
        &base_lines,
        &right_lines,
        MergeSide::Right,
    ));
    let components = build_components(changes);

    let mut output = Vec::new();
    let mut records = Vec::new();
    let mut base_cursor = 0;

    for component in components {
        if base_cursor < component.start {
            append_lines(&mut output, &base_lines[base_cursor..component.start]);
        }

        let base_projection = base_lines[component.start..component.end].to_vec();
        let left_projection = project(&base_lines, &component, MergeSide::Left);
        let right_projection = project(&base_lines, &component, MergeSide::Right);

        if left_projection == right_projection {
            append_lines(&mut output, &left_projection);
        } else if left_projection == base_projection {
            append_lines(&mut output, &right_projection);
        } else if right_projection == base_projection {
            append_lines(&mut output, &left_projection);
        } else {
            match resolution {
                Resolution::PreferLeft => append_lines(&mut output, &left_projection),
                Resolution::PreferRight => append_lines(&mut output, &right_projection),
                Resolution::Mark => {
                    let ordinal = records.len() + 1;
                    let output_start = output.len();
                    output.push(format!("<<<<<<< LEFT {}", ordinal));
                    append_lines(&mut output, &left_projection);
                    let region_len = component.end - component.start;
                    let base_display_start = if region_len > 0 {
                        component.start + 1
                    } else {
                        component.start
                    };
                    output.push(format!(
                        "||||||| BASE {},{}",
                        base_display_start, region_len
                    ));
                    append_lines(&mut output, &base_projection);
                    output.push("=======".to_owned());
                    append_lines(&mut output, &right_projection);
                    output.push(format!(">>>>>>> RIGHT {}", ordinal));
                    records.push(Conflict {
                        ordinal,
                        base_start: component.start,
                        base_len: region_len,
                        output_start,
                        output_len: 4
                            + left_projection.len()
                            + base_projection.len()
                            + right_projection.len(),
                    });
                }
            }
        }

        base_cursor = component.end;
    }

    if base_cursor < base_lines.len() {
        append_lines(&mut output, &base_lines[base_cursor..]);
    }

    let text = if output.is_empty() {
        String::new()
    } else {
        let mut text = output.join("\n");
        text.push('\n');
        text
    };

    Ok(MergeResult {
        text,
        conflicts: records.len(),
        regions: records,
    })
}