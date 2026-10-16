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

#[derive(Debug)]
struct Change {
    start: usize,
    end: usize,
    replacement: Vec<String>,
    side: MergeSide,
}

#[derive(Debug)]
struct Component {
    start: usize,
    end: usize,
    changes: Vec<usize>,
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

fn canonical_changes(base: &[String], other: &[String], side: MergeSide) -> Vec<Change> {
    let n = base.len();
    let m = other.len();
    let mut lcs = vec![vec![0usize; m + 1]; n + 1];

    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if base[i] == other[j] {
                1 + lcs[i + 1][j + 1]
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }

    let mut i = 0;
    let mut j = 0;
    let mut base_pos = 0;
    let mut changes = Vec::new();
    let mut active_start = None;
    let mut deletes = 0;
    let mut inserts = Vec::new();

    let flush = |changes: &mut Vec<Change>,
                 active_start: &mut Option<usize>,
                 deletes: &mut usize,
                 inserts: &mut Vec<String>,
                 base_pos: usize,
                 side: &MergeSide| {
        if let Some(start) = active_start.take() {
            changes.push(Change {
                start,
                end: start + *deletes,
                replacement: std::mem::take(inserts),
                side: side.clone(),
            });
            *deletes = 0;
        } else {
            let _ = base_pos;
        }
    };

    while i < n || j < m {
        if i < n && j < m && base[i] == other[j] {
            flush(
                &mut changes,
                &mut active_start,
                &mut deletes,
                &mut inserts,
                base_pos,
                &side,
            );
            i += 1;
            j += 1;
            base_pos += 1;
            continue;
        }

        let delete = if j == m {
            true
        } else if i == n {
            false
        } else {
            let del_len = lcs[i + 1][j];
            let ins_len = lcs[i][j + 1];
            if del_len > ins_len {
                true
            } else if ins_len > del_len {
                false
            } else {
                (i + j) % 2 == 0
            }
        };

        active_start.get_or_insert(base_pos);
        if delete {
            i += 1;
            base_pos += 1;
            deletes += 1;
        } else {
            inserts.push(other[j].clone());
            j += 1;
        }
    }

    flush(
        &mut changes,
        &mut active_start,
        &mut deletes,
        &mut inserts,
        base_pos,
        &side,
    );
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

fn make_components(changes: &[Change]) -> Vec<Component> {
    let mut visited = vec![false; changes.len()];
    let mut components = Vec::new();

    for root in 0..changes.len() {
        if visited[root] {
            continue;
        }
        visited[root] = true;
        let mut stack = vec![root];
        let mut members = Vec::new();

        while let Some(current) = stack.pop() {
            members.push(current);
            for candidate in 0..changes.len() {
                if !visited[candidate] && interacts(&changes[current], &changes[candidate]) {
                    visited[candidate] = true;
                    stack.push(candidate);
                }
            }
        }

        let start = members
            .iter()
            .map(|&idx| changes[idx].start)
            .min()
            .unwrap();
        let end = members
            .iter()
            .map(|&idx| changes[idx].end)
            .max()
            .unwrap();
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
    });
    components
}

fn project(
    base: &[String],
    component: &Component,
    changes: &[Change],
    side: MergeSide,
) -> Vec<String> {
    let mut side_changes: Vec<&Change> = component
        .changes
        .iter()
        .map(|&idx| &changes[idx])
        .filter(|change| change.side == side)
        .collect();
    side_changes.sort_by_key(|change| (change.start, change.end));

    let mut result = Vec::new();
    let mut cursor = component.start;
    for change in side_changes {
        result.extend(base[cursor..change.start].iter().cloned());
        result.extend(change.replacement.iter().cloned());
        cursor = change.end;
    }
    result.extend(base[cursor..component.end].iter().cloned());
    result
}

fn emit(lines: &[String], output: &mut Vec<String>) {
    output.extend(lines.iter().cloned());
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

    let mut changes = canonical_changes(&base_lines, &left_lines, MergeSide::Left);
    changes.extend(canonical_changes(
        &base_lines,
        &right_lines,
        MergeSide::Right,
    ));
    let components = make_components(&changes);

    let mut output = Vec::<String>::new();
    let mut conflicts = Vec::new();
    let mut base_cursor = 0;

    for component in components {
        emit(&base_lines[base_cursor..component.start], &mut output);

        let base_projection = base_lines[component.start..component.end].to_vec();
        let left_projection = project(
            &base_lines,
            &component,
            &changes,
            MergeSide::Left,
        );
        let right_projection = project(
            &base_lines,
            &component,
            &changes,
            MergeSide::Right,
        );

        if left_projection == right_projection {
            emit(&left_projection, &mut output);
        } else if left_projection == base_projection {
            emit(&right_projection, &mut output);
        } else if right_projection == base_projection {
            emit(&left_projection, &mut output);
        } else {
            match resolution {
                Resolution::PreferLeft => emit(&left_projection, &mut output),
                Resolution::PreferRight => emit(&right_projection, &mut output),
                Resolution::Mark => {
                    let ordinal = conflicts.len() + 1;
                    let output_start = output.len();
                    output.push(format!("<<<<<<< LEFT {ordinal}"));
                    emit(&left_projection, &mut output);
                    let base_label = if component.start < component.end {
                        component.start + 1
                    } else {
                        component.start
                    };
                    output.push(format!(
                        "||||||| BASE {base_label},{}",
                        component.end - component.start
                    ));
                    emit(&base_projection, &mut output);
                    output.push("=======".to_owned());
                    emit(&right_projection, &mut output);
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

    emit(&base_lines[base_cursor..], &mut output);
    let text = output
        .into_iter()
        .map(|line| {
            let mut s = line;
            s.push('\n');
            s
        })
        .collect();

    Ok(MergeResult {
        text,
        conflicts: conflicts.len(),
        regions: conflicts,
    })
}