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
    side: usize,
}

#[derive(Debug, Clone)]
struct Component {
    start: usize,
    end: usize,
    changes: Vec<usize>,
}

fn lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    text.strip_suffix('\n')
        .unwrap_or(text)
        .split('\n')
        .map(str::to_owned)
        .collect()
}

fn edit_changes(old: &[String], new: &[String], side: usize) -> Vec<Change> {
    #[derive(Clone)]
    enum Op {
        Keep,
        Delete,
        Insert(String),
    }

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

    let (mut i, mut j) = (0, 0);
    let mut ops = Vec::new();
    while i < n || j < m {
        if i < n && j < m && old[i] == new[j] {
            ops.push(Op::Keep);
            i += 1;
            j += 1;
        } else if i == n {
            ops.push(Op::Insert(new[j].clone()));
            j += 1;
        } else if j == m {
            ops.push(Op::Delete);
            i += 1;
        } else {
            let delete_len = lcs[i + 1][j];
            let insert_len = lcs[i][j + 1];
            let delete = if delete_len > insert_len {
                true
            } else if insert_len > delete_len {
                false
            } else {
                (i + j) % 2 == 0
            };

            if delete {
                ops.push(Op::Delete);
                i += 1;
            } else {
                ops.push(Op::Insert(new[j].clone()));
                j += 1;
            }
        }
    }

    let mut changes = Vec::new();
    let (mut base_pos, mut run_start, mut deletes) = (0usize, None, 0usize);
    let mut inserts = Vec::new();

    for op in ops {
        match op {
            Op::Keep => {
                if let Some(start) = run_start.take() {
                    changes.push(Change {
                        start,
                        end: start + deletes,
                        replacement: std::mem::take(&mut inserts),
                        side,
                    });
                    deletes = 0;
                }
                base_pos += 1;
            }
            Op::Delete => {
                run_start.get_or_insert(base_pos);
                deletes += 1;
                base_pos += 1;
            }
            Op::Insert(line) => {
                run_start.get_or_insert(base_pos);
                inserts.push(line);
            }
        }
    }
    if let Some(start) = run_start {
        changes.push(Change {
            start,
            end: start + deletes,
            replacement: inserts,
            side,
        });
    }

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

fn projection(
    base: &[String],
    region_start: usize,
    region_end: usize,
    changes: &[Change],
    component_changes: &[usize],
    side: usize,
) -> Vec<String> {
    let mut selected: Vec<&Change> = component_changes
        .iter()
        .map(|&idx| &changes[idx])
        .filter(|change| change.side == side)
        .collect();
    selected.sort_by_key(|change| (change.start, change.end));

    let mut result = Vec::new();
    let mut cursor = region_start;
    for change in selected {
        result.extend(base[cursor..change.start].iter().cloned());
        result.extend(change.replacement.iter().cloned());
        cursor = change.end;
    }
    result.extend(base[cursor..region_end].iter().cloned());
    result
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
    let inputs = [
        (base, MergeSide::Base),
        (left, MergeSide::Left),
        (right, MergeSide::Right),
    ];

    for (text, side) in &inputs {
        if let Some(byte) = text.as_bytes().iter().position(|&b| b == b'\r') {
            return Err(MergeError::CarriageReturn {
                side: side.clone(),
                byte,
            });
        }
    }

    for (text, side) in &inputs {
        if !text.is_empty() && !text.ends_with('\n') {
            return Err(MergeError::MissingFinalLf { side: side.clone() });
        }
    }

    let base_lines = lines(base);
    let left_lines = lines(left);
    let right_lines = lines(right);

    if base_lines.len() > 128 || left_lines.len() > 128 || right_lines.len() > 128 {
        return Err(MergeError::TooLarge {
            base_len: base_lines.len(),
            left_len: left_lines.len(),
            right_len: right_lines.len(),
        });
    }

    let mut changes = edit_changes(&base_lines, &left_lines, 0);
    changes.extend(edit_changes(&base_lines, &right_lines, 1));

    let mut visited = vec![false; changes.len()];
    let mut components = Vec::new();

    for seed in 0..changes.len() {
        if visited[seed] {
            continue;
        }
        visited[seed] = true;
        let mut stack = vec![seed];
        let mut members = Vec::new();

        while let Some(current) = stack.pop() {
            members.push(current);
            for candidate in 0..changes.len() {
                if !visited[candidate] && interact(&changes[current], &changes[candidate]) {
                    visited[candidate] = true;
                    stack.push(candidate);
                }
            }
        }

        let start = members.iter().map(|&i| changes[i].start).min().unwrap();
        let end = members.iter().map(|&i| changes[i].end).max().unwrap();
        components.push(Component {
            start,
            end,
            changes: members,
        });
    }

    components.sort_by_key(|component| (component.start, component.end > component.start));

    let mut output = Vec::new();
    let mut conflicts = Vec::new();
    let mut base_cursor = 0usize;

    for component in components {
        output.extend(base_lines[base_cursor..component.start].iter().cloned());

        let base_projection = base_lines[component.start..component.end].to_vec();
        let left_projection = projection(
            &base_lines,
            component.start,
            component.end,
            &changes,
            &component.changes,
            0,
        );
        let right_projection = projection(
            &base_lines,
            component.start,
            component.end,
            &changes,
            &component.changes,
            1,
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
                    let ordinal = conflicts.len() + 1;
                    let output_start = output.len();
                    output.push(format!("<<<<<<< LEFT {}", ordinal));
                    append_lines(&mut output, &left_projection);
                    let base_label_start = if component.end > component.start {
                        component.start + 1
                    } else {
                        component.start
                    };
                    output.push(format!(
                        "||||||| BASE {},{}",
                        base_label_start,
                        component.end - component.start
                    ));
                    append_lines(&mut output, &base_projection);
                    output.push("=======".to_owned());
                    append_lines(&mut output, &right_projection);
                    output.push(format!(">>>>>>> RIGHT {}", ordinal));
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

    output.extend(base_lines[base_cursor..].iter().cloned());
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