#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchOptions {
    pub max_offset: usize,
    pub fuzz: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyError {
    InvalidOptions,
    BadHeader { line: usize },
    BadHunkHeader { line: usize },
    BadBody { line: usize },
    CountMismatch { line: usize },
    Misordered { line: usize },
    SourceFormat { byte: usize },
    SourceMissingLf,
    TooLarge,
    NoMatch { hunk: usize },
    Ambiguous { hunk: usize, first: usize, second: usize },
}

#[derive(Clone)]
struct Row {
    kind: u8,
    payload: String,
    line: usize,
}

#[derive(Clone)]
struct Hunk {
    header_line: usize,
    old_start: usize,
    old_count: usize,
    new_start: usize,
    new_count: usize,
    rows: Vec<Row>,
}

fn parse_decimal(s: &str) -> Option<usize> {
    if s.is_empty() || (s.len() > 1 && s.starts_with('0')) {
        return None;
    }
    let mut value = 0usize;
    for b in s.bytes() {
        if !b.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add((b - b'0') as usize)?;
        if value > 128 {
            return None;
        }
    }
    Some(value)
}

fn parse_hunk_header(line: &str, physical: usize) -> Result<(usize, usize, usize, usize), ApplyError> {
    let bad = || ApplyError::BadHunkHeader { line: physical };
    let inner = line.strip_prefix("@@ -").and_then(|s| s.strip_suffix(" @@")).ok_or_else(bad)?;
    let (old, new) = inner.split_once(" +").ok_or_else(bad)?;
    if new.contains(' ') || old.contains(' ') {
        return Err(bad());
    }
    let (a, b) = old.split_once(',').ok_or_else(bad)?;
    let (c, d) = new.split_once(',').ok_or_else(bad)?;
    let (a, b, c, d) = (
        parse_decimal(a).ok_or_else(bad)?,
        parse_decimal(b).ok_or_else(bad)?,
        parse_decimal(c).ok_or_else(bad)?,
        parse_decimal(d).ok_or_else(bad)?,
    );
    if (b > 0 && a == 0) || (d > 0 && c == 0) {
        return Err(bad());
    }
    Ok((a, b, c, d))
}

fn physical_lines(s: &str) -> Vec<&str> {
    if s.is_empty() {
        return Vec::new();
    }
    let mut lines: Vec<&str> = s.split_terminator('\n').collect();
    if s.ends_with('\n') && lines.last() == Some(&"") {
        lines.pop();
    }
    lines
}

fn abs_distance(a: i128, b: i128) -> u128 {
    (a - b).unsigned_abs()
}

pub fn apply_unified(
    source: &str,
    patch: &str,
    options: PatchOptions,
) -> Result<String, ApplyError> {
    if options.max_offset > 16 || options.fuzz > 2 {
        return Err(ApplyError::InvalidOptions);
    }

    let patch_lines;
    if patch.is_empty() {
        patch_lines = Vec::new();
    } else {
        if !patch.ends_with('\n') {
            return Err(ApplyError::BadHeader {
                line: patch.bytes().filter(|b| *b == b'\n').count() + 1,
            });
        }
        patch_lines = physical_lines(patch);
    }

    let mut hunks = Vec::new();
    if !patch_lines.is_empty() {
        for (i, expected) in ["--- before", "+++ after"].iter().enumerate() {
            if patch_lines.get(i).copied() != Some(*expected) {
                return Err(ApplyError::BadHeader { line: i + 1 });
            }
        }
        if patch_lines.len() == 2 {
            return Err(ApplyError::BadHunkHeader { line: 3 });
        }

        let mut idx = 2usize;
        let mut previous_end = 0usize;
        let mut net: i128 = 0;
        while idx < patch_lines.len() {
            let header_line = idx + 1;
            let (a, b, c, d) = parse_hunk_header(patch_lines[idx], header_line)?;
            let old_start = if b > 0 { a - 1 } else { a };
            let new_start = if d > 0 { c - 1 } else { c };
            idx += 1;

            let mut rows = Vec::new();
            while idx < patch_lines.len() && !patch_lines[idx].starts_with("@@") {
                let line = patch_lines[idx];
                let physical = idx + 1;
                let bytes = line.as_bytes();
                if bytes.is_empty() || !matches!(bytes[0], b' ' | b'-' | b'+') {
                    return Err(ApplyError::BadBody { line: physical });
                }
                if line[1..].contains('\r') {
                    return Err(ApplyError::BadBody { line: physical });
                }
                rows.push(Row {
                    kind: bytes[0],
                    payload: line[1..].to_owned(),
                    line: physical,
                });
                idx += 1;
            }

            let old_seen = rows.iter().filter(|r| r.kind == b' ' || r.kind == b'-').count();
            let new_seen = rows.iter().filter(|r| r.kind == b' ' || r.kind == b'+').count();
            if old_seen != b || new_seen != d {
                return Err(ApplyError::CountMismatch { line: header_line });
            }
            if !rows.iter().any(|r| r.kind == b'+' || r.kind == b'-') {
                return Err(ApplyError::BadBody { line: header_line });
            }

            if old_start < previous_end
                || (new_start as i128) != (old_start as i128 + net)
            {
                return Err(ApplyError::Misordered { line: header_line });
            }
            previous_end = old_start + b;
            net += d as i128 - b as i128;

            hunks.push(Hunk {
                header_line,
                old_start,
                old_count: b,
                new_start,
                new_count: d,
                rows,
            });
            if hunks.len() > 16 {
                return Err(ApplyError::TooLarge);
            }
        }
    }

    if let Some(byte) = source.as_bytes().iter().position(|b| *b == b'\r') {
        return Err(ApplyError::SourceFormat { byte });
    }
    if !source.is_empty() && !source.ends_with('\n') {
        return Err(ApplyError::SourceMissingLf);
    }
    let mut working: Vec<String> = if source.is_empty() {
        Vec::new()
    } else {
        source.split_terminator('\n').map(str::to_owned).collect()
    };
    if working.len() > 128 {
        return Err(ApplyError::TooLarge);
    }

    let mut net: i128 = 0;
    let mut drift: i128 = 0;
    for (hunk_index, hunk) in hunks.iter().enumerate() {
        let lead = hunk.rows.iter().take_while(|r| r.kind == b' ').count();
        let trail = hunk.rows.iter().rev().take_while(|r| r.kind == b' ').count();
        let max_l = options.fuzz.min(lead);
        let max_r = options.fuzz.min(trail);
        let expected = hunk.old_start as i128 + net + drift;

        let mut best_rank: Option<(usize, u128)> = None;
        let mut best_starts = Vec::new();
        let mut best_choice: Option<(usize, usize, usize)> = None;

        for l in 0..=max_l {
            for r in 0..=max_r {
                if l + r > lead + trail || l + r > hunk.rows.len() {
                    continue;
                }
                let trimmed_start = l;
                let trimmed_end = hunk.rows.len().saturating_sub(r);
                let expected_old: Vec<&str> = hunk.rows[trimmed_start..trimmed_end]
                    .iter()
                    .filter(|row| row.kind == b' ' || row.kind == b'-')
                    .map(|row| row.payload.as_str())
                    .collect();

                for s in 0..=working.len().saturating_sub(hunk.old_count) {
                    if s + hunk.old_count > working.len() {
                        continue;
                    }
                    let distance = abs_distance(s as i128, expected);
                    if distance > options.max_offset as u128 {
                        continue;
                    }
                    let actual = &working[s + l..s + hunk.old_count - r];
                    if actual.len() != expected_old.len()
                        || !actual.iter().zip(expected_old.iter()).all(|(a, b)| a == b)
                    {
                        continue;
                    }

                    let rank = (l + r, distance);
                    match best_rank {
                        None => {
                            best_rank = Some(rank);
                            best_starts.clear();
                            best_starts.push(s);
                            best_choice = Some((s, l, r));
                        }
                        Some(current) if rank < current => {
                            best_rank = Some(rank);
                            best_starts.clear();
                            best_starts.push(s);
                            best_choice = Some((s, l, r));
                        }
                        Some(current) if rank == current => {
                            if !best_starts.contains(&s) {
                                best_starts.push(s);
                            }
                            if best_choice.map_or(true, |(bs, bl, _)| s == bs && l < bl) {
                                best_choice = Some((s, l, r));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        let Some((s, l, r)) = best_choice else {
            return Err(ApplyError::NoMatch { hunk: hunk_index + 1 });
        };
        best_starts.sort_unstable();
        best_starts.dedup();
        if best_starts.len() >= 2 {
            return Err(ApplyError::Ambiguous {
                hunk: hunk_index + 1,
                first: best_starts[0],
                second: best_starts[1],
            });
        }

        let begin = l;
        let end = hunk.rows.len() - r;
        let replacement: Vec<String> = hunk.rows[begin..end]
            .iter()
            .filter(|row| row.kind == b' ' || row.kind == b'+')
            .map(|row| row.payload.clone())
            .collect();
        let replace_start = s + l;
        let replace_end = s + hunk.old_count - r;
        working.splice(replace_start..replace_end, replacement);

        if working.len() > 128 {
            return Err(ApplyError::TooLarge);
        }
        let previous_net = net;
        net += hunk.new_count as i128 - hunk.old_count as i128;
        drift = s as i128 - (hunk.old_start as i128 + previous_net);
    }

    let mut output = String::new();
    for line in working {
        output.push_str(&line);
        output.push('\n');
    }
    Ok(output)
}