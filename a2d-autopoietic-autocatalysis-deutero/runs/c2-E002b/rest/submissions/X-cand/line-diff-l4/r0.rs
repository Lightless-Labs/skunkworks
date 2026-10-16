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
}

#[derive(Clone)]
struct Component {
    start: usize,
    end: usize,
    left: Vec<usize>,
    right: Vec<usize>,
}

#[derive(Clone, Copy)]
enum Op {
    Keep,
    Delete,
    Insert,
}

fn lines(input: &str) -> Vec<String> {
    if input.is_empty() {
        return Vec::new();
    }
    input[..input.len() - 1]
        .split('\n')
        .map(str::to_owned)
        .collect()
}

fn edit_script(old: &[String], new: &[String]) -> Vec<Change> {
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
    while i < n || j < m {
        if i == n {
            ops.push(Op::Insert);
            j += 1;
        } else if j == m {
            ops.push(Op::Delete);
            i += 1;
        } else if old[i] == new[j] {
            ops.push(Op::Keep);
            i += 1;
            j += 1;
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
            if delete {
                ops.push(Op::Delete);
                i += 1;
            } else {
                ops.push(Op::Insert);
                j += 1;
            }
        }
    }

    let mut changes = Vec::new();
    let (mut old_i, mut new_i, mut k) = (0, 0, 0);
    while k < ops.len() {
        if matches!(ops[k], Op::Keep) {
            old_i += 1;
            new_i += 1;
            k += 1;
            continue;
        }

        let start = old_i;
        let mut deleted = 0;
        let mut replacement = Vec::new();
        while k < ops.len() && !matches!(ops[k], Op::Keep) {
            match ops[k] {
                Op::Delete => {
                    deleted += 1;
                    old_i += 1;
                }
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
            end: start + deleted,
            replacement,
        });
    }
    changes
}

fn interacts(a: &Change, b: &Change) -> bool {
    let a_zero = a.start == a.end;
    let b_zero = b.start == b.end;
    match (a_zero, b_zero) {
        (true, true) => a.start == b.start,
        (false, false) => a.start.max(b.start) < a.end.min(b.end),
        (true, false) => b.start < a.start && a.start < b.end,
        (false, true) => a.start < b.start && b.start < a.end,
    }
}

fn projection(base: &[String], start: usize, end: usize, changes: &[Change], ids: &[usize]) -> Vec<String> {
    if ids.is_empty() {
        return base[start..end].to_vec();
    }

    let mut out = Vec::new();
    let mut cursor = start;
    for &id in ids {
        let change = &changes[id];
        if change.start > cursor {
            out.extend_from_slice(&base[cursor..change.start]);
        }
        out.extend(change.replacement.iter().cloned());
        cursor = change.end;
    }
    if cursor < end {
        out.extend_from_slice(&base[cursor..end]);
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

    let left_changes = edit_script(&base_lines, &left_lines);
    let right_changes = edit_script(&base_lines, &right_lines);

    let mut nodes: Vec<(bool, usize)> = (0..left_changes.len())
        .map(|i| (true, i))
        .chain((0..right_changes.len()).map(|i| (false, i)))
        .collect();
    let mut parent: Vec<usize> = (0..nodes.len()).collect();

    fn find(parent: &mut [usize], x: usize) -> usize {
        if parent[x] != x {
            parent[x] = find(parent, parent[x]);
        }
        parent[x]
    }

    for a in 0..nodes.len() {
        for b in a + 1..nodes.len() {
            let (a_left, a_id) = nodes[a];
            let (b_left, b_id) = nodes[b];
            let ca = if a_left {
                &left_changes[a_id]
            } else {
                &right_changes[a_id]
            };
            let cb = if b_left {
                &left_changes[b_id]
            } else {
                &right_changes[b_id]
            };
            if interacts(ca, cb) {
                let ra = find(&mut parent, a);
                let rb = find(&mut parent, b);
                if ra != rb {
                    parent[rb] = ra;
                }
            }
        }
    }

    let mut groups: Vec<Vec<usize>> = Vec::new();
    for i in 0..nodes.len() {
        let root = find(&mut parent, i);
        if let Some(group) = groups.iter_mut().find(|g| {
            let first = g[0];
            find(&mut parent, first) == root
        }) {
            group.push(i);
        } else {
            groups.push(vec![i]);
        }
    }

    let mut components = Vec::new();
    for group in groups {
        let mut start = usize::MAX;
        let mut end = 0;
        let mut left_ids = Vec::new();
        let mut right_ids = Vec::new();
        for node in group {
            let (is_left, id) = nodes[node];
            let change = if is_left {
                &left_changes[id]
            } else {
                &right_changes[id]
            };
            start = start.min(change.start);
            end = end.max(change.end);
            if is_left {
                left_ids.push(id);
            } else {
                right_ids.push(id);
            }
        }
        left_ids.sort_unstable();
        right_ids.sort_unstable();
        components.push(Component {
            start,
            end,
            left: left_ids,
            right: right_ids,
        });
    }

    components.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then_with(|| (a.start != a.end).cmp(&(b.start != b.end)))
            .then(a.end.cmp(&b.end))
    });

    let mut output = Vec::new();
    let mut conflicts = Vec::new();
    let mut base_cursor = 0;

    for component in components {
        if base_cursor < component.start {
            output.extend_from_slice(&base_lines[base_cursor..component.start]);
        }

        let base_projection = base_lines[component.start..component.end].to_vec();
        let left_projection = projection(
            &base_lines,
            component.start,
            component.end,
            &left_changes,
            &component.left,
        );
        let right_projection = projection(
            &base_lines,
            component.start,
            component.end,
            &right_changes,
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
                    let region_len = component.end - component.start;
                    let base_line_number = if region_len > 0 {
                        component.start + 1
                    } else {
                        component.start
                    };
                    output.push(format!("<<<<<<< LEFT {ordinal}"));
                    output.extend(left_projection);
                    output.push(format!(
                        "||||||| BASE {base_line_number},{region_len}"
                    ));
                    output.extend(base_projection);
                    output.push("=======".to_owned());
                    output.extend(right_projection);
                    output.push(format!(">>>>>>> RIGHT {ordinal}"));
                    let output_len = 4
                        + output_start
                        .checked_sub(output_start)
                        .unwrap_or(0);
                    let block_len = output.len() - output_start;
                    let _ = output_len;
                    conflicts.push(Conflict {
                        ordinal,
                        base_start: component.start,
                        base_len: region_len,
                        output_start,
                        output_len: block_len,
                    });
                }
            }
        }
        base_cursor = component.end;
    }

    if base_cursor < base_lines.len() {
        output.extend_from_slice(&base_lines[base_cursor..]);
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