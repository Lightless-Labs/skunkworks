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
    side: MergeSide,
}

#[derive(Clone)]
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

fn edit_changes(base: &[String], variant: &[String], side: MergeSide) -> Vec<Change> {
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

    let mut i = 0;
    let mut j = 0;
    let mut changes = Vec::new();
    let mut active_start = None;
    let mut deleted = 0usize;
    let mut inserted = Vec::new();

    let finish_change = |changes: &mut Vec<Change>,
                         active_start: &mut Option<usize>,
                         deleted: &mut usize,
                         inserted: &mut Vec<String>| {
        if let Some(start) = active_start.take() {
            changes.push(Change {
                start,
                end: start + *deleted,
                replacement: std::mem::take(inserted),
                side: side.clone(),
            });
            *deleted = 0;
        }
    };

    while i < n || j < m {
        if i < n && j < m && base[i] == variant[j] {
            finish_change(
                &mut changes,
                &mut active_start,
                &mut deleted,
                &mut inserted,
            );
            i += 1;
            j += 1;
            continue;
        }

        if active_start.is_none() {
            active_start = Some(i);
        }

        if i == n {
            inserted.push(variant[j].clone());
            j += 1;
        } else if j == m {
            deleted += 1;
            i += 1;
        } else {
            let del_score = lcs[i + 1][j];
            let ins_score = lcs[i][j + 1];
            let choose_delete = if del_score > ins_score {
                true
            } else if ins_score > del_score {
                false
            } else {
                (i + j) % 2 == 0
            };

            if choose_delete {
                deleted += 1;
                i += 1;
            } else {
                inserted.push(variant[j].clone());
                j += 1;
            }
        }
    }

    finish_change(
        &mut changes,
        &mut active_start,
        &mut deleted,
        &mut inserted,
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

fn project(base: &[String], start: usize, end: usize, changes: &[&Change]) -> Vec<String> {
    if changes.is_empty() {
        return base[start..end].to_vec();
    }

    let mut ordered = changes.to_vec();
    ordered.sort_by_key(|change| (change.start, change.end));

    let mut result = Vec::new();
    let mut cursor = start;

    for change in ordered {
        if change.start > cursor {
            result.extend_from_slice(&base[cursor..change.start]);
        }
        result.extend(change.replacement.iter().cloned());
        cursor = change.end;
    }

    if cursor < end {
        result.extend_from_slice(&base[cursor..end]);
    }

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

    let mut changes = edit_changes(&base_lines, &left_lines, MergeSide::Left);
    changes.extend(edit_changes(
        &base_lines,
        &right_lines,
        MergeSide::Right,
    ));

    let count = changes.len();
    let mut parent: Vec<usize> = (0..count).collect();

    fn root(parent: &mut [usize], x: usize) -> usize {
        if parent[x] != x {
            parent[x] = root(parent, parent[x]);
        }
        parent[x]
    }

    for i in 0..count {
        for j in i + 1..count {
            if interacts(&changes[i], &changes[j]) {
                let ri = root(&mut parent, i);
                let rj = root(&mut parent, j);
                if ri != rj {
                    parent[rj] = ri;
                }
            }
        }
    }

    let mut grouped: Vec<(usize, usize, Vec<usize>)> = Vec::new();
    for i in 0..count {
        let r = root(&mut parent, i);
        if let Some(group) = grouped.iter_mut().find(|g| g.2.first().is_some_and(|&idx| root(&mut parent, idx) == r)) {
            group.0 = group.0.min(changes[i].start);
            group.1 = group.1.max(changes[i].end);
            group.2.push(i);
        } else {
            grouped.push((changes[i].start, changes[i].end, vec![i]));
        }
    }

    let mut components: Vec<Component> = grouped
        .into_iter()
        .map(|(start, end, changes)| Component {
            start,
            end,
            changes,
        })
        .collect();

    components.sort_by_key(|c| (c.start, if c.start == c.end { 0 } else { 1 }, c.end));

    let mut output_lines = Vec::new();
    let mut regions = Vec::new();
    let mut base_cursor = 0usize;

    for component in components {
        if component.start > base_cursor {
            append_lines(&mut output_lines, &base_lines[base_cursor..component.start]);
        }

        let component_changes: Vec<&Change> = component
            .changes
            .iter()
            .map(|&idx| &changes[idx])
            .collect();
        let left_changes: Vec<&Change> = component_changes
            .iter()
            .copied()
            .filter(|change| change.side == MergeSide::Left)
            .collect();
        let right_changes: Vec<&Change> = component_changes
            .iter()
            .copied()
            .filter(|change| change.side == MergeSide::Right)
            .collect();

        let base_projection = base_lines[component.start..component.end].to_vec();
        let left_projection = project(
            &base_lines,
            component.start,
            component.end,
            &left_changes,
        );
        let right_projection = project(
            &base_lines,
            component.start,
            component.end,
            &right_changes,
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
                    let ordinal = regions.len() + 1;
                    let output_start = output_lines.len();
                    output_lines.push(format!("<<<<<<< LEFT {ordinal}"));
                    append_lines(&mut output_lines, &left_projection);
                    let base_label = if component.start < component.end {
                        component.start + 1
                    } else {
                        component.start
                    };
                    output_lines.push(format!(
                        "||||||| BASE {base_label},{}",
                        component.end - component.start
                    ));
                    append_lines(&mut output_lines, &base_projection);
                    output_lines.push("=======".to_owned());
                    append_lines(&mut output_lines, &right_projection);
                    output_lines.push(format!(">>>>>>> RIGHT {ordinal}"));
                    let output_len = output_lines.len() - output_start;

                    regions.push(Conflict {
                        ordinal,
                        base_start: component.start,
                        base_len: component.end - component.start,
                        output_start,
                        output_len,
                    });
                }
            }
        }

        base_cursor = base_cursor.max(component.end);
    }

    if base_cursor < base_lines.len() {
        append_lines(&mut output_lines, &base_lines[base_cursor..]);
    }

    let text = if output_lines.is_empty() {
        String::new()
    } else {
        let mut text = output_lines.join("\n");
        text.push('\n');
        text
    };

    Ok(MergeResult {
        text,
        conflicts: regions.len(),
        regions,
    })
}

fn main() {}