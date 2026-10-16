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
}

#[derive(Debug, Clone)]
struct Component {
    start: usize,
    end: usize,
    left: Vec<usize>,
    right: Vec<usize>,
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

fn edit_changes(base: &[String], side: &[String]) -> Vec<Change> {
    let n = base.len();
    let m = side.len();
    let mut lcs = vec![vec![0usize; m + 1]; n + 1];

    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if base[i] == side[j] {
                1 + lcs[i + 1][j + 1]
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }

    let (mut i, mut j) = (0usize, 0usize);
    let mut changes = Vec::new();
    let mut active_start = None;
    let mut deletes = 0usize;
    let mut inserts = Vec::new();

    let flush = |changes: &mut Vec<Change>,
                 active_start: &mut Option<usize>,
                 deletes: &mut usize,
                 inserts: &mut Vec<String>,
                 i: usize| {
        if let Some(start) = active_start.take() {
            changes.push(Change {
                start,
                end: start + *deletes,
                replacement: std::mem::take(inserts),
            });
            *deletes = 0;
        }
        let _ = i;
    };

    while i < n || j < m {
        if i < n && j < m && base[i] == side[j] {
            flush(
                &mut changes,
                &mut active_start,
                &mut deletes,
                &mut inserts,
                i,
            );
            i += 1;
            j += 1;
            continue;
        }

        if active_start.is_none() {
            active_start = Some(i);
        }

        if i == n {
            inserts.push(side[j].clone());
            j += 1;
        } else if j == m {
            deletes += 1;
            i += 1;
        } else {
            let delete_score = lcs[i + 1][j];
            let insert_score = lcs[i][j + 1];
            let take_delete = if delete_score > insert_score {
                true
            } else if insert_score > delete_score {
                false
            } else {
                (i + j) % 2 == 0
            };

            if take_delete {
                deletes += 1;
                i += 1;
            } else {
                inserts.push(side[j].clone());
                j += 1;
            }
        }
    }

    flush(
        &mut changes,
        &mut active_start,
        &mut deletes,
        &mut inserts,
        i,
    );
    changes
}

fn interact(a: &Change, b: &Change) -> bool {
    let a_zero = a.start == a.end;
    let b_zero = b.start == b.end;

    match (a_zero, b_zero) {
        (false, false) => a.start.max(b.start) < a.end.min(b.end),
        (true, true) => a.start == b.start,
        (true, false) => b.start < a.start && a.start < b.end,
        (false, true) => a.start < b.start && b.start < a.end,
    }
}

fn components(left: &[Change], right: &[Change]) -> Vec<Component> {
    let mut nodes: Vec<(bool, usize)> = (0..left.len())
        .map(|i| (true, i))
        .chain((0..right.len()).map(|i| (false, i)))
        .collect();
    let n = nodes.len();
    let mut parent: Vec<usize> = (0..n).collect();

    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }

    for a in 0..n {
        for b in a + 1..n {
            let ca = if nodes[a].0 {
                &left[nodes[a].1]
            } else {
                &right[nodes[a].1]
            };
            let cb = if nodes[b].0 {
                &left[nodes[b].1]
            } else {
                &right[nodes[b].1]
            };
            if interact(ca, cb) {
                let ra = root(&mut parent, a);
                let rb = root(&mut parent, b);
                if ra != rb {
                    parent[rb] = ra;
                }
            }
        }
    }

    let mut groups: Vec<(usize, Component)> = Vec::new();
    for (idx, &(is_left, change_idx)) in nodes.iter().enumerate() {
        let r = root(&mut parent, idx);
        let change = if is_left {
            &left[change_idx]
        } else {
            &right[change_idx]
        };

        if let Some((_, component)) = groups.iter_mut().find(|(group_root, _)| *group_root == r) {
            component.start = component.start.min(change.start);
            component.end = component.end.max(change.end);
            if is_left {
                component.left.push(change_idx);
            } else {
                component.right.push(change_idx);
            }
        } else {
            let mut component = Component {
                start: change.start,
                end: change.end,
                left: Vec::new(),
                right: Vec::new(),
            };
            if is_left {
                component.left.push(change_idx);
            } else {
                component.right.push(change_idx);
            }
            groups.push((r, component));
        }
    }

    let mut result: Vec<Component> = groups.into_iter().map(|(_, c)| c).collect();
    result.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then_with(|| (a.start != a.end).cmp(&(b.start != b.end)))
    });
    result
}

fn project(base: &[String], start: usize, end: usize, changes: &[Change], indices: &[usize]) -> Vec<String> {
    let mut selected: Vec<&Change> = indices.iter().map(|&i| &changes[i]).collect();
    selected.sort_by_key(|change| (change.start, change.end));

    let mut output = Vec::new();
    let mut cursor = start;

    for change in selected {
        while cursor < change.start {
            output.push(base[cursor].clone());
            cursor += 1;
        }
        output.extend(change.replacement.iter().cloned());
        cursor = change.end;
    }

    while cursor < end {
        output.push(base[cursor].clone());
        cursor += 1;
    }
    output
}

pub fn merge_three_way(
    base: &str,
    left: &str,
    right: &str,
    resolution: Resolution,
) -> Result<MergeResult, MergeError> {
    for (input, side) in [
        (base, MergeSide::Base),
        (left, MergeSide::Left),
        (right, MergeSide::Right),
    ] {
        if let Some(byte) = input.as_bytes().iter().position(|&b| b == b'\r') {
            return Err(MergeError::CarriageReturn { side, byte });
        }
    }

    for (input, side) in [
        (base, MergeSide::Base),
        (left, MergeSide::Left),
        (right, MergeSide::Right),
    ] {
        if !input.is_empty() && !input.ends_with('\n') {
            return Err(MergeError::MissingFinalLf { side });
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

    let left_changes = edit_changes(&base_lines, &left_lines);
    let right_changes = edit_changes(&base_lines, &right_lines);
    let components = components(&left_changes, &right_changes);

    let mut output_lines = Vec::new();
    let mut conflicts = Vec::new();
    let mut base_cursor = 0usize;

    for component in components {
        while base_cursor < component.start {
            output_lines.push(base_lines[base_cursor].clone());
            base_cursor += 1;
        }

        let base_projection = base_lines[component.start..component.end].to_vec();
        let left_projection = project(
            &base_lines,
            component.start,
            component.end,
            &left_changes,
            &component.left,
        );
        let right_projection = project(
            &base_lines,
            component.start,
            component.end,
            &right_changes,
            &component.right,
        );

        if left_projection == right_projection {
            output_lines.extend(left_projection);
        } else if left_projection == base_projection {
            output_lines.extend(right_projection);
        } else if right_projection == base_projection {
            output_lines.extend(left_projection);
        } else {
            match resolution {
                Resolution::PreferLeft => output_lines.extend(left_projection),
                Resolution::PreferRight => output_lines.extend(right_projection),
                Resolution::Mark => {
                    let ordinal = conflicts.len() + 1;
                    let output_start = output_lines.len();
                    let base_len = component.end - component.start;
                    let base_number = if base_len > 0 {
                        component.start + 1
                    } else {
                        component.start
                    };

                    output_lines.push(format!("<<<<<<< LEFT {ordinal}"));
                    output_lines.extend(left_projection);
                    output_lines.push(format!("||||||| BASE {base_number},{base_len}"));
                    output_lines.extend(base_projection);
                    output_lines.push("=======".to_owned());
                    output_lines.extend(right_projection);
                    output_lines.push(format!(">>>>>>> RIGHT {ordinal}"));

                    let output_len = output_lines.len() - output_start;
                    conflicts.push(Conflict {
                        ordinal,
                        base_start: component.start,
                        base_len,
                        output_start,
                        output_len,
                    });
                }
            }
        }

        base_cursor = component.end;
    }

    while base_cursor < base_lines.len() {
        output_lines.push(base_lines[base_cursor].clone());
        base_cursor += 1;
    }

    let mut text = String::new();
    for line in &output_lines {
        text.push_str(line);
        text.push('\n');
    }

    Ok(MergeResult {
        text,
        conflicts: conflicts.len(),
        regions: conflicts,
    })
}