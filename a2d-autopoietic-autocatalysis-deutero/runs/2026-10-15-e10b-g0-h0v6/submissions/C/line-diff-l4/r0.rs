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
struct Change<'a> {
    start: usize,
    end: usize,
    replacement: Vec<&'a str>,
    side: MergeSide,
}

#[derive(Clone)]
struct Component {
    start: usize,
    end: usize,
    changes: Vec<usize>,
}

#[derive(Clone, Copy)]
enum Op<'a> {
    Keep,
    Delete,
    Insert(&'a str),
}

fn split_lines(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return Vec::new();
    }
    text[..text.len() - 1].split('\n').collect()
}

fn edit_script<'a>(old: &[&'a str], new: &[&'a str]) -> Vec<Op<'a>> {
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

    let mut script = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < old.len() && j < new.len() {
        if old[i] == new[j] {
            script.push(Op::Keep);
            i += 1;
            j += 1;
        } else {
            let delete_len = lcs[(i + 1) * cols + j];
            let insert_len = lcs[i * cols + j + 1];
            if delete_len > insert_len || (delete_len == insert_len && (i + j) % 2 == 0) {
                script.push(Op::Delete);
                i += 1;
            } else {
                script.push(Op::Insert(new[j]));
                j += 1;
            }
        }
    }
    while i < old.len() {
        script.push(Op::Delete);
        i += 1;
    }
    while j < new.len() {
        script.push(Op::Insert(new[j]));
        j += 1;
    }
    script
}

fn changes_from_script<'a>(
    old: &[&'a str],
    new_script: &[Op<'a>],
    side: MergeSide,
) -> Vec<Change<'a>> {
    let mut changes = Vec::new();
    let mut old_pos = 0usize;
    let mut pos = 0usize;

    while pos < new_script.len() {
        if matches!(new_script[pos], Op::Keep) {
            old_pos += 1;
            pos += 1;
            continue;
        }

        let start = old_pos;
        let mut deletes = 0usize;
        let mut replacement = Vec::new();
        while pos < new_script.len() && !matches!(new_script[pos], Op::Keep) {
            match new_script[pos] {
                Op::Delete => {
                    deletes += 1;
                    old_pos += 1;
                }
                Op::Insert(line) => replacement.push(line),
                Op::Keep => unreachable!(),
            }
            pos += 1;
        }
        changes.push(Change {
            start,
            end: start + deletes,
            replacement,
            side: side.clone(),
        });
    }

    debug_assert_eq!(old_pos, old.len());
    changes
}

fn interacts(a: &Change<'_>, b: &Change<'_>) -> bool {
    let a_empty = a.start == a.end;
    let b_empty = b.start == b.end;

    match (a_empty, b_empty) {
        (true, true) => a.start == b.start,
        (false, false) => a.start.max(b.start) < a.end.min(b.end),
        (true, false) => b.start < a.start && a.start < b.end,
        (false, true) => a.start < b.start && b.start < a.end,
    }
}

fn build_components(changes: &[Change<'_>]) -> Vec<Component> {
    let mut seen = vec![false; changes.len()];
    let mut components = Vec::new();

    for root in 0..changes.len() {
        if seen[root] {
            continue;
        }
        seen[root] = true;
        let mut stack = vec![root];
        let mut members = Vec::new();

        while let Some(current) = stack.pop() {
            members.push(current);
            for other in 0..changes.len() {
                if !seen[other] && interacts(&changes[current], &changes[other]) {
                    seen[other] = true;
                    stack.push(other);
                }
            }
        }

        let start = members.iter().map(|&i| changes[i].start).min().unwrap();
        let end = members.iter().map(|&i| changes[i].end).max().unwrap();
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

fn project<'a>(
    base: &[&'a str],
    component: &Component,
    changes: &[Change<'a>],
    side: MergeSide,
) -> Vec<&'a str> {
    let mut selected: Vec<&Change<'a>> = component
        .changes
        .iter()
        .map(|&i| &changes[i])
        .filter(|change| change.side == side)
        .collect();
    selected.sort_by_key(|change| (change.start, change.end));

    let mut output = Vec::new();
    let mut cursor = component.start;
    for change in selected {
        output.extend_from_slice(&base[cursor..change.start]);
        output.extend(change.replacement.iter().copied());
        cursor = change.end;
    }
    output.extend_from_slice(&base[cursor..component.end]);
    output
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

    let left_script = edit_script(&base_lines, &left_lines);
    let right_script = edit_script(&base_lines, &right_lines);
    let mut changes = changes_from_script(&base_lines, &left_script, MergeSide::Left);
    changes.extend(changes_from_script(
        &base_lines,
        &right_script,
        MergeSide::Right,
    ));
    let components = build_components(&changes);

    let mut output_lines: Vec<String> = Vec::new();
    let mut conflicts = Vec::new();
    let mut base_cursor = 0usize;

    for component in &components {
        output_lines.extend(base_lines[base_cursor..component.start].iter().map(|s| (*s).to_owned()));

        let base_projection: Vec<&str> =
            base_lines[component.start..component.end].to_vec();
        let left_projection = project(&base_lines, component, &changes, MergeSide::Left);
        let right_projection = project(&base_lines, component, &changes, MergeSide::Right);

        if left_projection == right_projection {
            output_lines.extend(left_projection.iter().map(|s| (*s).to_owned()));
        } else if left_projection == base_projection {
            output_lines.extend(right_projection.iter().map(|s| (*s).to_owned()));
        } else if right_projection == base_projection {
            output_lines.extend(left_projection.iter().map(|s| (*s).to_owned()));
        } else {
            match resolution {
                Resolution::PreferLeft => {
                    output_lines.extend(left_projection.iter().map(|s| (*s).to_owned()));
                }
                Resolution::PreferRight => {
                    output_lines.extend(right_projection.iter().map(|s| (*s).to_owned()));
                }
                Resolution::Mark => {
                    let ordinal = conflicts.len() + 1;
                    let output_start = output_lines.len();
                    let base_len = component.end - component.start;
                    let base_start_display = if base_len > 0 {
                        component.start + 1
                    } else {
                        component.start
                    };

                    output_lines.push(format!("<<<<<<< LEFT {ordinal}"));
                    output_lines.extend(left_projection.iter().map(|s| (*s).to_owned()));
                    output_lines.push(format!(
                        "||||||| BASE {base_start_display},{base_len}"
                    ));
                    output_lines.extend(base_projection.iter().map(|s| (*s).to_owned()));
                    output_lines.push("=======".to_owned());
                    output_lines.extend(right_projection.iter().map(|s| (*s).to_owned()));
                    output_lines.push(format!(">>>>>>> RIGHT {ordinal}"));

                    conflicts.push(Conflict {
                        ordinal,
                        base_start: component.start,
                        base_len,
                        output_start,
                        output_len: output_lines.len() - output_start,
                    });
                }
            }
        }

        base_cursor = component.end;
    }
    output_lines.extend(base_lines[base_cursor..].iter().map(|s| (*s).to_owned()));

    let mut text = String::new();
    for line in &output_lines {
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