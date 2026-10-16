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

#[derive(Debug, Clone, Copy)]
enum Op {
    Keep,
    Delete,
    Insert,
}

fn validate_cr(text: &str, side: MergeSide) -> Result<(), MergeError> {
    if let Some(byte) = text.as_bytes().iter().position(|&b| b == b'\r') {
        return Err(MergeError::CarriageReturn { side, byte });
    }
    Ok(())
}

fn line_count(text: &str) -> usize {
    if text.is_empty() {
        0
    } else {
        text.bytes().filter(|&b| b == b'\n').count()
    }
}

fn split_lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        Vec::new()
    } else {
        text[..text.len() - 1]
            .split('\n')
            .map(str::to_owned)
            .collect()
    }
}

fn changes(old: &[String], new: &[String]) -> Vec<Change> {
    let m = old.len();
    let n = new.len();
    let mut lcs = vec![vec![0usize; n + 1]; m + 1];

    for i in (0..m).rev() {
        for j in (0..n).rev() {
            lcs[i][j] = if old[i] == new[j] {
                1 + lcs[i + 1][j + 1]
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }

    let mut ops = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < m || j < n {
        if i == m {
            ops.push(Op::Insert);
            j += 1;
        } else if j == n {
            ops.push(Op::Delete);
            i += 1;
        } else if old[i] == new[j] {
            ops.push(Op::Keep);
            i += 1;
            j += 1;
        } else {
            let delete_score = lcs[i + 1][j];
            let insert_score = lcs[i][j + 1];
            if delete_score > insert_score
                || (delete_score == insert_score && (i + j) % 2 == 0)
            {
                ops.push(Op::Delete);
                i += 1;
            } else {
                ops.push(Op::Insert);
                j += 1;
            }
        }
    }

    let mut result = Vec::new();
    let (mut old_i, mut new_i) = (0, 0);
    let mut run_start = 0;
    let mut run_deletes = 0;
    let mut replacement = Vec::new();
    let mut in_run = false;

    for op in ops {
        match op {
            Op::Keep => {
                if in_run {
                    result.push(Change {
                        start: run_start,
                        end: run_start + run_deletes,
                        replacement: std::mem::take(&mut replacement),
                    });
                    run_deletes = 0;
                    in_run = false;
                }
                old_i += 1;
                new_i += 1;
            }
            Op::Delete => {
                if !in_run {
                    in_run = true;
                    run_start = old_i;
                }
                old_i += 1;
                run_deletes += 1;
            }
            Op::Insert => {
                if !in_run {
                    in_run = true;
                    run_start = old_i;
                }
                replacement.push(new[new_i].clone());
                new_i += 1;
            }
        }
    }

    if in_run {
        result.push(Change {
            start: run_start,
            end: run_start + run_deletes,
            replacement,
        });
    }

    result
}

fn interacts(a: &Change, b: &Change) -> bool {
    let a_len = a.end - a.start;
    let b_len = b.end - b.start;
    match (a_len, b_len) {
        (0, 0) => a.start == b.start,
        (0, _) => b.start < a.start && a.start < b.end,
        (_, 0) => a.start < b.start && b.start < a.end,
        _ => a.start.max(b.start) < a.end.min(b.end),
    }
}

fn components(left: &[Change], right: &[Change]) -> Vec<Component> {
    let total = left.len() + right.len();
    let mut parent: Vec<usize> = (0..total).collect();

    fn root(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }

    fn union(parent: &mut [usize], a: usize, b: usize) {
        let ra = root(parent, a);
        let rb = root(parent, b);
        if ra != rb {
            parent[rb] = ra;
        }
    }

    for i in 0..total {
        for j in (i + 1)..total {
            let a = if i < left.len() {
                &left[i]
            } else {
                &right[i - left.len()]
            };
            let b = if j < left.len() {
                &left[j]
            } else {
                &right[j - left.len()]
            };
            if interacts(a, b) {
                union(&mut parent, i, j);
            }
        }
    }

    let mut groups: Vec<(usize, Vec<usize>)> = Vec::new();
    for i in 0..total {
        let r = root(&mut parent, i);
        let group_index = groups.iter().position(|(g, _)| *g == r);
        let index = match group_index {
            Some(index) => index,
            None => {
                groups.push((r, Vec::new()));
                groups.len() - 1
            }
        };
        groups[index].1.push(i);
    }

    let mut result = Vec::new();
    for (_, members) in groups {
        let mut component = Component {
            start: usize::MAX,
            end: 0,
            left: Vec::new(),
            right: Vec::new(),
        };
        for member in members {
            if member < left.len() {
                let change = &left[member];
                component.left.push(member);
                component.start = component.start.min(change.start);
                component.end = component.end.max(change.end);
            } else {
                let index = member - left.len();
                let change = &right[index];
                component.right.push(index);
                component.start = component.start.min(change.start);
                component.end = component.end.max(change.end);
            }
        }
        component.left.sort_unstable();
        component.right.sort_unstable();
        result.push(component);
    }

    result.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then_with(|| (a.start != a.end).cmp(&(b.start != b.end)))
    });
    result
}

fn project(
    base: &[String],
    start: usize,
    end: usize,
    changes: &[Change],
    indices: &[usize],
) -> Vec<String> {
    let mut output = Vec::new();
    let mut cursor = start;

    for &index in indices {
        let change = &changes[index];
        output.extend(base[cursor..change.start].iter().cloned());
        output.extend(change.replacement.iter().cloned());
        cursor = change.end;
    }

    output.extend(base[cursor..end].iter().cloned());
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

    let base_len = line_count(base);
    let left_len = line_count(left);
    let right_len = line_count(right);
    if base_len > 128 || left_len > 128 || right_len > 128 {
        return Err(MergeError::TooLarge {
            base_len,
            left_len,
            right_len,
        });
    }

    let base_lines = split_lines(base);
    let left_lines = split_lines(left);
    let right_lines = split_lines(right);

    let left_changes = changes(&base_lines, &left_lines);
    let right_changes = changes(&base_lines, &right_lines);
    let mut comps = components(&left_changes, &right_changes);
    comps.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then_with(|| (a.start != a.end).cmp(&(b.start != b.end)))
    });

    let mut output = Vec::new();
    let mut conflicts = Vec::new();
    let mut cursor = 0usize;

    for component in comps {
        append_lines(&mut output, &base_lines[cursor..component.start]);

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
                    let base_len = component.end - component.start;
                    let base_marker_start = if base_len > 0 {
                        component.start + 1
                    } else {
                        component.start
                    };

                    output.push(format!("<<<<<<< LEFT {ordinal}"));
                    append_lines(&mut output, &left_projection);
                    output.push(format!(
                        "||||||| BASE {base_marker_start},{base_len}"
                    ));
                    append_lines(&mut output, &base_projection);
                    output.push("=======".to_owned());
                    append_lines(&mut output, &right_projection);
                    output.push(format!(">>>>>>> RIGHT {ordinal}"));

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

        cursor = component.end;
    }

    append_lines(&mut output, &base_lines[cursor..]);
    let text = if output.is_empty() {
        String::new()
    } else {
        let mut text = output.join("\n");
        text.push('\n');
        text
    };

    Ok(MergeResult {
        conflicts: conflicts.len(),
        regions: conflicts,
        text,
    })
}