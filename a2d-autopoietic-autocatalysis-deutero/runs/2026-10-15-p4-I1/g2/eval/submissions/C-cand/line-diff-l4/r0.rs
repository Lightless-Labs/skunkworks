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

#[derive(Clone)]
struct Component {
    changes: Vec<Change>,
    start: usize,
    end: usize,
}

fn lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    text.strip_suffix('\n')
        .unwrap()
        .split('\n')
        .map(str::to_owned)
        .collect()
}

fn validate_cr(text: &str, side: MergeSide) -> Result<(), MergeError> {
    if let Some(byte) = text.as_bytes().iter().position(|&b| b == b'\r') {
        Err(MergeError::CarriageReturn { side, byte })
    } else {
        Ok(())
    }
}

fn edit_changes(base: &[String], variant: &[String], side: usize) -> Vec<Change> {
    let n = base.len();
    let m = variant.len();
    let mut lcs = vec![vec![0usize; m + 1]; n + 1];

    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if base[i] == variant[j] {
                1 + lcs[i + 1][j + 1]
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }

    let mut changes = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    let mut run_start = None;
    let mut deleted = 0usize;
    let mut inserted = Vec::new();

    let finish_run = |changes: &mut Vec<Change>,
                      run_start: &mut Option<usize>,
                      deleted: &mut usize,
                      inserted: &mut Vec<String>,
                      i: usize| {
        if let Some(start) = run_start.take() {
            changes.push(Change {
                start,
                end: start + *deleted,
                replacement: std::mem::take(inserted),
                side,
            });
            *deleted = 0;
        }
        let _ = i;
    };

    while i < n || j < m {
        if i < n && j < m && base[i] == variant[j] {
            finish_run(&mut changes, &mut run_start, &mut deleted, &mut inserted, i);
            i += 1;
            j += 1;
            continue;
        }

        if run_start.is_none() {
            run_start = Some(i);
        }

        if i == n {
            inserted.push(variant[j].clone());
            j += 1;
        } else if j == m {
            deleted += 1;
            i += 1;
        } else {
            let delete_score = lcs[i + 1][j];
            let insert_score = lcs[i][j + 1];
            let take_delete = if delete_score > insert_score {
                true
            } else if insert_score > delete_score {
                false
            } else {
                (i + j) % 2 == 0
            };

            if take_delete {
                deleted += 1;
                i += 1;
            } else {
                inserted.push(variant[j].clone());
                j += 1;
            }
        }
    }

    finish_run(&mut changes, &mut run_start, &mut deleted, &mut inserted, i);
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

fn make_components(changes: Vec<Change>) -> Vec<Component> {
    let count = changes.len();
    let mut seen = vec![false; count];
    let mut components = Vec::new();

    for seed in 0..count {
        if seen[seed] {
            continue;
        }
        seen[seed] = true;
        let mut stack = vec![seed];
        let mut members = Vec::new();

        while let Some(index) = stack.pop() {
            members.push(changes[index].clone());
            for other in 0..count {
                if !seen[other] && interact(&changes[index], &changes[other]) {
                    seen[other] = true;
                    stack.push(other);
                }
            }
        }

        let start = members.iter().map(|c| c.start).min().unwrap();
        let end = members.iter().map(|c| c.end).max().unwrap();
        components.push(Component {
            changes: members,
            start,
            end,
        });
    }

    components.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then_with(|| (a.start < a.end).cmp(&(b.start < b.end)))
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

    let mut output = Vec::new();
    let mut cursor = component.start;

    for change in changes {
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
    let components = make_components(changes);

    let mut output_lines = Vec::new();
    let mut conflicts = Vec::new();
    let mut cursor = 0usize;

    for component in components {
        if cursor < component.start {
            output_lines.extend(base_lines[cursor..component.start].iter().cloned());
        }

        let base_projection = base_lines[component.start..component.end].to_vec();
        let left_projection = project(&base_lines, &component, 0);
        let right_projection = project(&base_lines, &component, 1);

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
                    let ordinal = conflicts.len() + 1;
                    let output_start = output_lines.len();
                    let output_len =
                        4 + left_projection.len() + base_projection.len() + right_projection.len();
                    let base_start_label = if component.start < component.end {
                        component.start + 1
                    } else {
                        component.start
                    };

                    output_lines.push(format!("<<<<<<< LEFT {ordinal}"));
                    output_lines.extend(left_projection);
                    output_lines.push(format!(
                        "||||||| BASE {base_start_label},{}",
                        component.end - component.start
                    ));
                    output_lines.extend(base_projection);
                    output_lines.push("=======".to_owned());
                    output_lines.extend(right_projection);
                    output_lines.push(format!(">>>>>>> RIGHT {ordinal}"));

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

        cursor = component.end;
    }

    if cursor < base_lines.len() {
        output_lines.extend(base_lines[cursor..].iter().cloned());
    }

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