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
    Ambiguous {
        hunk: usize,
        first: usize,
        second: usize,
    },
}

#[derive(Clone)]
struct Row {
    prefix: u8,
    payload: String,
    line: usize,
}

struct Hunk {
    old_start: usize,
    old_count: usize,
    new_start: usize,
    new_count: usize,
    rows: Vec<Row>,
    line: usize,
    lead: usize,
    trail: usize,
}

fn parse_number(bytes: &[u8]) -> Option<usize> {
    if bytes.is_empty() || (bytes.len() > 1 && bytes[0] == b'0') {
        return None;
    }
    let mut value = 0usize;
    for &b in bytes {
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

fn parse_hunk_header(line: &str) -> Option<(usize, usize, usize, usize)> {
    let b = line.as_bytes();
    if b.len() < 11 || !b.starts_with(b"@@ -") || !b.ends_with(b" @@") {
        return None;
    }
    let core = &b[4..b.len() - 3];
    let comma1 = core.iter().position(|&x| x == b',')?;
    let plus = core.iter().position(|&x| x == b' ')?;
    if plus <= comma1 || core.get(plus + 1) != Some(&b'+') {
        return None;
    }
    let comma2 = core[plus + 2..].iter().position(|&x| x == b',')? + plus + 2;
    if comma2 <= plus + 2 {
        return None;
    }
    let a = parse_number(&core[..comma1])?;
    let count_b = parse_number(&core[comma1 + 1..plus])?;
    let c = parse_number(&core[plus + 2..comma2])?;
    let count_d = parse_number(&core[comma2 + 1..])?;
    if (count_b > 0 && a == 0) || (count_d > 0 && c == 0) {
        return None;
    }
    Some((a, count_b, c, count_d))
}

fn old_full_start(a: usize, b: usize) -> usize {
    if b > 0 { a - 1 } else { a }
}

fn new_full_start(c: usize, d: usize) -> usize {
    if d > 0 { c - 1 } else { c }
}

fn patch_lines(patch: &str) -> Vec<&str> {
    patch[..patch.len() - 1].split('\n').collect()
}

fn parse_patch(patch: &str) -> Result<Vec<Hunk>, ApplyError> {
    if patch.is_empty() {
        return Ok(Vec::new());
    }
    if !patch.ends_with('\n') {
        return Err(ApplyError::BadHeader {
            line: patch.bytes().filter(|&b| b == b'\n').count() + 1,
        });
    }

    let lines = patch_lines(patch);
    for (i, expected) in ["--- before", "+++ after"].iter().enumerate() {
        if lines.get(i).copied() != Some(*expected) {
            return Err(ApplyError::BadHeader { line: i + 1 });
        }
    }
    if lines.len() < 3 || !lines[2].starts_with("@@") {
        return Err(ApplyError::BadHunkHeader { line: 3 });
    }

    let mut hunks = Vec::new();
    let mut i = 2;
    let mut previous_end = 0usize;
    let mut net: i64 = 0;
    while i < lines.len() {
        let header_line = i + 1;
        let (a, b, c, d) = parse_hunk_header(lines[i])
            .ok_or(ApplyError::BadHunkHeader { line: header_line })?;
        let old_start = old_full_start(a, b);
        let new_start = new_full_start(c, d);

        i += 1;
        let body_start = i;
        let mut rows = Vec::new();
        while i < lines.len() && !lines[i].starts_with("@@") {
            let raw = lines[i].as_bytes();
            if raw.is_empty() || !matches!(raw[0], b' ' | b'-' | b'+') {
                return Err(ApplyError::BadBody { line: i + 1 });
            }
            if raw[1..].contains(&b'\r') {
                return Err(ApplyError::BadBody { line: i + 1 });
            }
            rows.push(Row {
                prefix: raw[0],
                payload: lines[i][1..].to_owned(),
                line: i + 1,
            });
            i += 1;
        }

        let old_actual = rows.iter().filter(|r| r.prefix != b'+').count();
        let new_actual = rows.iter().filter(|r| r.prefix != b'-').count();
        if old_actual != b || new_actual != d {
            return Err(ApplyError::CountMismatch { line: header_line });
        }
        if !rows.iter().any(|r| r.prefix == b'+' || r.prefix == b'-') {
            return Err(ApplyError::BadBody { line: header_line });
        }

        if old_start < previous_end {
            return Err(ApplyError::Misordered { line: header_line });
        }
        let required_new = old_start as i64 + net;
        if required_new < 0 || new_start as i64 != required_new {
            return Err(ApplyError::Misordered { line: header_line });
        }
        previous_end = old_start + b;
        net += d as i64 - b as i64;

        let lead = rows.iter().take_while(|r| r.prefix == b' ').count();
        let trail = rows.iter().rev().take_while(|r| r.prefix == b' ').count();
        hunks.push(Hunk {
            old_start,
            old_count: b,
            new_start,
            new_count: d,
            rows,
            line: header_line,
            lead,
            trail,
        });
        if hunks.len() > 16 {
            return Err(ApplyError::TooLarge);
        }
        if i == body_start {
            // Empty bodies are valid structurally only when their declared counts
            // are zero; the required change-row check above rejects those.
        }
    }
    Ok(hunks)
}

fn abs_distance(a: i64, b: i64) -> u64 {
    a.abs_diff(b)
}

pub fn apply_unified(
    source: &str,
    patch: &str,
    options: PatchOptions,
) -> Result<String, ApplyError> {
    if options.max_offset > 16 || options.fuzz > 2 {
        return Err(ApplyError::InvalidOptions);
    }

    let hunks = parse_patch(patch)?;

    if let Some(byte) = source.bytes().position(|b| b == b'\r') {
        return Err(ApplyError::SourceFormat { byte });
    }
    if !source.is_empty() && !source.ends_with('\n') {
        return Err(ApplyError::SourceMissingLf);
    }
    let mut working: Vec<String> = if source.is_empty() {
        Vec::new()
    } else {
        source[..source.len() - 1]
            .split('\n')
            .map(str::to_owned)
            .collect()
    };
    if working.len() > 128 {
        return Err(ApplyError::TooLarge);
    }

    let mut net: i64 = 0;
    let mut drift: i64 = 0;
    for (hunk_index, hunk) in hunks.iter().enumerate() {
        let expected = hunk.old_start as i64 + net + drift;
        let max_l = options.fuzz.min(hunk.lead);
        let max_r = options.fuzz.min(hunk.trail);
        let mut best_rank: Option<(usize, u64)> = None;
        let mut best_starts: Vec<(usize, usize, usize)> = Vec::new();

        for l in 0..=max_l {
            for r in 0..=max_r {
                if l + r > hunk.old_count {
                    continue;
                }
                let mut match_payloads = Vec::new();
                for row in &hunk.rows {
                    if row.prefix == b' ' || row.prefix == b'-' {
                        match_payloads.push(row.payload.as_str());
                    }
                }
                let trimmed: Vec<&str> = hunk.rows
                    .iter()
                    .enumerate()
                    .filter(|(idx, row)| {
                        !((*idx < l && row.prefix == b' ')
                            || (*idx >= hunk.rows.len().saturating_sub(r)
                                && row.prefix == b' '))
                    })
                    .filter_map(|(_, row)| {
                        if row.prefix == b' ' || row.prefix == b'-' {
                            Some(row.payload.as_str())
                        } else {
                            None
                        }
                    })
                    .collect();

                for s in 0..=working.len().saturating_sub(hunk.old_count) {
                    if s + hunk.old_count > working.len() {
                        continue;
                    }
                    let distance = abs_distance(s as i64, expected);
                    if distance > options.max_offset as u64 {
                        continue;
                    }
                    let start = s + l;
                    let end = s + hunk.old_count - r;
                    if end < start || end - start != trimmed.len() {
                        continue;
                    }
                    if working[start..end]
                        .iter()
                        .map(String::as_str)
                        .ne(trimmed.iter().copied())
                    {
                        continue;
                    }
                    let rank = (l + r, distance);
                    match best_rank {
                        None => {
                            best_rank = Some(rank);
                            best_starts.clear();
                            best_starts.push((s, l, r));
                        }
                        Some(old) if rank < old => {
                            best_rank = Some(rank);
                            best_starts.clear();
                            best_starts.push((s, l, r));
                        }
                        Some(old) if rank == old => best_starts.push((s, l, r)),
                        _ => {}
                    }
                }
                let _ = match_payloads;
            }
        }

        if best_starts.is_empty() {
            return Err(ApplyError::NoMatch {
                hunk: hunk_index + 1,
            });
        }
        let mut distinct: Vec<usize> = best_starts.iter().map(|x| x.0).collect();
        distinct.sort_unstable();
        distinct.dedup();
        if distinct.len() > 1 {
            return Err(ApplyError::Ambiguous {
                hunk: hunk_index + 1,
                first: distinct[0],
                second: distinct[1],
            });
        }

        let s = distinct[0];
        let (_, l, r) = best_starts
            .iter()
            .filter(|x| x.0 == s)
            .min_by_key(|x| x.1)
            .copied()
            .unwrap();

        let trim_start = l;
        let trim_end = hunk.rows.len().saturating_sub(r);
        let mut replacement = Vec::new();
        for row in &hunk.rows[trim_start..trim_end] {
            if row.prefix == b' ' || row.prefix == b'+' {
                replacement.push(row.payload.clone());
            }
        }
        let replace_start = s + l;
        let replace_end = s + hunk.old_count - r;
        working.splice(replace_start..replace_end, replacement);

        if working.len() > 128 {
            return Err(ApplyError::TooLarge);
        }
        drift = s as i64 - (hunk.old_start as i64 + net);
        net += hunk.new_count as i64 - hunk.old_count as i64;
    }

    let mut result = String::new();
    for line in working {
        result.push_str(&line);
        result.push('\n');
    }
    Ok(result)
}