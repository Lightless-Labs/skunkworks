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
    id: usize,
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

fn split_lines(input: &str) -> Vec<String> {
    if input.is_empty() {
        return Vec::new();
    }
    input[..input.len() - 1]
        .split('\n')
        .map(str::to_owned)
        .collect()
}

fn validate_cr(input: &str, side: MergeSide) -> Result<(), MergeError> {
    if let Some(byte) = input.as_bytes().iter().position(|&b| b == b'\r') {
        return Err(MergeError::CarriageReturn { side, byte });
    }
    Ok(())
}

fn canonical_changes(old: &[String], new: &[String], side: MergeSide) -> Vec<Change> {
    let n = old.len();
    let m = new.len();
    let mut lcs = vec![vec![0usize; m + 1]; n + 1];

    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if old[i] == new[j] {
                1 + lcs[i + 1][j + 1]
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }

    let mut ops = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if old[i] == new[j] {
            ops.push(Op::Keep);
            i += 1;
            j += 1;
        } else {
            let delete = lcs[i + 1][j];
            let insert = lcs[i][j + 1];
            if delete > insert || (delete == insert && (i + j) % 2 == 0) {
                ops.push(Op::Delete);
                i += 1;
            } else {
                ops.push(Op::Insert);
                j += 1;
            }
        }
    }
    while i < n {
        ops.push(Op::Delete);
        i += 1;
    }
    while j < m {
        ops.push(Op::Insert);
        j += 1;
    }

    let mut changes = Vec::new();
    let (mut old_i, mut new_i) = (0, 0);
    let mut k = 0;
    while k < ops.len() {
        if matches!(ops[k], Op::Keep) {
            old_i += 1;
            new_i += 1;
            k += 1;
            continue;
        }

        let start = old_i;
        let mut replacement = Vec::new();
        while k < ops.len() && !matches!(ops[k], Op::Keep) {
            match ops[k] {
                Op::Delete => old_i += 1,
                Op::Insert => {
                    replacement.push(new[new_i].clone());
                    new_i += 1;
                }
                Op::Keep => unreachable!(),
            }
            k += 1;
        }
        changes.push(Change {
            start,
            end: old_i,
            replacement,
            side: side.clone(),
            id: changes.len(),
        });
    }

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

fn components(changes: &[Change]) -> Vec<Component> {
    let mut seen = vec![false; changes.len()];
    let mut result = Vec::new();

    for seed in 0..changes.len() {
        if seen[seed] {
            continue;
        }
        seen[seed] = true;
        let mut stack = vec![seed];
        let mut members = Vec::new();

        while let Some(i) = stack.pop() {
            members.push(changes[i].clone());
            for j in 0..changes.len() {
                if !seen[j] && interacts(&changes[i], &changes[j]) {
                    seen[j] = true;
                    stack.push(j);
                }
            }
        }

        let start = members.iter().map(|c| c.start).min().unwrap();
        let end = members.iter().map(|c| c.end).max().unwrap();
        members.sort_by_key(|c| (c.start, c.end, match c.side {
            MergeSide::Base => 0,
            MergeSide::Left => 1,
            MergeSide::Right => 2,
        }, c.id));
        result.push(Component {
            start,
            end,
            changes: members,
        });
    }

    result.sort_by_key(|c| (c.start, usize::from(c.start != c.end), c.end));
    result
}

fn projection(base: &[String], component: &Component, side: MergeSide) -> Vec<String> {
    let mut changes: Vec<&Change> = component
        .changes
        .iter()
        .filter(|c| c.side == side)
        .collect();
    changes.sort_by_key(|c| (c.start, c.end, c.id));

    let mut out = Vec::new();
    let mut cursor = component.start;
    for change in changes {
        if change.start > cursor {
            out.extend(base[cursor..change.start].iter().cloned());
        }
        out.extend(change.replacement.iter().cloned());
        cursor = change.end;
    }
    if cursor < component.end {
        out.extend(base[cursor..component.end].iter().cloned());
    }
    out
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

    for (input, side) in [
        (base, MergeSide::Base),
        (left, MergeSide::Left),
        (right, MergeSide::Right),
    ] {
        if !input.is_empty() && !input.ends_with('\n') {
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

    let mut changes = canonical_changes(&base_lines, &left_lines, MergeSide::Left);
    changes.extend(canonical_changes(&base_lines, &right_lines, MergeSide::Right));
    let mut comps = components(&changes);

    let mut output = Vec::<String>::new();
    let mut cursor = 0usize;
    let mut conflicts = Vec::new();

    for component in comps.drain(..) {
        if cursor < component.start {
            output.extend(base_lines[cursor..component.start].iter().cloned());
        }

        let base_projection = base_lines[component.start..component.end].to_vec();
        let left_projection = projection(&base_lines, &component, MergeSide::Left);
        let right_projection = projection(&base_lines, &component, MergeSide::Right);

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
                    output.push(format!("<<<<<<< LEFT {ordinal}"));
                    output.extend(left_projection.iter().cloned());
                    let base_start_label = if component.end > component.start {
                        component.start + 1
                    } else {
                        component.start
                    };
                    output.push(format!(
                        "||||||| BASE {base_start_label},{}",
                        component.end - component.start
                    ));
                    output.extend(base_projection.iter().cloned());
                    output.push("=======".to_owned());
                    output.extend(right_projection.iter().cloned());
                    output.push(format!(">>>>>>> RIGHT {ordinal}"));
                    let output_len = output.len() - output_start;
                    conflicts.push(Conflict {
                        ordinal,
                        base_start: component.start,
                        base_len: component.end - component.start,
                        output_start,
                        output_len,
                    });
                }
            }
        }

        cursor = component.end;
    }

    if cursor < base_lines.len() {
        output.extend(base_lines[cursor..].iter().cloned());
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