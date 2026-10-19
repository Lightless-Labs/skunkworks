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

#[derive(Clone, Debug)]
struct Change {
    start: usize,
    end: usize,
    replacement: Vec<String>,
}

#[derive(Clone, Debug)]
struct Component {
    start: usize,
    end: usize,
    left: Vec<Change>,
    right: Vec<Change>,
}

fn lines(input: &str) -> Vec<String> {
    if input.is_empty() {
        Vec::new()
    } else {
        input[..input.len() - 1]
            .split('\n')
            .map(str::to_owned)
            .collect()
    }
}

fn canonical_changes(old: &[String], new: &[String]) -> Vec<Change> {
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

    let mut changes = Vec::new();
    let (mut i, mut j, mut base_pos) = (0usize, 0usize, 0usize);
    let mut run_start = None;
    let mut deletes = 0usize;
    let mut inserts = Vec::new();

    let finish_run = |changes: &mut Vec<Change>,
                      run_start: &mut Option<usize>,
                      deletes: &mut usize,
                      inserts: &mut Vec<String>| {
        if let Some(start) = run_start.take() {
            changes.push(Change {
                start,
                end: start + *deletes,
                replacement: std::mem::take(inserts),
            });
            *deletes = 0;
        }
    };

    while i < n || j < m {
        if i < n && j < m && old[i] == new[j] {
            finish_run(&mut changes, &mut run_start, &mut deletes, &mut inserts);
            i += 1;
            j += 1;
            base_pos += 1;
        } else if i == n {
            run_start.get_or_insert(base_pos);
            inserts.push(new[j].clone());
            j += 1;
        } else if j == m {
            run_start.get_or_insert(base_pos);
            deletes += 1;
            i += 1;
            base_pos += 1;
        } else {
            let delete_score = lcs[i + 1][j];
            let insert_score = lcs[i][j + 1];
            let delete = if delete_score > insert_score {
                true
            } else if insert_score > delete_score {
                false
            } else {
                (i + j) % 2 == 0
            };

            run_start.get_or_insert(base_pos);
            if delete {
                deletes += 1;
                i += 1;
                base_pos += 1;
            } else {
                inserts.push(new[j].clone());
                j += 1;
            }
        }
    }

    finish_run(&mut changes, &mut run_start, &mut deletes, &mut inserts);
    changes
}

fn interacts(a: &Change, b: &Change) -> bool {
    let a_positive = a.start < a.end;
    let b_positive = b.start < b.end;

    match (a_positive, b_positive) {
        (true, true) => a.start.max(b.start) < a.end.min(b.end),
        (false, false) => a.start == b.start,
        (false, true) => b.start < a.start && a.start < b.end,
        (true, false) => a.start < b.start && b.start < a.end,
    }
}

fn project(base: &[String], start: usize, end: usize, changes: &[Change]) -> Vec<String> {
    let mut result = Vec::new();
    let mut cursor = start;

    for change in changes {
        if change.start > cursor {
            result.extend(base[cursor..change.start].iter().cloned());
        }
        result.extend(change.replacement.iter().cloned());
        cursor = change.end;
    }

    if cursor < end {
        result.extend(base[cursor..end].iter().cloned());
    }
    result
}

fn push_line(output: &mut Vec<String>, line: String) {
    output.push(line);
}

pub fn merge_three_way(
    base: &str,
    left: &str,
    right: &str,
    resolution: Resolution,
) -> Result<MergeResult, MergeError> {
    let inputs = [
        (MergeSide::Base, base),
        (MergeSide::Left, left),
        (MergeSide::Right, right),
    ];

    for (side, input) in &inputs {
        if let Some(byte) = input.as_bytes().iter().position(|&b| b == b'\r') {
            return Err(MergeError::CarriageReturn {
                side: side.clone(),
                byte,
            });
        }
    }

    for (side, input) in &inputs {
        if !input.is_empty() && !input.ends_with('\n') {
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

    let left_changes = canonical_changes(&base_lines, &left_lines);
    let right_changes = canonical_changes(&base_lines, &right_lines);

    let mut nodes: Vec<(bool, usize)> = Vec::new();
    nodes.extend((0..left_changes.len()).map(|i| (true, i)));
    nodes.extend((0..right_changes.len()).map(|i| (false, i)));

    let mut seen = vec![false; nodes.len()];
    let mut components = Vec::new();

    for seed in 0..nodes.len() {
        if seen[seed] {
            continue;
        }

        let mut stack = vec![seed];
        seen[seed] = true;
        let mut component_left = Vec::new();
        let mut component_right = Vec::new();

        while let Some(node_idx) = stack.pop() {
            let (is_left, change_idx) = nodes[node_idx];
            let change = if is_left {
                &left_changes[change_idx]
            } else {
                &right_changes[change_idx]
            };

            if is_left {
                component_left.push(change.clone());
            } else {
                component_right.push(change.clone());
            }

            for other_idx in 0..nodes.len() {
                if seen[other_idx] {
                    continue;
                }
                let (other_left, other_change_idx) = nodes[other_idx];
                let other = if other_left {
                    &left_changes[other_change_idx]
                } else {
                    &right_changes[other_change_idx]
                };
                if interacts(change, other) {
                    seen[other_idx] = true;
                    stack.push(other_idx);
                }
            }
        }

        component_left.sort_by_key(|c| (c.start, c.end));
        component_right.sort_by_key(|c| (c.start, c.end));
        let all_changes = component_left.iter().chain(component_right.iter());
        let start = all_changes.clone().map(|c| c.start).min().unwrap();
        let end = all_changes.map(|c| c.end).max().unwrap();
        components.push(Component {
            start,
            end,
            left: component_left,
            right: component_right,
        });
    }

    components.sort_by_key(|c| (c.start, c.start != c.end, c.end));

    let mut output = Vec::new();
    let mut conflicts = Vec::new();
    let mut base_cursor = 0usize;

    for component in components {
        if base_cursor < component.start {
            output.extend(base_lines[base_cursor..component.start].iter().cloned());
        }

        let base_projection = base_lines[component.start..component.end].to_vec();
        let left_projection = project(
            &base_lines,
            component.start,
            component.end,
            &component.left,
        );
        let right_projection = project(
            &base_lines,
            component.start,
            component.end,
            &component.right,
        );

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
                    let base_len = component.end - component.start;
                    let base_line_start = if base_len > 0 {
                        component.start + 1
                    } else {
                        component.start
                    };

                    push_line(&mut output, format!("<<<<<<< LEFT {ordinal}"));
                    output.extend(left_projection);
                    push_line(&mut output, format!("||||||| BASE {base_line_start},{base_len}"));
                    output.extend(base_projection);
                    push_line(&mut output, "=======".to_owned());
                    output.extend(right_projection);

                    let output_len = 4
                        + component.left.iter().map(|_| 0usize).sum::<usize>()
                        + 0; // The payload lengths are recorded below.
                    let _ = output_len;
                    let block_len = output.len() - output_start + 1;
                    push_line(&mut output, format!(">>>>>>> RIGHT {ordinal}"));

                    conflicts.push(Conflict {
                        ordinal,
                        base_start: component.start,
                        base_len,
                        output_start,
                        output_len: block_len,
                    });
                }
            }
        }

        base_cursor = component.end;
    }

    if base_cursor < base_lines.len() {
        output.extend(base_lines[base_cursor..].iter().cloned());
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
        conflicts: conflicts.len(),
        regions: conflicts,
    })
}