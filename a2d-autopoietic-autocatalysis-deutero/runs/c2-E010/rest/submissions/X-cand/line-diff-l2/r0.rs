#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Side {
    Old,
    New,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    Keep { old_index: usize, new_index: usize, line: String },
    Delete { old_index: usize, line: String },
    Insert { new_index: usize, line: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffError {
    ContextTooLarge { context: usize },
    CarriageReturn { side: Side, byte: usize },
    MissingFinalLf { side: Side },
    TooLarge { old_len: usize, new_len: usize },
}

fn first_cr(s: &str) -> Option<usize> {
    s.as_bytes().iter().position(|&b| b == b'\r')
}

fn lines(s: &str) -> Vec<&str> {
    if s.is_empty() {
        return Vec::new();
    }
    let body = s.strip_suffix('\n').unwrap_or(s);
    body.split('\n').collect()
}

fn canonical_script(old: &[&str], new: &[&str]) -> Vec<Edit> {
    let m = old.len();
    let n = new.len();
    let width = n + 1;
    let mut lcs = vec![0usize; (m + 1) * (n + 1)];

    for i in (0..m).rev() {
        for j in (0..n).rev() {
            let value = if old[i] == new[j] {
                1 + lcs[(i + 1) * width + j + 1]
            } else {
                lcs[(i + 1) * width + j].max(lcs[i * width + j + 1])
            };
            lcs[i * width + j] = value;
        }
    }

    let mut edits = Vec::with_capacity(m + n);
    let (mut i, mut j) = (0, 0);
    while i < m && j < n {
        if old[i] == new[j] {
            edits.push(Edit::Keep {
                old_index: i,
                new_index: j,
                line: old[i].to_owned(),
            });
            i += 1;
            j += 1;
        } else {
            let delete_score = lcs[(i + 1) * width + j];
            let insert_score = lcs[i * width + j + 1];
            if delete_score > insert_score
                || (delete_score == insert_score && (i + j) % 2 == 0)
            {
                edits.push(Edit::Delete {
                    old_index: i,
                    line: old[i].to_owned(),
                });
                i += 1;
            } else {
                edits.push(Edit::Insert {
                    new_index: j,
                    line: new[j].to_owned(),
                });
                j += 1;
            }
        }
    }

    while i < m {
        edits.push(Edit::Delete {
            old_index: i,
            line: old[i].to_owned(),
        });
        i += 1;
    }
    while j < n {
        edits.push(Edit::Insert {
            new_index: j,
            line: new[j].to_owned(),
        });
        j += 1;
    }
    edits
}

fn row_is_change(edit: &Edit) -> bool {
    !matches!(edit, Edit::Keep { .. })
}

fn consumed_before(edits: &[Edit], end: usize) -> (usize, usize) {
    let mut old = 0;
    let mut new = 0;
    for edit in &edits[..end] {
        match edit {
            Edit::Keep { .. } => {
                old += 1;
                new += 1;
            }
            Edit::Delete { .. } => old += 1,
            Edit::Insert { .. } => new += 1,
        }
    }
    (old, new)
}

fn render_hunk(out: &mut String, edits: &[Edit], start: usize, end: usize) {
    let (old_before, new_before) = consumed_before(edits, start);
    let mut old_count = 0usize;
    let mut new_count = 0usize;

    for edit in &edits[start..end] {
        match edit {
            Edit::Keep { .. } => {
                old_count += 1;
                new_count += 1;
            }
            Edit::Delete { .. } => old_count += 1,
            Edit::Insert { .. } => new_count += 1,
        }
    }

    let old_start = if old_count > 0 { old_before + 1 } else { old_before };
    let new_start = if new_count > 0 { new_before + 1 } else { new_before };
    out.push_str(&format!(
        "@@ -{old_start},{old_count} +{new_start},{new_count} @@\n"
    ));

    for edit in &edits[start..end] {
        let (prefix, line) = match edit {
            Edit::Keep { line, .. } => (' ', line),
            Edit::Delete { line, .. } => ('-', line),
            Edit::Insert { line, .. } => ('+', line),
        };
        out.push(prefix);
        out.push_str(line);
        out.push('\n');
    }
}

pub fn unified_diff(old: &str, new: &str, context: usize) -> Result<String, DiffError> {
    if context > 8 {
        return Err(DiffError::ContextTooLarge { context });
    }
    if let Some(byte) = first_cr(old) {
        return Err(DiffError::CarriageReturn { side: Side::Old, byte });
    }
    if let Some(byte) = first_cr(new) {
        return Err(DiffError::CarriageReturn { side: Side::New, byte });
    }
    if !old.is_empty() && !old.ends_with('\n') {
        return Err(DiffError::MissingFinalLf { side: Side::Old });
    }
    if !new.is_empty() && !new.ends_with('\n') {
        return Err(DiffError::MissingFinalLf { side: Side::New });
    }

    let old_lines = lines(old);
    let new_lines = lines(new);
    if old_lines.len() > 128 || new_lines.len() > 128 {
        return Err(DiffError::TooLarge {
            old_len: old_lines.len(),
            new_len: new_lines.len(),
        });
    }

    let edits = canonical_script(&old_lines, &new_lines);
    if edits.iter().all(|edit| !row_is_change(edit)) {
        return Ok(String::new());
    }

    let mut groups = Vec::<(usize, usize)>::new();
    let mut k = 0;
    while k < edits.len() {
        if row_is_change(&edits[k]) {
            let start = k;
            while k < edits.len() && row_is_change(&edits[k]) {
                k += 1;
            }
            groups.push((start, k));
        } else {
            k += 1;
        }
    }

    let merge_gap = 2 * context;
    let mut merged = Vec::<(usize, usize)>::new();
    for (start, end) in groups {
        if let Some(last) = merged.last_mut() {
            let keep_gap = start - last.1;
            if keep_gap <= merge_gap {
                last.1 = end;
                continue;
            }
        }
        merged.push((start, end));
    }

    let mut out = String::from("--- before\n+++ after\n");
    for (group_start, group_end) in merged {
        let mut start = group_start;
        let mut before = context;
        while start > 0 && before > 0 {
            if matches!(edits[start - 1], Edit::Keep { .. }) {
                start -= 1;
                before -= 1;
            } else {
                break;
            }
        }

        let mut end = group_end;
        let mut after = context;
        while end < edits.len() && after > 0 {
            if matches!(edits[end], Edit::Keep { .. }) {
                end += 1;
                after -= 1;
            } else {
                break;
            }
        }

        render_hunk(&mut out, &edits, start, end);
    }
    Ok(out)
}
