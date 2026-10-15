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

fn lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    text[..text.len() - 1]
        .split('\n')
        .map(str::to_owned)
        .collect()
}

fn edit_changes(base: &[String], newer: &[String], side: usize) -> Vec<Change> {
    let n = base.len();
    let m = newer.len();
    let mut lcs = vec![vec![0usize; m + 1]; n + 1];

    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if base[i] == newer[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }

    let (mut i, mut j) = (0usize, 0usize);
    let mut changes = Vec::new();
    let mut active_start = 0usize;
    let mut deletes = 0usize;
    let mut replacement = Vec::new();

    while i < n || j < m {
        if i < n && j < m && base[i] == newer[j] {
            if deletes > 0 || !replacement.is_empty() {
                changes.push(Change {
                    start: active_start,
                    end: active_start + deletes,
                    replacement: std::mem::take(&mut replacement),
                    side,
                });
                deletes = 0;
            }
            i += 1;
            j += 1;
            active_start = i;
        } else {
            if deletes == 0 && replacement.is_empty() {
                active_start = i;
            }

            let take_delete = if i == n {
                false
            } else if j == m {
                true
            } else {
                let del = lcs[i + 1][j];
                let ins = lcs[i][j + 1];
                if del > ins {
                    true
                } else if ins > del {
                    false
                } else {
                    (i + j) % 2 == 0
                }
            };

            if take_delete {
                i += 1;
                deletes += 1;
            } else {
                replacement.push(newer[j].clone());
                j += 1;
            }
        }
    }

    if deletes > 0 || !replacement.is_empty() {
        changes.push(Change {
            start: active_start,
            end: active_start + deletes,
            replacement,
            side,
        });
    }
    changes
}

fn interacts(a: &Change, b: &Change) -> bool {
    let a_len = a.end - a.start;
    let b_len = b.end - b.start;

    match (a_len > 0, b_len > 0) {
        (true, true) => a.start.max(b.start) < a.end.min(b.end),
        (false, false) => a.start == b.start,
        (false, true) => b.start < a.start && a.start < b.end,
        (true, false) => a.start < b.start && b.start < a.end,
    }
}

fn make_components(changes: &[Change]) -> Vec<Component> {
    let mut visited = vec![false; changes.len()];
    let mut components = Vec::new();

    for seed in 0..changes.len() {
        if visited[seed] {
            continue;
        }

        visited[seed] = true;
        let mut stack = vec![seed];
        let mut indices = Vec::new();

        while let Some(i) = stack.pop() {
            indices.push(i);
            for j in 0..changes.len() {
                if !visited[j] && interacts(&changes[i], &changes[j]) {
                    visited[j] = true;
                    stack.push(j);
                }
            }
        }

        let members: Vec<Change> = indices.iter().map(|&i| changes[i].clone()).collect();
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
            .then_with(|| (a.end > a.start).cmp(&(b.end > b.start)))
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

    let mut result = Vec::new();
    let mut cursor = component.start;

    for change in changes {
        if change.start < cursor {
            continue;
        }
        result.extend(base[cursor..change.start].iter().cloned());
        result.extend(change.replacement.iter().cloned());
        cursor = change.end;
    }
    result.extend(base[cursor..component.end].iter().cloned());
    result
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
        if let Some(byte) = text.bytes().position(|b| b == b'\r') {
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
    let components = make_components(&changes);

    let mut output = Vec::<String>::new();
    let mut conflicts = Vec::new();
    let mut base_cursor = 0usize;
    let mut marked_ordinal = 0usize;

    for component in components {
        output.extend(base_lines[base_cursor..component.start].iter().cloned());

        let base_projection = base_lines[component.start..component.end].to_vec();
        let left_projection = project(&base_lines, &component, 0);
        let right_projection = project(&base_lines, &component, 1);

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
                    marked_ordinal += 1;
                    let ordinal = marked_ordinal;
                    let output_start = output.len();
                    let base_len = component.end - component.start;
                    let base_start_marker = if base_len > 0 {
                        component.start + 1
                    } else {
                        component.start
                    };

                    output.push(format!("<<<<<<< LEFT {}", ordinal));
                    output.extend(left_projection.iter().cloned());
                    output.push(format!(
                        "||||||| BASE {},{}",
                        base_start_marker, base_len
                    ));
                    output.extend(base_projection.iter().cloned());
                    output.push("=======".to_owned());
                    output.extend(right_projection.iter().cloned());
                    output.push(format!(">>>>>>> RIGHT {}", ordinal));

                    conflicts.push(Conflict {
                        ordinal,
                        base_start: component.start,
                        base_len,
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

fn main() {}