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

fn physical_lines(s: &str) -> Vec<&str> {
    if s.is_empty() {
        return Vec::new();
    }
    let mut lines: Vec<&str> = s.split('\n').collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    lines
}

fn parse_decimal(s: &str) -> Option<usize> {
    if s.is_empty()
        || (s.len() > 1 && s.starts_with('0'))
        || !s.bytes().all(|b| b.is_ascii_digit())
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

fn parse_hunk_header(line: &str) -> Option<(usize, usize, usize, usize)> {
    let inner = line.strip_prefix("@@ -")?.strip_suffix(" @@")?;
    let (old, new) = inner.split_once(" +")?;
    let (a, b) = old.split_once(',')?;
    let (c, d) = new.split_once(',')?;
    if a.contains(',') || b.contains(',') || c.contains(',') || d.contains(',') {
        return None;
    }
    let a = parse_decimal(a)?;
    let b = parse_decimal(b)?;
    let c = parse_decimal(c)?;
    let d = parse_decimal(d)?;
    if (b > 0 && a == 0) || (d > 0 && c == 0) {
        return None;
    }
    Some((a, b, c, d))
}

fn full_start(start: usize, count: usize) -> usize {
    if count > 0 { start - 1 } else { start }
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

    let lines = physical_lines(patch);
    for (idx, expected) in ["--- before", "+++ after"].iter().enumerate() {
        if lines.get(idx).copied() != Some(*expected) {
            return Err(ApplyError::BadHeader { line: idx + 1 });
        }
    }
    if lines.len() <= 2 || !lines[2].starts_with("@@") {
        return Err(ApplyError::BadHunkHeader { line: 3 });
    }

    let mut hunks = Vec::new();
    let mut i = 2;
    let mut previous_old_end = 0usize;
    let mut net: i128 = 0;

    while i < lines.len() {
        let header_line = i + 1;
        if !lines[i].starts_with("@@") {
            return Err(ApplyError::BadHunkHeader { line: header_line });
        }
        let (a, b, c, d) = parse_hunk_header(lines[i])
            .ok_or(ApplyError::BadHunkHeader { line: header_line })?;
        let old_start = full_start(a, b);
        let new_start = full_start(c, d);
        i += 1;

        let mut rows = Vec::new();
        let mut old_seen = 0usize;
        let mut new_seen = 0usize;
        while i < lines.len() && !lines[i].starts_with("@@") {
            let text = lines[i];
            let bytes = text.as_bytes();
            if bytes.is_empty() || !matches!(bytes[0], b' ' | b'-' | b'+') || text.contains('\r') {
                return Err(ApplyError::BadBody { line: i + 1 });
            }
            let payload = &text[1..];
            let prefix = bytes[0];
            if prefix == b' ' || prefix == b'-' {
                old_seen += 1;
            }
            if prefix == b' ' || prefix == b'+' {
                new_seen += 1;
            }
            rows.push(Row {
                prefix,
                payload: payload.to_owned(),
            });
            i += 1;
        }

        if old_seen != b || new_seen != d {
            return Err(ApplyError::CountMismatch { line: header_line });
        }
        if !rows.iter().any(|r| r.prefix == b'+' || r.prefix == b'-') {
            return Err(ApplyError::BadBody { line: header_line });
        }
        if old_start < previous_old_end
            || (new_start as i128) != (old_start as i128 + net)
        {
            return Err(ApplyError::Misordered { line: header_line });
        }
        previous_old_end = old_start + b;
        net += d as i128 - b as i128;

        hunks.push(Hunk {
            line: header_line,
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

    if hunks.is_empty() {
        return Err(ApplyError::BadHunkHeader { line: 3 });
    }
    Ok(hunks)
}

fn source_lines(source: &str) -> Result<Vec<String>, ApplyError> {
    if let Some(byte) = source.as_bytes().iter().position(|&b| b == b'\r') {
        return Err(ApplyError::SourceFormat { byte });
    }
    if !source.is_empty() && !source.ends_with('\n') {
        return Err(ApplyError::SourceMissingLf);
    }
    let lines = physical_lines(source);
    if lines.len() > 128 {
        return Err(ApplyError::TooLarge);
    }
    Ok(lines.into_iter().map(str::to_owned).collect())
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
    let mut working = source_lines(source)?;
    let mut net: i128 = 0;
    let mut drift: i128 = 0;

    for (idx, hunk) in hunks.iter().enumerate() {
        let mut lead = 0usize;
        while lead < hunk.rows.len() && hunk.rows[lead].prefix == b' ' {
            lead += 1;
        }
        let mut trail = 0usize;
        while trail < hunk.rows.len() && hunk.rows[hunk.rows.len() - 1 - trail].prefix == b' ' {
            trail += 1;
        }
        let expected = hunk.old_start as i128 + net + drift;
        let mut best_rank: Option<(usize, i128)> = None;
        let mut best_by_start: Vec<(usize, usize, usize)> = Vec::new();

        let max_left = options.fuzz.min(lead);
        let max_right = options.fuzz.min(trail);
        for s in 0..=working.len() {
            if s + hunk.old_count > working.len() {
                continue;
            }
            let distance = (s as i128 - expected).abs();
            if distance > options.max_offset as i128 {
                continue;
            }
            for left in 0..=max_left {
                for right in 0..=max_right {
                    if left + right > hunk.old_count {
                        continue;
                    }
                    let mut old_payloads = Vec::new();
                    let mut new_payloads = Vec::new();
                    let mut context_index = 0usize;
                    for (row_idx, row) in hunk.rows.iter().enumerate() {
                        if row.prefix == b' ' {
                            let dropped = row_idx < left
                                || row_idx >= hunk.rows.len().saturating_sub(right);
                            if !dropped {
                                old_payloads.push(row.payload.as_str());
                                new_payloads.push(row.payload.as_str());
                            } else {
                                context_index += 1;
                            }
                        } else if row.prefix == b'-' {
                            old_payloads.push(row.payload.as_str());
                        } else {
                            new_payloads.push(row.payload.as_str());
                        }
                    }
                    let start = s + left;
                    let end = s + hunk.old_count - right;
                    if end > working.len() || start > end {
                        continue;
                    }
                    if old_payloads.len() != end - start
                        || !old_payloads
                            .iter()
                            .zip(&working[start..end])
                            .all(|(a, b)| *a == b)
                    {
                        continue;
                    }

                    let rank = (left + right, distance);
                    match best_rank {
                        None => {
                            best_rank = Some(rank);
                            best_by_start.clear();
                            best_by_start.push((s, left, right));
                        }
                        Some(old_rank) if rank < old_rank => {
                            best_rank = Some(rank);
                            best_by_start.clear();
                            best_by_start.push((s, left, right));
                        }
                        Some(old_rank) if rank == old_rank => {
                            if let Some(existing) = best_by_start.iter_mut().find(|x| x.0 == s) {
                                if left < existing.1 {
                                    *existing = (s, left, right);
                                }
                            } else {
                                best_by_start.push((s, left, right));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        if best_by_start.is_empty() {
            return Err(ApplyError::NoMatch { hunk: idx + 1 });
        }
        best_by_start.sort_by_key(|x| x.0);
        if best_by_start.len() > 1 {
            return Err(ApplyError::Ambiguous {
                hunk: idx + 1,
                first: best_by_start[0].0,
                second: best_by_start[1].0,
            });
        }

        let (s, left, right) = best_by_start[0];
        let mut replacement = Vec::new();
        for (row_idx, row) in hunk.rows.iter().enumerate() {
            if row.prefix == b' ' {
                let dropped = row_idx < left || row_idx >= hunk.rows.len().saturating_sub(right);
                if !dropped {
                    replacement.push(row.payload.clone());
                }
            } else if row.prefix == b'+' {
                replacement.push(row.payload.clone());
            }
        }
        let start = s + left;
        let end = s + hunk.old_count - right;
        working.splice(start..end, replacement);
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