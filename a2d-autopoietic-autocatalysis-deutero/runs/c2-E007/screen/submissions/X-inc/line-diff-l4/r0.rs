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
    left: Vec<Change>,
    right: Vec<Change>,
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

fn canonical_changes(base: &[String], side: &[String]) -> Vec<Change> {
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

    #[derive(Clone)]
    enum Op {
        Keep,
        Delete,
        Insert(String),
    }

    let mut ops = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < n && j < m {
        if base[i] == side[j] {
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
                ops.push(Op::Insert(side[j].clone()));
                j += 1;
            }
        }
    }
    while i < n {
        ops.push(Op::Delete);
        i += 1;
    }
    while j < m {
        ops.push(Op::Insert(side[j].clone()));
        j += 1;
    }

    let mut changes = Vec::new();
    let mut base_pos = 0usize;
    let mut current: Option<Change> = None;

    for op in ops {
        match op {
            Op::Keep => {
                if let Some(change) = current.take() {
                    changes.push(change);
                }
                base_pos += 1;
            }
            Op::Delete => {
                let change = current.get_or_insert_with(|| Change {
                    start: base_pos,
                    end: base_pos,
                    replacement: Vec::new(),
                });
                change.end += 1;
                base_pos += 1;
            }
            Op::Insert(line) => {
                let change = current.get_or_insert_with(|| Change {
                    start: base_pos,
                    end: base_pos,
                    replacement: Vec::new(),
                });
                change.replacement.push(line);
            }
        }
    }
    if let Some(change) = current {
        changes.push(change);
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

fn projections(
    base: &[String],
    start: usize,
    end: usize,
    changes: &[Change],
) -> Vec<String> {
    let mut out = Vec::new();
    let mut cursor = start;
    for change in changes {
        if change.start > cursor {
            out.extend(base[cursor..change.start].iter().cloned());
        }
        out.extend(change.replacement.iter().cloned());
        cursor = change.end;
    }
    if cursor < end {
        out.extend(base[cursor..end].iter().cloned());
    }
    out
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

    let left_changes = canonical_changes(&base_lines, &left_lines);
    let right_changes = canonical_changes(&base_lines, &right_lines);

    let mut nodes: Vec<(bool, usize)> = Vec::new();
    nodes.extend((0..left_changes.len()).map(|i| (true, i)));
    nodes.extend((0..right_changes.len()).map(|i| (false, i)));

    let mut parent: Vec<usize> = (0..nodes.len()).collect();

    fn find(parent: &mut [usize], x: usize) -> usize {
        if parent[x] != x {
            parent[x] = find(parent, parent[x]);
        }
        parent[x]
    }

    for a in 0..nodes.len() {
        for b in (a + 1)..nodes.len() {
            let change_a = if nodes[a].0 {
                &left_changes[nodes[a].1]
            } else {
                &right_changes[nodes[a].1]
            };
            let change_b = if nodes[b].0 {
                &left_changes[nodes[b].1]
            } else {
                &right_changes[nodes[b].1]
            };
            if interacts(change_a, change_b) {
                let root_a = find(&mut parent, a);
                let root_b = find(&mut parent, b);
                if root_a != root_b {
                    parent[root_b] = root_a;
                }
            }
        }
    }

    let mut groups: Vec<Vec<usize>> = Vec::new();
    for idx in 0..nodes.len() {
        let root = find(&mut parent, idx);
        if let Some(group) = groups.iter_mut().find(|g| {
            g.first()
                .is_some_and(|&member| find(&mut parent, member) == root)
        }) {
            group.push(idx);
        } else {
            groups.push(vec![idx]);
        }
    }

    let mut components = Vec::new();
    for group in groups {
        let mut component = Component {
            start: usize::MAX,
            end: 0,
            left: Vec::new(),
            right: Vec::new(),
        };
        for idx in group {
            let (is_left, change_idx) = nodes[idx];
            let change = if is_left {
                left_changes[change_idx].clone()
            } else {
                right_changes[change_idx].clone()
            };
            component.start = component.start.min(change.start);
            component.end = component.end.max(change.end);
            if is_left {
                component.left.push(change);
            } else {
                component.right.push(change);
            }
        }
        component.left.sort_by_key(|c| (c.start, c.end));
        component.right.sort_by_key(|c| (c.start, c.end));
        components.push(component);
    }

    components.sort_by_key(|c| (c.start, usize::from(c.start != c.end), c.end));

    let mut output = Vec::new();
    let mut conflicts = Vec::new();
    let mut base_cursor = 0usize;

    for component in components {
        if base_cursor < component.start {
            output.extend(base_lines[base_cursor..component.start].iter().cloned());
        }

        let base_projection = base_lines[component.start..component.end].to_vec();
        let left_projection = projections(
            &base_lines,
            component.start,
            component.end,
            &component.left,
        );
        let right_projection = projections(
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
                    let base_label_start = if component.start < component.end {
                        component.start + 1
                    } else {
                        component.start
                    };

                    output.push(format!("<<<<<<< LEFT {ordinal}"));
                    output.extend(left_projection);
                    output.push(format!(
                        "||||||| BASE {base_label_start},{}",
                        component.end - component.start
                    ));
                    output.extend(base_projection);
                    output.push("=======".to_owned());
                    output.extend(right_projection);
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

        base_cursor = component.end;
    }

    output.extend(base_lines[base_cursor..].iter().cloned());

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