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

#[derive(Debug)]
struct Hunk {
    line: usize,
    old_start: usize,
    old_count: usize,
    new_start: usize,
    new_count: usize,
    rows: Vec<(u8, String)>,
}

fn parse_number(s: &str) -> Option<usize> {
    if s.is_empty()
        || !s.bytes().all(|b| b.is_ascii_digit())
        || (s.len() > 1 && s.starts_with('0'))
    {
        return None;
    }
    let mut value = 0usize;
    for b in s.bytes() {
        value = value.checked_mul(10)?.checked_add((b - b'0') as usize)?;
        if value > 128 {
            return None;
        }
    }
    Some(value)
}

fn parse_hunk_header(s: &str) -> Option<(usize, usize, usize, usize)> {
    let rest = s.strip_prefix("@@ -")?;
    let (old, rest) = rest.split_once(" +")?;
    let (new, tail) = rest.split_once(" @@")?;
    if !tail.is_empty() {
        return None;
    }
    let (a, b) = old.split_once(',')?;
    let (c, d) = new.split_once(',')?;
    let a = parse_number(a)?;
    let b = parse_number(b)?;
    let c = parse_number(c)?;
    let d = parse_number(d)?;
    if (b > 0 && a == 0) || (d > 0 && c == 0) {
        return None;
    }
    Some((a, b, c, d))
}

fn physical_lines(s: &str) -> Vec<&str> {
    if s.is_empty() {
        return Vec::new();
    }
    let mut lines: Vec<&str> = s.split_terminator('\n').collect();
    if !s.ends_with('\n') {
        // This helper is only used after checking the final LF.
        lines.push(s.rsplit('\n').next().unwrap_or(""));
    }
    lines
}

pub fn apply_unified(
    source: &str,
    patch: &str,
    options: PatchOptions,
) -> Result<String, ApplyError> {
    if options.max_offset > 16 || options.fuzz > 2 {
        return Err(ApplyError::InvalidOptions);
    }

    let mut hunks = Vec::new();

    if !patch.is_empty() {
        if !patch.ends_with('\n') {
            return Err(ApplyError::BadHeader {
                line: patch.bytes().filter(|&b| b == b'\n').count() + 1,
            });
        }

        let lines: Vec<&str> = patch.split_terminator('\n').collect();

        if lines.first().copied() != Some("--- before") {
            return Err(ApplyError::BadHeader { line: 1 });
        }
        if lines.get(1).copied() != Some("+++ after") {
            return Err(ApplyError::BadHeader { line: 2 });
        }
        if lines.len() < 3 {
            return Err(ApplyError::BadHunkHeader { line: 3 });
        }

        let mut i = 2usize;
        let mut previous_old_end = 0usize;
        let mut cumulative_delta: i128 = 0;

        while i < lines.len() {
            let header_line = i + 1;
            let Some((a, b, c, d)) = parse_hunk_header(lines[i]) else {
                return Err(ApplyError::BadHunkHeader { line: header_line });
            };
            let old_start = if b > 0 { a - 1 } else { a };
            let new_start = if d > 0 { c - 1 } else { c };

            i += 1;
            let mut rows = Vec::new();
            let mut old_seen = 0usize;
            let mut new_seen = 0usize;

            while i < lines.len() && !lines[i].starts_with("@@") {
                let row_line = i + 1;
                let row = lines[i];
                if row.is_empty() || row.as_bytes()[0] != b' ' && row.as_bytes()[0] != b'-' && row.as_bytes()[0] != b'+' {
                    return Err(ApplyError::BadBody { line: row_line });
                }
                if row.as_bytes().contains(&b'\r') {
                    return Err(ApplyError::BadBody { line: row_line });
                }
                let prefix = row.as_bytes()[0];
                let payload = &row[1..];
                rows.push((prefix, payload.to_string()));
                if prefix == b' ' || prefix == b'-' {
                    old_seen += 1;
                }
                if prefix == b' ' || prefix == b'+' {
                    new_seen += 1;
                }
                i += 1;
            }

            if old_seen != b || new_seen != d {
                return Err(ApplyError::CountMismatch { line: header_line });
            }
            if !rows.iter().any(|(p, _)| *p == b'+' || *p == b'-') {
                return Err(ApplyError::BadBody { line: header_line });
            }

            if old_start < previous_old_end {
                return Err(ApplyError::Misordered { line: header_line });
            }
            let required_new = old_start as i128 + cumulative_delta;
            if required_new < 0 || new_start as i128 != required_new {
                return Err(ApplyError::Misordered { line: header_line });
            }

            if hunks.len() == 16 {
                return Err(ApplyError::TooLarge);
            }

            previous_old_end = old_start.saturating_add(b);
            cumulative_delta += d as i128 - b as i128;
            hunks.push(Hunk {
                line: header_line,
                old_start,
                old_count: b,
                new_start,
                new_count: d,
                rows,
            });
        }

        if hunks.is_empty() {
            return Err(ApplyError::BadHunkHeader { line: 3 });
        }
    }

    if let Some(byte) = source.as_bytes().iter().position(|&b| b == b'\r') {
        return Err(ApplyError::SourceFormat { byte });
    }
    if !source.is_empty() && !source.ends_with('\n') {
        return Err(ApplyError::SourceMissingLf);
    }
    let mut working: Vec<String> = if source.is_empty() {
        Vec::new()
    } else {
        source.split_terminator('\n').map(str::to_string).collect()
    };
    if working.len() > 128 {
        return Err(ApplyError::TooLarge);
    }

    let mut net: i128 = 0;
    let mut drift: i128 = 0;

    for (index, hunk) in hunks.iter().enumerate() {
        let lead = hunk.rows.iter().take_while(|(p, _)| *p == b' ').count();
        let trail = hunk.rows.iter().rev().take_while(|(p, _)| *p == b' ').count();

        let expected = hunk.old_start as i128 + net + drift;
        let mut best_rank: Option<(usize, i128)> = None;
        let mut best_starts = Vec::new();
        let mut best_by_start: Vec<(usize, usize, usize)> = Vec::new();

        for left in 0..=options.fuzz.min(lead) {
            for right in 0..=options.fuzz.min(trail) {
                if left + right > hunk.old_count {
                    continue;
                }

                let mut old_payloads = Vec::new();
                for (row_index, (prefix, payload)) in hunk.rows.iter().enumerate() {
                    if row_index < left || row_index >= hunk.rows.len().saturating_sub(right) {
                        continue;
                    }
                    if *prefix == b' ' || *prefix == b'-' {
                        old_payloads.push(payload.as_str());
                    }
                }
                if old_payloads.len() != hunk.old_count - left - right {
                    continue;
                }

                for start in 0..=working.len().saturating_sub(hunk.old_count) {
                    if start + hunk.old_count > working.len() {
                        continue;
                    }
                    let distance = (start as i128 - expected).abs();
                    if distance > options.max_offset as i128 {
                        continue;
                    }
                    let match_start = start + left;
                    let match_end = start + hunk.old_count - right;
                    if !working[match_start..match_end]
                        .iter()
                        .map(String::as_str)
                        .eq(old_payloads.iter().copied())
                    {
                        continue;
                    }

                    let rank = (left + right, distance);
                    match best_rank {
                        None => {
                            best_rank = Some(rank);
                            best_starts.clear();
                            best_by_start.clear();
                        }
                        Some(current) if rank < current => {
                            best_rank = Some(rank);
                            best_starts.clear();
                            best_by_start.clear();
                        }
                        Some(current) if rank > current => continue,
                        _ => {}
                    }
                    best_starts.push(start);
                    best_by_start.push((start, left, right));
                }
            }
        }

        if best_starts.is_empty() {
            return Err(ApplyError::NoMatch { hunk: index + 1 });
        }

        best_starts.sort_unstable();
        best_starts.dedup();
        if best_starts.len() > 1 {
            return Err(ApplyError::Ambiguous {
                hunk: index + 1,
                first: best_starts[0],
                second: best_starts[1],
            });
        }

        let start = best_starts[0];
        let (_, left, right) = best_by_start
            .iter()
            .filter(|(s, _, _)| *s == start)
            .min_by_key(|(_, l, _)| *l)
            .copied()
            .unwrap();

        let mut replacement = Vec::new();
        for (row_index, (prefix, payload)) in hunk.rows.iter().enumerate() {
            if row_index < left || row_index >= hunk.rows.len().saturating_sub(right) {
                continue;
            }
            if *prefix == b' ' || *prefix == b'+' {
                replacement.push(payload.clone());
            }
        }

        let replace_start = start + left;
        let replace_end = start + hunk.old_count - right;
        working.splice(replace_start..replace_end, replacement);

        if working.len() > 128 {
            return Err(ApplyError::TooLarge);
        }

        drift = start as i128 - (hunk.old_start as i128 + net);
        net += hunk.new_count as i128 - hunk.old_count as i128;
    }

    let mut output = String::new();
    for line in working {
        output.push_str(&line);
        output.push('\n');
    }
    Ok(output)
}

fn main() {}