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
    side: usize,
}

#[derive(Clone)]
struct Component {
    start: usize,
    end: usize,
    changes: Vec<Change>,
}

#[derive(Clone, Copy)]
enum Op {
    Keep,
    Delete,
    Insert,
}

fn split_lines(input: &str) -> Vec<String> {
    if input.is_empty() {
        Vec::new()
    } else {
        input[..input.len() - 1]
            .split('\n')
            .map(str::to_owned)
            .collect()
    }
}

fn edit_changes(old: &[String], new: &[String], side: usize) -> Vec<Change> {
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

    let (mut i, mut j) = (0usize, 0usize);
    let mut base_pos = 0usize;
    let mut run_start: Option<usize> = None;
    let mut deletes = 0usize;
    let mut inserts = Vec::new();
    let mut changes = Vec::new();

    let flush = |changes: &mut Vec<Change>,
                 run_start: &mut Option<usize>,
                 deletes: &mut usize,
                 inserts: &mut Vec<String>| {
        if let Some(start) = run_start.take() {
            changes.push(Change {
                start,
                end: start + *deletes,
                replacement: std::mem::take(inserts),
                side,
            });
            *deletes = 0;
        }
    };

    while i < n || j < m {
        let op = if i == n {
            Op::Insert
        } else if j == m {
            Op::Delete
        } else if old[i] == new[j] {
            Op::Keep
        } else {
            let del = lcs[i + 1][j];
            let ins = lcs[i][j + 1];
            if del > ins || (del == ins && (i + j) % 2 == 0) {
                Op::Delete
            } else {
                Op::Insert
            }
        };

        match op {
            Op::Keep => {
                flush(
                    &mut changes,
                    &mut run_start,
                    &mut deletes,
                    &mut inserts,
                );
                i += 1;
                j += 1;
                base_pos += 1;
            }
            Op::Delete => {
                run_start.get_or_insert(base_pos);
                i += 1;
                base_pos += 1;
                deletes += 1;
            }
            Op::Insert => {
                run_start.get_or_insert(base_pos);
                inserts.push(new[j].clone());
                j += 1;
            }
        }
    }

    flush(
        &mut changes,
        &mut run_start,
        &mut deletes,
        &mut inserts,
    );
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

fn build_components(changes: Vec<Change>) -> Vec<Component> {
    let n = changes.len();
    let mut seen = vec![false; n];
    let mut components = Vec::new();

    for seed in 0..n {
        if seen[seed] {
            continue;
        }
        seen[seed] = true;
        let mut stack = vec![seed];
        let mut members = Vec::new();

        while let Some(i) = stack.pop() {
            members.push(changes[i].clone());
            for j in 0..n {
                if !seen[j] && interacts(&changes[i], &changes[j]) {
                    seen[j] = true;
                    stack.push(j);
                }
            }
        }

        let start = members.iter().map(|c| c.start).min().unwrap();
        let end = members.iter().map(|c| c.end).max().unwrap();
        components.push(Component {
            start,
            end,
            changes: members,
        });
    }

    components.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then_with(|| (a.start != a.end).cmp(&(b.start != b.end)))
            .then(a.end.cmp(&b.end))
    });
    components
}

fn project(base: &[String], component: &Component, side: usize) -> Vec<String> {
    let mut changes: Vec<&Change> = component
        .changes
        .iter()
        .filter(|c| c.side == side)
        .collect();
    changes.sort_by_key(|c| (c.start, c.end));

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
    let inputs = [
        (base, MergeSide::Base),
        (left, MergeSide::Left),
        (right, MergeSide::Right),
    ];

    for (input, side) in &inputs {
        if let Some(byte) = input.as_bytes().iter().position(|&b| b == b'\r') {
            return Err(MergeError::CarriageReturn {
                side: side.clone(),
                byte,
            });
        }
    }

    for (input, side) in &inputs {
        if !input.is_empty() && !input.ends_with('\n') {
            return Err(MergeError::MissingFinalLf { side: side.clone() });
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

    let mut changes = edit_changes(&base_lines, &left_lines, 0);
    changes.extend(edit_changes(&base_lines, &right_lines, 1));
    let components = build_components(changes);

    let mut output = Vec::<String>::new();
    let mut cursor = 0usize;
    let mut regions = Vec::new();

    for component in &components {
        if cursor < component.start {
            output.extend(base_lines[cursor..component.start].iter().cloned());
        }

        let base_projection = base_lines[component.start..component.end].to_vec();
        let left_projection = project(&base_lines, component, 0);
        let right_projection = project(&base_lines, component, 1);

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
                    let ordinal = regions.len() + 1;
                    let output_start = output.len();
                    output.push(format!("<<<<<<< LEFT {ordinal}"));
                    output.extend(left_projection.iter().cloned());

                    let region_len = component.end - component.start;
                    let marker_start = if region_len > 0 {
                        component.start + 1
                    } else {
                        component.start
                    };
                    output.push(format!("||||||| BASE {marker_start},{region_len}"));
                    output.extend(base_projection.iter().cloned());
                    output.push("=======".to_owned());
                    output.extend(right_projection.iter().cloned());
                    output.push(format!(">>>>>>> RIGHT {ordinal}"));

                    regions.push(Conflict {
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

        cursor = component.end;
    }

    output.extend(base_lines[cursor..].iter().cloned());
    let mut text = String::new();
    for line in output {
        text.push_str(&line);
        text.push('\n');
    }

    Ok(MergeResult {
        text,
        conflicts: regions.len(),
        regions,
    })
}