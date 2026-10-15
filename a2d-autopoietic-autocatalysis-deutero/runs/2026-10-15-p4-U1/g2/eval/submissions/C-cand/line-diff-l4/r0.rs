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
    let rows = old.len() + 1;
    let cols = new.len() + 1;
    let mut lcs = vec![vec![0usize; cols]; rows];

    for i in (0..old.len()).rev() {
        for j in (0..new.len()).rev() {
            lcs[i][j] = if old[i] == new[j] {
                1 + lcs[i + 1][j + 1]
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }

    let mut changes = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    let mut active: Option<Change> = None;

    while i < old.len() || j < new.len() {
        let op = if i == old.len() {
            Op::Insert
        } else if j == new.len() {
            Op::Delete
        } else if old[i] == new[j] {
            Op::Keep
        } else {
            let delete_len = lcs[i + 1][j];
            let insert_len = lcs[i][j + 1];
            if delete_len > insert_len {
                Op::Delete
            } else if insert_len > delete_len {
                Op::Insert
            } else if (i + j) % 2 == 0 {
                Op::Delete
            } else {
                Op::Insert
            }
        };

        match op {
            Op::Keep => {
                if let Some(change) = active.take() {
                    changes.push(change);
                }
                i += 1;
                j += 1;
            }
            Op::Delete => {
                let change = active.get_or_insert_with(|| Change {
                    start: i,
                    end: i,
                    replacement: Vec::new(),
                    side: 0,
                });
                i += 1;
                change.end = i;
            }
            Op::Insert => {
                let change = active.get_or_insert_with(|| Change {
                    start: i,
                    end: i,
                    replacement: Vec::new(),
                    side: 0,
                });
                change.replacement.push(new[j].clone());
                j += 1;
            }
        }
    }

    if let Some(change) = active {
        changes.push(change);
    }

    changes
}

fn interacts(a: &Change, b: &Change) -> bool {
    let a_empty = a.start == a.end;
    let b_empty = b.start == b.end;

    match (a_empty, b_empty) {
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
) -> Vec<String> {
    let mut result = Vec::new();
    let mut cursor = start;

    for change in changes {
        if change.start < cursor || change.start > end || change.end > end {
            continue;
        }

        result.extend(base[cursor..change.start].iter().cloned());
        result.extend(change.replacement.iter().cloned());
        cursor = change.end;
    }

    result.extend(base[cursor..end].iter().cloned());
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

    let mut changes = edit_script(&base_lines, &left_lines);
    let left_count = changes.len();
    for change in &mut changes {
        change.side = 0;
    }

    let right_changes = edit_script(&base_lines, &right_lines);
    for mut change in right_changes {
        change.side = 1;
        changes.push(change);
    }

    let count = changes.len();
    let mut parent: Vec<usize> = (0..count).collect();

    fn find(parent: &mut [usize], x: usize) -> usize {
        if parent[x] != x {
            parent[x] = find(parent, parent[x]);
        }
        parent[x]
    }

    for i in 0..count {
        for j in (i + 1)..count {
            if interacts(&changes[i], &changes[j]) {
                let a = find(&mut parent, i);
                let b = find(&mut parent, j);
                if a != b {
                    parent[b] = a;
                }
            }
        }
    }

    let mut components: Vec<Vec<usize>> = Vec::new();
    let mut roots: Vec<usize> = Vec::new();
    for i in 0..count {
        let root = find(&mut parent, i);
        if let Some(pos) = roots.iter().position(|&r| r == root) {
            components[pos].push(i);
        } else {
            roots.push(root);
            components.push(vec![i]);
        }
    }

    let mut regions: Vec<(usize, usize, Vec<usize>)> = components
        .into_iter()
        .map(|indices| {
            let start = indices.iter().map(|&i| changes[i].start).min().unwrap();
            let end = indices.iter().map(|&i| changes[i].end).max().unwrap();
            (start, end, indices)
        })
        .collect();

    regions.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| (a.0 != a.1).cmp(&(b.0 != b.1)))
            .then(a.1.cmp(&b.1))
    });

    let mut output = Vec::new();
    let mut conflict_records = Vec::new();
    let mut base_cursor = 0usize;

    for (start, end, indices) in regions {
        append_lines(&mut output, &base_lines[base_cursor..start]);

        let mut left_component = Vec::new();
        let mut right_component = Vec::new();
        for &index in &indices {
            if changes[index].side == 0 {
                left_component.push(changes[index].clone());
            } else {
                right_component.push(changes[index].clone());
            }
        }

        left_component.sort_by_key(|c| (c.start, c.end));
        right_component.sort_by_key(|c| (c.start, c.end));

        let base_projection = base_lines[start..end].to_vec();
        let left_projection = projection(&base_lines, start, end, &left_component);
        let right_projection = projection(&base_lines, start, end, &right_component);

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
                    let ordinal = conflict_records.len() + 1;
                    let output_start = output.len();
                    output.push(format!("<<<<<<< LEFT {}", ordinal));
                    append_lines(&mut output, &left_projection);

                    let region_len = end - start;
                    let displayed_start = if region_len > 0 { start + 1 } else { start };
                    output.push(format!(
                        "||||||| BASE {},{}",
                        displayed_start, region_len
                    ));
                    append_lines(&mut output, &base_projection);
                    output.push("=======".to_owned());
                    append_lines(&mut output, &right_projection);
                    output.push(format!(">>>>>>> RIGHT {}", ordinal));

                    let output_len = 4
                        + left_projection.len()
                        + base_projection.len()
                        + right_projection.len();

                    conflict_records.push(Conflict {
                        ordinal,
                        base_start: start,
                        base_len: region_len,
                        output_start,
                        output_len,
                    });
                }
            }
        }

        base_cursor = end;
    }

    append_lines(&mut output, &base_lines[base_cursor..]);

    let mut text = String::new();
    for line in &output {
        text.push_str(line);
        text.push('\n');
    }

    let _ = left_count;
    Ok(MergeResult {
        text,
        conflicts: conflict_records.len(),
        regions: conflict_records,
    })
}

fn main() {}