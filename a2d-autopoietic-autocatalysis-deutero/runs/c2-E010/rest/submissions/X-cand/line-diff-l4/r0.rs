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

#[derive(Clone)]
struct Change {
    start: usize,
    end: usize,
    replacement: Vec<String>,
    side: usize,
}

struct Component {
    start: usize,
    end: usize,
    changes: Vec<usize>,
}

#[derive(Clone, Copy)]
enum Op {
    Keep,
    Delete,
    Insert,
}

fn split_lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    text[..text.len() - 1]
        .split('\n')
        .map(str::to_owned)
        .collect()
}

fn edit_script(old: &[String], new: &[String]) -> Vec<Op> {
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
    while i < n && j < m {
        if old[i] == new[j] {
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
    while i < n {
        ops.push(Op::Delete);
        i += 1;
    }
    while j < m {
        ops.push(Op::Insert);
        j += 1;
    }
    ops
}

fn changes_for(old: &[String], new: &[String], side: usize) -> Vec<Change> {
    let ops = edit_script(old, new);
    let (mut i, mut j) = (0usize, 0usize);
    let mut result = Vec::new();

    let mut active = false;
    let mut start = 0usize;
    let mut deletes = 0usize;
    let mut replacement = Vec::new();

    for op in ops {
        match op {
            Op::Keep => {
                if active {
                    result.push(Change {
                        start,
                        end: start + deletes,
                        replacement: std::mem::take(&mut replacement),
                        side,
                    });
                    active = false;
                    deletes = 0;
                }
                i += 1;
                j += 1;
            }
            Op::Delete => {
                if !active {
                    active = true;
                    start = i;
                }
                deletes += 1;
                i += 1;
            }
            Op::Insert => {
                if !active {
                    active = true;
                    start = i;
                }
                replacement.push(new[j].clone());
                j += 1;
            }
        }
    }
    if active {
        result.push(Change {
            start,
            end: start + deletes,
            replacement,
            side,
        });
    }
    result
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

fn projection(base: &[String], start: usize, end: usize, changes: &[&Change]) -> Vec<String> {
    let mut ordered = changes.to_vec();
    ordered.sort_by_key(|c| (c.start, c.end));
    let mut result = Vec::new();
    let mut cursor = start;

    for change in ordered {
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

fn append_lines(out: &mut Vec<String>, lines: &[String]) {
    out.extend(lines.iter().cloned());
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

    let mut changes = changes_for(&base_lines, &left_lines, 0);
    changes.extend(changes_for(&base_lines, &right_lines, 1));

    let count = changes.len();
    let mut parent: Vec<usize> = (0..count).collect();

    fn root(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }

    for i in 0..count {
        for j in (i + 1)..count {
            if interacts(&changes[i], &changes[j]) {
                let ri = root(&mut parent, i);
                let rj = root(&mut parent, j);
                if ri != rj {
                    parent[rj] = ri;
                }
            }
        }
    }

    let mut grouped: Vec<Component> = Vec::new();
    let mut roots = Vec::new();
    for i in 0..count {
        let r = root(&mut parent, i);
        let position = roots.iter().position(|&x| x == r);
        let idx = if let Some(idx) = position {
            idx
        } else {
            roots.push(r);
            grouped.push(Component {
                start: changes[i].start,
                end: changes[i].end,
                changes: Vec::new(),
            });
            grouped.len() - 1
        };
        grouped[idx].start = grouped[idx].start.min(changes[i].start);
        grouped[idx].end = grouped[idx].end.max(changes[i].end);
        grouped[idx].changes.push(i);
    }

    grouped.sort_by_key(|c| (c.start, c.end != c.start, c.end));

    let mut output = Vec::<String>::new();
    let mut conflicts = Vec::new();
    let mut base_cursor = 0usize;

    for component in grouped {
        if base_cursor < component.start {
            append_lines(&mut output, &base_lines[base_cursor..component.start]);
        }

        let base_projection = base_lines[component.start..component.end].to_vec();
        let left_changes: Vec<&Change> = component
            .changes
            .iter()
            .map(|&i| &changes[i])
            .filter(|c| c.side == 0)
            .collect();
        let right_changes: Vec<&Change> = component
            .changes
            .iter()
            .map(|&i| &changes[i])
            .filter(|c| c.side == 1)
            .collect();

        let left_projection = projection(
            &base_lines,
            component.start,
            component.end,
            &left_changes,
        );
        let right_projection = projection(
            &base_lines,
            component.start,
            component.end,
            &right_changes,
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
                    let region_len = component.end - component.start;
                    let base_marker_start = if region_len > 0 {
                        component.start + 1
                    } else {
                        component.start
                    };

                    output.push(format!("<<<<<<< LEFT {ordinal}"));
                    append_lines(&mut output, &left_projection);
                    output.push(format!(
                        "||||||| BASE {base_marker_start},{region_len}"
                    ));
                    append_lines(&mut output, &base_projection);
                    output.push("=======".to_owned());
                    append_lines(&mut output, &right_projection);
                    output.push(format!(">>>>>>> RIGHT {ordinal}"));

                    conflicts.push(Conflict {
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
