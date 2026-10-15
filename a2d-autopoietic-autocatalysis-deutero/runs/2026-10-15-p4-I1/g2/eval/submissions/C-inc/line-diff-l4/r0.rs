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

#[derive(Clone)]
struct Change {
    start: usize,
    end: usize,
    replacement: Vec<String>,
    side: MergeSide,
}

#[derive(Clone)]
struct Component {
    start: usize,
    end: usize,
    changes: Vec<Change>,
}

fn input_lines(input: &str) -> Vec<String> {
    if input.is_empty() {
        return Vec::new();
    }
    input[..input.len() - 1]
        .split('\n')
        .map(str::to_owned)
        .collect()
}

fn canonical_changes(base: &[String], new: &[String], side: MergeSide) -> Vec<Change> {
    let n = base.len();
    let m = new.len();
    let mut lcs = vec![vec![0usize; m + 1]; n + 1];

    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if base[i] == new[j] {
                1 + lcs[i + 1][j + 1]
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }

    #[derive(Clone)]
    enum Op {
        Keep,
        Delete,
        Insert(String),
    }

    let mut ops = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < n && j < m {
        if base[i] == new[j] {
            ops.push(Op::Keep);
            i += 1;
            j += 1;
        } else {
            let delete_len = lcs[i + 1][j];
            let insert_len = lcs[i][j + 1];
            if delete_len > insert_len || (delete_len == insert_len && (i + j) % 2 == 0) {
                ops.push(Op::Delete);
                i += 1;
            } else {
                ops.push(Op::Insert(new[j].clone()));
                j += 1;
            }
        }
    }
    while i < n {
        ops.push(Op::Delete);
        i += 1;
    }
    while j < m {
        ops.push(Op::Insert(new[j].clone()));
        j += 1;
    }

    let mut changes = Vec::new();
    let mut base_pos = 0usize;
    let mut k = 0usize;
    while k < ops.len() {
        if matches!(ops[k], Op::Keep) {
            base_pos += 1;
            k += 1;
            continue;
        }

        let start = base_pos;
        let mut deleted = 0usize;
        let mut replacement = Vec::new();
        while k < ops.len() && !matches!(ops[k], Op::Keep) {
            match &ops[k] {
                Op::Keep => unreachable!(),
                Op::Delete => {
                    deleted += 1;
                    base_pos += 1;
                }
                Op::Insert(line) => replacement.push(line.clone()),
            }
            k += 1;
        }
        changes.push(Change {
            start,
            end: start + deleted,
            replacement,
            side: side.clone(),
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

fn projection(
    base: &[String],
    start: usize,
    end: usize,
    changes: &[Change],
    side: MergeSide,
) -> Vec<String> {
    let mut selected: Vec<&Change> = changes
        .iter()
        .filter(|c| c.side == side)
        .collect();
    selected.sort_by_key(|c| (c.start, c.end));

    let mut out = Vec::new();
    let mut cursor = start;
    for change in selected {
        if change.start < cursor || change.end > end {
            continue;
        }
        out.extend(base[cursor..change.start].iter().cloned());
        out.extend(change.replacement.iter().cloned());
        cursor = change.end;
    }
    out.extend(base[cursor..end].iter().cloned());
    out
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
    for (text, side) in [
        (base, MergeSide::Base),
        (left, MergeSide::Left),
        (right, MergeSide::Right),
    ] {
        if let Some(byte) = text.as_bytes().iter().position(|&b| b == b'\r') {
            return Err(MergeError::CarriageReturn { side, byte });
        }
    }

    for (text, side) in [
        (base, MergeSide::Base),
        (left, MergeSide::Left),
        (right, MergeSide::Right),
    ] {
        if !text.is_empty() && !text.ends_with('\n') {
            return Err(MergeError::MissingFinalLf { side });
        }
    }

    let base_lines = input_lines(base);
    let left_lines = input_lines(left);
    let right_lines = input_lines(right);

    if base_lines.len() > 128 || left_lines.len() > 128 || right_lines.len() > 128 {
        return Err(MergeError::TooLarge {
            base_len: base_lines.len(),
            left_len: left_lines.len(),
            right_len: right_lines.len(),
        });
    }

    let left_changes = canonical_changes(&base_lines, &left_lines, MergeSide::Left);
    let right_changes = canonical_changes(&base_lines, &right_lines, MergeSide::Right);
    let all_changes: Vec<Change> = left_changes
        .iter()
        .chain(right_changes.iter())
        .cloned()
        .collect();

    let mut visited = vec![false; all_changes.len()];
    let mut components = Vec::new();

    for seed in 0..all_changes.len() {
        if visited[seed] {
            continue;
        }
        visited[seed] = true;
        let mut stack = vec![seed];
        let mut indices = Vec::new();
        while let Some(index) = stack.pop() {
            indices.push(index);
            for other in 0..all_changes.len() {
                if !visited[other] && interacts(&all_changes[index], &all_changes[other]) {
                    visited[other] = true;
                    stack.push(other);
                }
            }
        }

        let changes: Vec<Change> = indices.iter().map(|&i| all_changes[i].clone()).collect();
        let start = changes.iter().map(|c| c.start).min().unwrap();
        let end = changes.iter().map(|c| c.end).max().unwrap();
        components.push(Component {
            start,
            end,
            changes,
        });
    }

    components.sort_by_key(|c| (c.start, usize::from(c.start != c.end), c.end));

    let mut output = Vec::new();
    let mut conflicts = Vec::new();
    let mut base_cursor = 0usize;
    let mut ordinal = 0usize;

    for component in components {
        if component.start > base_cursor {
            append_lines(&mut output, &base_lines[base_cursor..component.start]);
        }

        let base_projection = base_lines[component.start..component.end].to_vec();
        let left_projection = projection(
            &base_lines,
            component.start,
            component.end,
            &component.changes,
            MergeSide::Left,
        );
        let right_projection = projection(
            &base_lines,
            component.start,
            component.end,
            &component.changes,
            MergeSide::Right,
        );

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
                    ordinal += 1;
                    let output_start = output.len();
                    output.push(format!("<<<<<<< LEFT {ordinal}"));
                    append_lines(&mut output, &left_projection);

                    let base_label_start = if component.start < component.end {
                        component.start + 1
                    } else {
                        component.start
                    };
                    output.push(format!(
                        "||||||| BASE {base_label_start},{}",
                        component.end - component.start
                    ));
                    append_lines(&mut output, &base_projection);
                    output.push("=======".to_owned());
                    append_lines(&mut output, &right_projection);
                    output.push(format!(">>>>>>> RIGHT {ordinal}"));

                    conflicts.push(Conflict {
                        ordinal,
                        base_start: component.start,
                        base_len: component.end - component.start,
                        output_start,
                        output_len: output.len() - output_start,
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
        let mut serialized = output.join("\n");
        serialized.push('\n');
        serialized
    };

    Ok(MergeResult {
        text,
        conflicts: conflicts.len(),
        regions: conflicts,
    })
}

fn main() {}