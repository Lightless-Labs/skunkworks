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
    left_changes: Vec<Change>,
    right_changes: Vec<Change>,
}

#[derive(Debug, Clone)]
enum Edit {
    Keep,
    Delete,
    Insert(String),
}

fn lines(input: &str) -> Vec<String> {
    if input.is_empty() {
        return Vec::new();
    }

    input
        .strip_suffix('\n')
        .unwrap_or(input)
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
        (MergeSide::Base, base),
        (MergeSide::Left, left),
        (MergeSide::Right, right),
    ];

    for (side, input) in &inputs {
        if let Some(byte) = input.as_bytes().iter().position(|&b| b == b'\r') {
            return Err(MergeError::CarriageReturn {
                side: side.clone(),
                byte,
            });
        }
    }

    for (side, input) in &inputs {
        if !input.is_empty() && !input.ends_with('\n') {
            return Err(MergeError::MissingFinalLf { side: side.clone() });
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

    Ok((base_lines, left_lines, right_lines))
}

fn canonical_edits(old: &[String], new: &[String]) -> Vec<Edit> {
    let n = old.len();
    let m = new.len();
    let mut lcs = vec![vec![0usize; m + 1]; n + 1];

    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if old[i] == new[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }

    let mut edits = Vec::with_capacity(n + m);
    let (mut i, mut j) = (0, 0);

    while i < n || j < m {
        if i == n {
            edits.push(Edit::Insert(new[j].clone()));
            j += 1;
        } else if j == m {
            edits.push(Edit::Delete);
            i += 1;
        } else if old[i] == new[j] {
            edits.push(Edit::Keep);
            i += 1;
            j += 1;
        } else {
            let delete_len = lcs[i + 1][j];
            let insert_len = lcs[i][j + 1];
            if delete_len > insert_len || (delete_len == insert_len && (i + j) % 2 == 0) {
                edits.push(Edit::Delete);
                i += 1;
            } else {
                edits.push(Edit::Insert(new[j].clone()));
                j += 1;
            }
        }
    }

    edits
}

fn changes_from_edits(edits: &[Edit]) -> Vec<Change> {
    let mut changes = Vec::new();
    let mut base_pos = 0;
    let mut index = 0;

    while index < edits.len() {
        if matches!(edits[index], Edit::Keep) {
            base_pos += 1;
            index += 1;
            continue;
        }

        let start = base_pos;
        let mut deleted = 0;
        let mut replacement = Vec::new();

        while index < edits.len() && !matches!(edits[index], Edit::Keep) {
            match &edits[index] {
                Edit::Delete => {
                    deleted += 1;
                    base_pos += 1;
                }
                Edit::Insert(line) => replacement.push(line.clone()),
                Edit::Keep => unreachable!(),
            }
            index += 1;
        }

        changes.push(Change {
            start,
            end: start + deleted,
            replacement,
        });
    }

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

fn build_components(left: &[Change], right: &[Change]) -> Vec<Component> {
    let mut all: Vec<(bool, usize)> = Vec::new();
    for i in 0..left.len() {
        all.push((true, i));
    }
    for i in 0..right.len() {
        all.push((false, i));
    }

    let count = all.len();
    let mut parent: Vec<usize> = (0..count).collect();

    fn root(parent: &mut [usize], x: usize) -> usize {
        if parent[x] != x {
            let r = root(parent, parent[x]);
            parent[x] = r;
        }
        parent[x]
    }

    for i in 0..count {
        for j in (i + 1)..count {
            let (left_i, idx_i) = all[i];
            let (left_j, idx_j) = all[j];
            let a = if left_i { &left[idx_i] } else { &right[idx_i] };
            let b = if left_j { &left[idx_j] } else { &right[idx_j] };

            if interacts(a, b) {
                let ri = root(&mut parent, i);
                let rj = root(&mut parent, j);
                if ri != rj {
                    parent[rj] = ri;
                }
            }
        }
    }

    let mut groups: Vec<Vec<usize>> = Vec::new();
    for i in 0..count {
        let r = root(&mut parent, i);
        if let Some(pos) = groups.iter().position(|g| root(&mut parent, g[0]) == r) {
            groups[pos].push(i);
        } else {
            groups.push(vec![i]);
        }
    }

    let mut components = Vec::new();
    for group in groups {
        let mut component = Component {
            start: usize::MAX,
            end: 0,
            left_changes: Vec::new(),
            right_changes: Vec::new(),
        };

        for id in group {
            let (is_left, index) = all[id];
            let change = if is_left {
                left[index].clone()
            } else {
                right[index].clone()
            };
            component.start = component.start.min(change.start);
            component.end = component.end.max(change.end);
            if is_left {
                component.left_changes.push(change);
            } else {
                component.right_changes.push(change);
            }
        }

        component.left_changes.sort_by_key(|c| (c.start, c.end));
        component.right_changes.sort_by_key(|c| (c.start, c.end));
        components.push(component);
    }

    components.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then_with(|| (a.start != a.end).cmp(&(b.start != b.end)))
            .then_with(|| a.end.cmp(&b.end))
    });

    components
}

fn projection(base: &[String], start: usize, end: usize, changes: &[Change]) -> Vec<String> {
    let mut out = Vec::new();
    let mut cursor = start;

    for change in changes {
        if cursor < change.start {
            out.extend(base[cursor..change.start].iter().cloned());
        }
        out.extend(change.replacement.iter().cloned());
        cursor = change.end;
    }

    if cursor < end {
        out.extend(base[cursor..end].iter().cloned());
    }

    out
}

fn append_lines(output: &mut Vec<String>, lines: &[String]) {
    output.extend(lines.iter().cloned());
}

fn serialize(lines: &[String]) -> String {
    if lines.is_empty() {
        String::new()
    } else {
        let mut result = lines.join("\n");
        result.push('\n');
        result
    }
}

pub fn merge_three_way(
    base: &str,
    left: &str,
    right: &str,
    resolution: Resolution,
) -> Result<MergeResult, MergeError> {
    let (base_lines, left_lines, right_lines) = validate(base, left, right)?;

    let left_changes = changes_from_edits(&canonical_edits(&base_lines, &left_lines));
    let right_changes = changes_from_edits(&canonical_edits(&base_lines, &right_lines));
    let components = build_components(&left_changes, &right_changes);

    let mut output = Vec::new();
    let mut regions = Vec::new();
    let mut cursor = 0;

    for component in components {
        if cursor < component.start {
            append_lines(&mut output, &base_lines[cursor..component.start]);
        }

        let base_projection = base_lines[component.start..component.end].to_vec();
        let left_projection = projection(
            &base_lines,
            component.start,
            component.end,
            &component.left_changes,
        );
        let right_projection = projection(
            &base_lines,
            component.start,
            component.end,
            &component.right_changes,
        );

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
                    let ordinal = regions.len() + 1;
                    let output_start = output.len();
                    output.push(format!("<<<<<<< LEFT {}", ordinal));
                    append_lines(&mut output, &left_projection);

                    let base_label_start = if base_projection.is_empty() {
                        component.start
                    } else {
                        component.start + 1
                    };
                    output.push(format!(
                        "||||||| BASE {},{}",
                        base_label_start,
                        base_projection.len()
                    ));
                    append_lines(&mut output, &base_projection);
                    output.push("=======".to_owned());
                    append_lines(&mut output, &right_projection);
                    output.push(format!(">>>>>>> RIGHT {}", ordinal));

                    regions.push(Conflict {
                        ordinal,
                        base_start: component.start,
                        base_len: component.end - component.start,
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

    if cursor < base_lines.len() {
        append_lines(&mut output, &base_lines[cursor..]);
    }

    Ok(MergeResult {
        text: serialize(&output),
        conflicts: regions.len(),
        regions,
    })
}

fn main() {}