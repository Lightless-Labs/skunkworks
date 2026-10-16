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
    side: MergeSide,
}

#[derive(Debug, Clone)]
struct Component {
    start: usize,
    end: usize,
    changes: Vec<usize>,
}

fn split_lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }

    text.strip_suffix('\n')
        .unwrap()
        .split('\n')
        .map(str::to_owned)
        .collect()
}

fn validate(
    base: &str,
    left: &str,
    right: &str,
) -> Result<(Vec<String>, Vec<String>, Vec<String>), MergeError> {
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

    Ok((base_lines, left_lines, right_lines))
}

#[derive(Debug, Clone)]
enum Edit {
    Keep,
    Delete,
    Insert(String),
}

fn canonical_script(old: &[String], new: &[String]) -> Vec<Edit> {
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

    let (mut i, mut j) = (0, 0);
    let mut script = Vec::new();

    while i < m && j < n {
        if old[i] == new[j] {
            script.push(Edit::Keep);
            i += 1;
            j += 1;
        } else {
            let del = lcs[i + 1][j];
            let ins = lcs[i][j + 1];
            if del > ins || (del == ins && (i + j) % 2 == 0) {
                script.push(Edit::Delete);
                i += 1;
            } else {
                script.push(Edit::Insert(new[j].clone()));
                j += 1;
            }
        }
    }

    while i < m {
        script.push(Edit::Delete);
        i += 1;
    }
    while j < n {
        script.push(Edit::Insert(new[j].clone()));
        j += 1;
    }

    script
}

fn changes_for(old: &[String], new: &[String], side: MergeSide) -> Vec<Change> {
    let script = canonical_script(old, new);
    let mut changes = Vec::new();
    let mut base_pos = 0usize;
    let mut active_start = None;
    let mut deleted = 0usize;
    let mut replacement = Vec::new();

    for edit in script {
        match edit {
            Edit::Keep => {
                if let Some(start) = active_start.take() {
                    changes.push(Change {
                        start,
                        end: start + deleted,
                        replacement: std::mem::take(&mut replacement),
                        side: side.clone(),
                    });
                    deleted = 0;
                }
                base_pos += 1;
            }
            Edit::Delete => {
                active_start.get_or_insert(base_pos);
                deleted += 1;
                base_pos += 1;
            }
            Edit::Insert(line) => {
                active_start.get_or_insert(base_pos);
                replacement.push(line);
            }
        }
    }

    if let Some(start) = active_start {
        changes.push(Change {
            start,
            end: start + deleted,
            replacement,
            side,
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

fn make_components(changes: &[Change]) -> Vec<Component> {
    let mut parent: Vec<usize> = (0..changes.len()).collect();

    fn root(parent: &mut [usize], x: usize) -> usize {
        if parent[x] != x {
            parent[x] = root(parent, parent[x]);
        }
        parent[x]
    }

    for i in 0..changes.len() {
        for j in (i + 1)..changes.len() {
            if interact(&changes[i], &changes[j]) {
                let ri = root(&mut parent, i);
                let rj = root(&mut parent, j);
                if ri != rj {
                    parent[rj] = ri;
                }
            }
        }
    }

    let mut groups: Vec<(usize, Vec<usize>)> = Vec::new();
    for i in 0..changes.len() {
        let r = root(&mut parent, i);
        if let Some((_, members)) = groups.iter_mut().find(|(id, _)| *id == r) {
            members.push(i);
        } else {
            groups.push((r, vec![i]));
        }
    }

    let mut components = groups
        .into_iter()
        .map(|(_, members)| {
            let start = members.iter().map(|&i| changes[i].start).min().unwrap();
            let end = members.iter().map(|&i| changes[i].end).max().unwrap();
            Component {
                start,
                end,
                changes: members,
            }
        })
        .collect::<Vec<_>>();

    components.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then_with(|| (a.start < a.end).cmp(&(b.start < b.end)))
            .then(a.end.cmp(&b.end))
    });
    components
}

fn projection(base: &[String], component: &Component, changes: &[Change], side: &MergeSide) -> Vec<String> {
    let mut selected = component
        .changes
        .iter()
        .copied()
        .filter(|&i| changes[i].side == *side)
        .collect::<Vec<_>>();
    selected.sort_by_key(|&i| (changes[i].start, changes[i].end));

    let mut output = Vec::new();
    let mut cursor = component.start;
    for i in selected {
        let change = &changes[i];
        if change.start > cursor {
            output.extend(base[cursor..change.start].iter().cloned());
        }
        output.extend(change.replacement.iter().cloned());
        cursor = change.end;
    }
    if cursor < component.end {
        output.extend(base[cursor..component.end].iter().cloned());
    }
    output
}

pub fn merge_three_way(
    base: &str,
    left: &str,
    right: &str,
    resolution: Resolution,
) -> Result<MergeResult, MergeError> {
    let (base_lines, left_lines, right_lines) = validate(base, left, right)?;

    let mut changes = changes_for(&base_lines, &left_lines, MergeSide::Left);
    changes.extend(changes_for(&base_lines, &right_lines, MergeSide::Right));
    let components = make_components(&changes);

    let mut output = Vec::<String>::new();
    let mut records = Vec::<Conflict>::new();
    let mut base_cursor = 0usize;

    for component in components {
        if component.start > base_cursor {
            output.extend(base_lines[base_cursor..component.start].iter().cloned());
        }

        let base_projection = base_lines[component.start..component.end].to_vec();
        let left_projection = projection(&base_lines, &component, &changes, &MergeSide::Left);
        let right_projection = projection(&base_lines, &component, &changes, &MergeSide::Right);

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
                    let ordinal = records.len() + 1;
                    let output_start = output.len();
                    let base_len = component.end - component.start;
                    let base_label_start = if base_len > 0 {
                        component.start + 1
                    } else {
                        component.start
                    };

                    output.push(format!("<<<<<<< LEFT {ordinal}"));
                    output.extend(left_projection.iter().cloned());
                    output.push(format!("||||||| BASE {base_label_start},{base_len}"));
                    output.extend(base_projection.iter().cloned());
                    output.push("=======".to_owned());
                    output.extend(right_projection.iter().cloned());
                    output.push(format!(">>>>>>> RIGHT {ordinal}"));

                    records.push(Conflict {
                        ordinal,
                        base_start: component.start,
                        base_len,
                        output_start,
                        output_len: output.len() - output_start,
                    });
                }
            }
        }

        base_cursor = component.end;
    }

    if base_cursor < base_lines.len() {
        output.extend(base_lines[base_cursor..].iter().cloned());
    }

    let mut text = String::new();
    for line in &output {
        text.push_str(line);
        text.push('\n');
    }

    Ok(MergeResult {
        text,
        conflicts: records.len(),
        regions: records,
    })
}