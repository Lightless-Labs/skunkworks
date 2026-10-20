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
    left: Vec<Change>,
    right: Vec<Change>,
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

    let (mut i, mut j, mut base_pos) = (0usize, 0usize, 0usize);
    let mut changes = Vec::new();
    let mut active_start: Option<usize> = None;
    let mut deletes = 0usize;
    let mut inserted = Vec::new();

    while i < old.len() || j < new.len() {
        if i < old.len() && j < new.len() && old[i] == new[j] {
            if let Some(start) = active_start.take() {
                changes.push(Change {
                    start,
                    end: start + deletes,
                    replacement: std::mem::take(&mut inserted),
                });
                deletes = 0;
            }
            i += 1;
            j += 1;
            base_pos += 1;
            continue;
        }

        let do_delete = if i == old.len() {
            false
        } else if j == new.len() {
            true
        } else {
            let del_score = lcs[i + 1][j];
            let ins_score = lcs[i][j + 1];
            if del_score > ins_score {
                true
            } else if ins_score > del_score {
                false
            } else {
                (i + j) % 2 == 0
            }
        };

        if active_start.is_none() {
            active_start = Some(base_pos);
        }

        if do_delete {
            i += 1;
            base_pos += 1;
            deletes += 1;
        } else {
            inserted.push(new[j].clone());
            j += 1;
        }
    }

    if let Some(start) = active_start {
        changes.push(Change {
            start,
            end: start + deletes,
            replacement: inserted,
        });
    }

    changes
}

fn interact(a: &Change, b: &Change) -> bool {
    let a_empty = a.start == a.end;
    let b_empty = b.start == b.end;
    match (a_empty, b_empty) {
        (false, false) => a.start.max(b.start) < a.end.min(b.end),
        (true, true) => a.start == b.start,
        (true, false) => b.start < a.start && a.start < b.end,
        (false, true) => a.start < b.start && b.start < a.end,
    }
}

fn project(
    base: &[String],
    region_start: usize,
    region_end: usize,
    changes: &[Change],
) -> Vec<String> {
    let mut output = Vec::new();
    let mut cursor = region_start;

    for change in changes {
        while cursor < change.start {
            output.push(base[cursor].clone());
            cursor += 1;
        }
        output.extend(change.replacement.iter().cloned());
        cursor = change.end;
    }

    while cursor < region_end {
        output.push(base[cursor].clone());
        cursor += 1;
    }
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

    let left_changes = edit_script(&base_lines, &left_lines);
    let right_changes = edit_script(&base_lines, &right_lines);

    let mut all: Vec<(bool, usize)> = Vec::new();
    for i in 0..left_changes.len() {
        all.push((true, i));
    }
    for i in 0..right_changes.len() {
        all.push((false, i));
    }

    let mut parent: Vec<usize> = (0..all.len()).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        let mut root = x;
        while parent[root] != root {
            root = parent[root];
        }
        while parent[x] != x {
            let next = parent[x];
            parent[x] = root;
            x = next;
        }
        root
    }

    for i in 0..all.len() {
        for j in i + 1..all.len() {
            let change_i = if all[i].0 {
                &left_changes[all[i].1]
            } else {
                &right_changes[all[i].1]
            };
            let change_j = if all[j].0 {
                &left_changes[all[j].1]
            } else {
                &right_changes[all[j].1]
            };
            if interact(change_i, change_j) {
                let ri = find(&mut parent, i);
                let rj = find(&mut parent, j);
                if ri != rj {
                    parent[rj] = ri;
                }
            }
        }
    }

    let mut components: Vec<Component> = Vec::new();
    let mut roots = Vec::new();
    for i in 0..all.len() {
        let root = find(&mut parent, i);
        let change = if all[i].0 {
            &left_changes[all[i].1]
        } else {
            &right_changes[all[i].1]
        };
        if let Some(pos) = roots.iter().position(|&r| r == root) {
            let component = &mut components[pos];
            component.start = component.start.min(change.start);
            component.end = component.end.max(change.end);
            if all[i].0 {
                component.left.push(change.clone());
            } else {
                component.right.push(change.clone());
            }
        } else {
            roots.push(root);
            components.push(Component {
                start: change.start,
                end: change.end,
                left: if all[i].0 {
                    vec![change.clone()]
                } else {
                    Vec::new()
                },
                right: if all[i].0 {
                    Vec::new()
                } else {
                    vec![change.clone()]
                },
            });
        }
    }

    let mut order: Vec<usize> = (0..components.len()).collect();
    order.sort_by_key(|&i| {
        let c = &components[i];
        (c.start, if c.start == c.end { 0usize } else { 1usize }, c.end)
    });

    let mut output_lines = Vec::new();
    let mut conflicts = Vec::new();
    let mut base_cursor = 0usize;

    for index in order {
        let component = &components[index];
        while base_cursor < component.start {
            output_lines.push(base_lines[base_cursor].clone());
            base_cursor += 1;
        }

        let base_projection = base_lines[component.start..component.end].to_vec();
        let left_projection = project(
            &base_lines,
            component.start,
            component.end,
            &component.left,
        );
        let right_projection = project(
            &base_lines,
            component.start,
            component.end,
            &component.right,
        );

        if left_projection == right_projection {
            append_lines(&mut output_lines, &left_projection);
        } else if left_projection == base_projection {
            append_lines(&mut output_lines, &right_projection);
        } else if right_projection == base_projection {
            append_lines(&mut output_lines, &left_projection);
        } else {
            match resolution {
                Resolution::PreferLeft => append_lines(&mut output_lines, &left_projection),
                Resolution::PreferRight => append_lines(&mut output_lines, &right_projection),
                Resolution::Mark => {
                    let ordinal = conflicts.len() + 1;
                    let output_start = output_lines.len();
                    output_lines.push(format!("<<<<<<< LEFT {ordinal}"));
                    append_lines(&mut output_lines, &left_projection);
                    let region_len = component.end - component.start;
                    let base_marker_start = if region_len > 0 {
                        component.start + 1
                    } else {
                        component.start
                    };
                    output_lines.push(format!(
                        "||||||| BASE {base_marker_start},{region_len}"
                    ));
                    append_lines(&mut output_lines, &base_projection);
                    output_lines.push("=======".to_owned());
                    append_lines(&mut output_lines, &right_projection);
                    output_lines.push(format!(">>>>>>> RIGHT {ordinal}"));
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

    while base_cursor < base_lines.len() {
        output_lines.push(base_lines[base_cursor].clone());
        base_cursor += 1;
    }

    let mut text = String::new();
    for line in output_lines {
        text.push_str(&line);
        text.push('\n');
    }

    Ok(MergeResult {
        conflicts: conflicts.len(),
        text,
        regions: conflicts,
    })
}