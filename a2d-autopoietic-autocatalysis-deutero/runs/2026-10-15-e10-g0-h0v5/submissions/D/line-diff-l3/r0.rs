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
    prefix: u8,
    payload: String,
}

#[derive(Clone)]
struct Hunk {
    line: usize,
    old_start: usize,
    old_count: usize,
    new_start: usize,
    new_count: usize,
    rows: Vec<Row>,
}

fn parse_number(bytes: &[u8], pos: &mut usize) -> Option<usize> {
    let start = *pos;
    while *pos < bytes.len() && bytes[*pos].is_ascii_digit() {
        *pos += 1;
    }
    if start == *pos {
        return None;
    }
    let digits = &bytes[start..*pos];
    if digits.len() > 1 && digits[0] == b'0' {
        return None;
    }
    let mut n = 0usize;
    for &b in digits {
        n = n.checked_mul(10)?.checked_add((b - b'0') as usize)?;
        if n > 128 {
            return None;
        }
    }
    Some(n)
}

fn parse_hunk_header(line: &str, line_no: usize) -> Result<(usize, usize, usize, usize), ApplyError> {
    let bad = || ApplyError::BadHunkHeader { line: line_no };
    let b = line.as_bytes();
    if !b.starts_with(b"@@ -") {
        return Err(bad());
    }
    let mut p = 4;
    let a = parse_number(b, &mut p).ok_or_else(bad)?;
    if b.get(p) != Some(&b',') {
        return Err(bad());
    }
    p += 1;
    let old_count = parse_number(b, &mut p).ok_or_else(bad)?;
    if b.get(p) != Some(&b' ') {
        return Err(bad());
    }
    p += 1;
    if b.get(p..p + 1) != Some(b"+") {
        return Err(bad());
    }
    p += 1;
    let c = parse_number(b, &mut p).ok_or_else(bad)?;
    if b.get(p) != Some(&b',') {
        return Err(bad());
    }
    p += 1;
    let new_count = parse_number(b, &mut p).ok_or_else(bad)?;
    if b.get(p..p + 3) != Some(b" @@") || p + 3 != b.len() {
        return Err(bad());
    }
    if (old_count > 0 && a == 0) || (new_count > 0 && c == 0) {
        return Err(bad());
    }
    let old_start = if old_count > 0 { a - 1 } else { a };
    let new_start = if new_count > 0 { c - 1 } else { c };
    Ok((old_start, old_count, new_start, new_count))
}

fn physical_lines(s: &str) -> Vec<&str> {
    let mut lines: Vec<&str> = s.split('\n').collect();
    if s.ends_with('\n') {
        lines.pop();
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

        let lines = physical_lines(patch);
        for (i, expected) in ["--- before", "+++ after"].iter().enumerate() {
            if lines.get(i).copied() != Some(*expected) {
                return Err(ApplyError::BadHeader { line: i + 1 });
            }
        }
        if lines.len() < 3 {
            return Err(ApplyError::BadHunkHeader { line: 3 });
        }

        let mut i = 2;
        let mut prev_old_end = 0usize;
        let mut net: i64 = 0;
        while i < lines.len() {
            let header_line = i + 1;
            let (old_start, old_count, new_start, new_count) =
                parse_hunk_header(lines[i], header_line)?;
            i += 1;

            let mut rows = Vec::new();
            let mut old_seen = 0usize;
            let mut new_seen = 0usize;
            while i < lines.len() && !lines[i].starts_with("@@") {
                let row_line = i + 1;
                let bytes = lines[i].as_bytes();
                if bytes.is_empty()
                    || !matches!(bytes[0], b' ' | b'-' | b'+')
                    || lines[i].contains('\r')
                {
                    return Err(ApplyError::BadBody { line: row_line });
                }
                let payload = &lines[i][1..];
                let prefix = bytes[0];
                if prefix == b' ' || prefix == b'-' {
                    old_seen += 1;
                }
                if prefix == b' ' || prefix == b'+' {
                    new_seen += 1;
                }
                rows.push(Row {
                    prefix,
                    payload: payload.to_string(),
                });
                i += 1;
            }

            if old_seen != old_count || new_seen != new_count {
                return Err(ApplyError::CountMismatch { line: header_line });
            }
            if !rows.iter().any(|r| r.prefix == b'+' || r.prefix == b'-') {
                return Err(ApplyError::BadBody { line: header_line });
            }

            let old_end = old_start.checked_add(old_count).ok_or(ApplyError::TooLarge)?;
            if old_start < prev_old_end {
                return Err(ApplyError::Misordered { line: header_line });
            }
            let required_new = (old_start as i64).checked_add(net).ok_or(ApplyError::Misordered {
                line: header_line,
            })?;
            if required_new < 0 || new_start as i64 != required_new {
                return Err(ApplyError::Misordered { line: header_line });
            }
            prev_old_end = old_end;
            net += new_count as i64 - old_count as i64;

            hunks.push(Hunk {
                line: header_line,
                old_start,
                old_count,
                new_start,
                new_count,
                rows,
            });
            if hunks.len() > 16 {
                return Err(ApplyError::TooLarge);
            }
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
        source[..source.len() - 1]
            .split('\n')
            .map(str::to_string)
            .collect()
    };
    if working.len() > 128 {
        return Err(ApplyError::TooLarge);
    }

    let mut net: i64 = 0;
    let mut drift: i64 = 0;
    for (hunk_idx, hunk) in hunks.iter().enumerate() {
        let mut lead = 0usize;
        while lead < hunk.rows.len() && hunk.rows[lead].prefix == b' ' {
            lead += 1;
        }
        let mut trail = 0usize;
        while trail < hunk.rows.len() && hunk.rows[hunk.rows.len() - 1 - trail].prefix == b' ' {
            trail += 1;
        }

        let expected = hunk.old_start as i64 + net + drift;
        let mut best_rank: Option<(usize, i64)> = None;
        let mut best_starts = Vec::new();
        let mut best_by_start: Vec<(usize, usize, usize)> = Vec::new();

        for l in 0..=options.fuzz.min(lead) {
            for r in 0..=options.fuzz.min(trail) {
                if l + r > hunk.old_count {
                    continue;
                }
                let mut old_payloads = Vec::new();
                for row in &hunk.rows[l..hunk.rows.len() - r] {
                    if row.prefix == b' ' || row.prefix == b'-' {
                        old_payloads.push(row.payload.as_str());
                    }
                }

                for s in 0..=working.len() {
                    if s + hunk.old_count > working.len() {
                        continue;
                    }
                    let distance = ((s as i64) - expected).abs();
                    if distance > options.max_offset as i64 {
                        continue;
                    }
                    let start = s + l;
                    let end = s + hunk.old_count - r;
                    if end - start != old_payloads.len()
                        || !working[start..end]
                            .iter()
                            .map(String::as_str)
                            .eq(old_payloads.iter().copied())
                    {
                        continue;
                    }
                    let rank = (l + r, distance);
                    match best_rank {
                        None => {
                            best_rank = Some(rank);
                            best_starts.clear();
                            best_by_start.clear();
                            best_starts.push(s);
                            best_by_start.push((s, l, r));
                        }
                        Some(current) if rank < current => {
                            best_rank = Some(rank);
                            best_starts.clear();
                            best_by_start.clear();
                            best_starts.push(s);
                            best_by_start.push((s, l, r));
                        }
                        Some(current) if rank == current => {
                            best_starts.push(s);
                            best_by_start.push((s, l, r));
                        }
                        _ => {}
                    }
                }
            }
        }

        if best_rank.is_none() {
            return Err(ApplyError::NoMatch { hunk: hunk_idx + 1 });
        }
        best_starts.sort_unstable();
        best_starts.dedup();
        if best_starts.len() > 1 {
            return Err(ApplyError::Ambiguous {
                hunk: hunk_idx + 1,
                first: best_starts[0],
                second: best_starts[1],
            });
        }
        let s = best_starts[0];
        let (_, l, r) = best_by_start
            .into_iter()
            .filter(|(start, _, _)| *start == s)
            .min_by_key(|(_, l, _)| *l)
            .unwrap();

        let mut replacement = Vec::new();
        for row in &hunk.rows[l..hunk.rows.len() - r] {
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

    let mut output = String::new();
    for line in working {
        output.push_str(&line);
        output.push('\n');
    }
    Ok(output)
}