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

#[derive(Debug, Clone)]
struct Change {
    start: usize,
    end: usize,
    replacement: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
enum Edit {
    Keep,
    Delete,
    Insert,
}

#[derive(Debug, Clone)]
struct Component {
    start: usize,
    end: usize,
    base_indices: Vec<usize>,
    left_indices: Vec<usize>,
    right_indices: Vec<usize>,
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

fn validate(
    base: &str,
    left: &str,
    right: &str,
) -> Result<(Vec<String>, Vec<String>, Vec<String>), MergeError> {
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

    Ok((base_lines, left_lines, right_lines))
}

fn edit_script(old: &[String], new: &[String]) -> Vec<Edit> {
    let rows = old.len() + 1;
    let cols = new.len() + 1;
    let mut lcs = vec![0usize; rows * cols];

    for i in (0..old.len()).rev() {
        for j in (0..new.len()).rev() {
            let value = if old[i] == new[j] {
                1 + lcs[(i + 1) * cols + j + 1]
            } else {
                lcs[(i + 1) * cols + j].max(lcs[i * cols + j + 1])
            };
            lcs[i * cols + j] = value;
        }
    }

    let (mut i, mut j) = (0usize, 0usize);
    let mut script = Vec::with_capacity(old.len() + new.len());

    while i < old.len() && j < new.len() {
        if old[i] == new[j] {
            script.push(Edit::Keep);
            i += 1;
            j += 1;
        } else {
            let delete_lcs = lcs[(i + 1) * cols + j];
            let insert_lcs = lcs[i * cols + j + 1];
            if delete_lcs > insert_lcs || (delete_lcs == insert_lcs && (i + j) % 2 == 0) {
                script.push(Edit::Delete);
                i += 1;
            } else {
                script.push(Edit::Insert);
                j += 1;
            }
        }
    }

    while i < old.len() {
        script.push(Edit::Delete);
        i += 1;
    }
    while j < new.len() {
        script.push(Edit::Insert);
        j += 1;
    }

    script
}

fn changes_from_script(old: &[String], new: &[String]) -> Vec<Change> {
    let script = edit_script(old, new);
    let (mut old_i, mut new_i) = (0usize, 0usize);
    let mut changes = Vec::new();
    let mut active_start = 0usize;
    let mut active_deletes = 0usize;
    let mut active_replacement = Vec::new();
    let mut active = false;

    for edit in script {
        match edit {
            Edit::Keep => {
                if active {
                    changes.push(Change {
                        start: active_start,
                        end: active_start + active_deletes,
                        replacement: std::mem::take(&mut active_replacement),
                    });
                    active_deletes = 0;
                    active = false;
                }
                old_i += 1;
                new_i += 1;
            }
            Edit::Delete => {
                if !active {
                    active = true;
                    active_start = old_i;
                }
                active_deletes += 1;
                old_i += 1;
            }
            Edit::Insert => {
                if !active {
                    active = true;
                    active_start = old_i;
                }
                active_replacement.push(new[new_i].clone());
                new_i += 1;
            }
        }
    }

    if active {
        changes.push(Change {
            start: active_start,
            end: active_start + active_deletes,
            replacement: active_replacement,
        });
    }

    changes
}

fn interact(a: &Change, b: &Change) -> bool {
    let a_positive = a.start < a.end;
    let b_positive = b.start < b.end;

    match (a_positive, b_positive) {
        (true, true) => a.start.max(b.start) < a.end.min(b.end),
        (false, false) => a.start == b.start,
        (false, true) => b.start < a.start && a.start < b.end,
        (true, false) => a.start < b.start && b.start < a.end,
    }
}

fn build_components(left: &[Change], right: &[Change]) -> Vec<Component> {
    let mut all: Vec<(bool, usize)> = Vec::with_capacity(left.len() + right.len());
    all.extend((0..left.len()).map(|i| (true, i)));
    all.extend((0..right.len()).map(|i| (false, i)));

    let mut parent: Vec<usize> = (0..all.len()).collect();

    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }

    for i in 0..all.len() {
        for j in i + 1..all.len() {
            let (is_left_i, idx_i) = all[i];
            let (is_left_j, idx_j) = all[j];

            let a = if is_left_i {
                &left[idx_i]
            } else {
                &right[idx_i]
            };
            let b = if is_left_j {
                &left[idx_j]
            } else {
                &right[idx_j]
            };

            if interact(a, b) {
                let ri = root(&mut parent, i);
                let rj = root(&mut parent, j);
                if ri != rj {
                    parent[rj] = ri;
                }
            }
        }
    }

    let mut grouped: Vec<(usize, Vec<usize>)> = Vec::new();
    for i in 0..all.len() {
        let r = root(&mut parent, i);
        if let Some((_, indices)) = grouped.iter_mut().find(|(key, _)| *key == r) {
            indices.push(i);
        } else {
            grouped.push((r, vec![i]));
        }
    }

    let mut components = Vec::new();
    for (_, members) in grouped {
        let mut start = usize::MAX;
        let mut end = 0usize;
        let mut base_indices = Vec::new();
        let mut left_indices = Vec::new();
        let mut right_indices = Vec::new();

        for member in members {
            let (is_left, index) = all[member];
            let change = if is_left {
                left_indices.push(index);
                &left[index]
            } else {
                right_indices.push(index);
                &right[index]
            };
            start = start.min(change.start);
            end = end.max(change.end);
            base_indices.push(member);
        }

        left_indices.sort_unstable();
        right_indices.sort_unstable();
        components.push(Component {
            start,
            end,
            base_indices,
            left_indices,
            right_indices,
        });
    }

    components.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then_with(|| (a.start < a.end).cmp(&(b.start < b.end)))
    });
    components
}

fn projection(base: &[String], start: usize, end: usize, changes: &[Change], indices: &[usize]) -> Vec<String> {
    let mut out = Vec::new();
    let mut cursor = start;

    for &index in indices {
        let change = &changes[index];
        if change.start < cursor || change.start > end || change.end > end {
            continue;
        }

        out.extend(base[cursor..change.start].iter().cloned());
        out.extend(change.replacement.iter().cloned());
        cursor = change.end;
    }

    out.extend(base[cursor..end].iter().cloned());
    out
}

pub fn merge_three_way(
    base: &str,
    left: &str,
    right: &str,
    resolution: Resolution,
) -> Result<MergeResult, MergeError> {
    let (base_lines, left_lines, right_lines) = validate(base, left, right)?;
    let left_changes = changes_from_script(&base_lines, &left_lines);
    let right_changes = changes_from_script(&base_lines, &right_lines);
    let components = build_components(&left_changes, &right_changes);

    let mut output_lines = Vec::new();
    let mut regions = Vec::new();
    let mut cursor = 0usize;

    for component in components {
        let start = component.start;
        let end = component.end;

        if start > cursor {
            output_lines.extend(base_lines[cursor..start].iter().cloned());
            cursor = start;
        }

        let base_projection = base_lines[start..end].to_vec();
        let left_projection = projection(
            &base_lines,
            start,
            end,
            &left_changes,
            &component.left_indices,
        );
        let right_projection = projection(
            &base_lines,
            start,
            end,
            &right_changes,
            &component.right_indices,
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
                    let ordinal = regions.len() + 1;
                    let output_start = output_lines.len();
                    let base_len = end - start;
                    let base_marker_start = if base_len > 0 { start + 1 } else { start };

                    output_lines.push(format!("<<<<<<< LEFT {ordinal}"));
                    output_lines.extend(left_projection.iter().cloned());
                    output_lines.push(format!("||||||| BASE {base_marker_start},{base_len}"));
                    output_lines.extend(base_projection.iter().cloned());
                    output_lines.push("=======".to_owned());
                    output_lines.extend(right_projection.iter().cloned());
                    output_lines.push(format!(">>>>>>> RIGHT {ordinal}"));

                    regions.push(Conflict {
                        ordinal,
                        base_start: start,
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

        if end > cursor {
            cursor = end;
        }
    }

    output_lines.extend(base_lines[cursor..].iter().cloned());

    let mut text = String::new();
    for line in &output_lines {
        text.push_str(line);
        text.push('\n');
    }

    Ok(MergeResult {
        text,
        conflicts: regions.len(),
        regions,
    })
}