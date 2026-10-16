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

#[derive(Debug, Clone)]
struct Row {
    prefix: u8,
    payload: String,
    line: usize,
}

#[derive(Debug, Clone)]
struct Hunk {
    old_start: usize,
    old_count: usize,
    new_start: usize,
    new_count: usize,
    rows: Vec<Row>,
    header_line: usize,
}

fn parse_decimal(s: &str) -> Option<usize> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if s.len() > 1 && s.starts_with('0') {
        return None;
    }
    let n = s.parse::<usize>().ok()?;
    (n <= 128).then_some(n)
}

fn parse_hunk_header(line: &str, line_no: usize) -> Result<(usize, usize, usize, usize), ApplyError> {
    let bad = || ApplyError::BadHunkHeader { line: line_no };
    let inner = line
        .strip_prefix("@@ -")
        .and_then(|s| s.strip_suffix(" @@"))
        .ok_or_else(bad)?;
    let (old, new) = inner.split_once(" +").ok_or_else(bad)?;
    let (a, b) = old.split_once(',').ok_or_else(bad)?;
    let (c, d) = new.split_once(',').ok_or_else(bad)?;
    if a.contains(',') || b.contains(',') || c.contains(',') || d.contains(',') {
        return Err(bad());
    }
    let a = parse_decimal(a).ok_or_else(bad)?;
    let b = parse_decimal(b).ok_or_else(bad)?;
    let c = parse_decimal(c).ok_or_else(bad)?;
    let d = parse_decimal(d).ok_or_else(bad)?;
    if (b > 0 && a == 0) || (d > 0 && c == 0) {
        return Err(bad());
    }
    Ok((a, b, c, d))
}

fn full_start(anchor: usize, count: usize) -> isize {
    if count > 0 {
        anchor as isize - 1
    } else {
        anchor as isize
    }
}

fn patch_lines(patch: &str) -> Vec<&str> {
    let bytes = patch.as_bytes();
    let mut lines = Vec::new();
    let mut start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\n' {
            lines.push(&patch[start..i]);
            start = i + 1;
        }
    }
    if start < bytes.len() {
        lines.push(&patch[start..]);
    }
    lines
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
    if lines.first().copied() != Some("--- before") {
        return Err(ApplyError::BadHeader { line: 1 });
    }
    if lines.get(1).copied() != Some("+++ after") {
        return Err(ApplyError::BadHeader { line: 2 });
    }
    if lines.len() <= 2 {
        return Err(ApplyError::BadHunkHeader { line: 3 });
    }

    let mut hunks = Vec::new();
    let mut i = 2;
    let mut previous_old_end: Option<isize> = None;
    let mut preceding_delta: isize = 0;

    while i < lines.len() {
        let header_line = i + 1;
        let (a, b, c, d) = parse_hunk_header(lines[i], header_line)?;
        let old_start = full_start(a, b);
        let new_start = full_start(c, d);

        i += 1;
        let mut rows = Vec::new();
        let mut old_seen = 0usize;
        let mut new_seen = 0usize;

        while i < lines.len() && !lines[i].starts_with("@@") {
            let row_line = i + 1;
            let row = lines[i];
            let bytes = row.as_bytes();
            if bytes.is_empty() || !matches!(bytes[0], b' ' | b'-' | b'+') {
                return Err(ApplyError::BadBody { line: row_line });
            }
            let payload = &row[1..];
            if payload.contains('\r') {
                return Err(ApplyError::BadBody { line: row_line });
            }
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
                line: row_line,
            });
            i += 1;
        }

        if old_seen != b || new_seen != d {
            return Err(ApplyError::CountMismatch { line: header_line });
        }
        if !rows.iter().any(|r| r.prefix == b'+' || r.prefix == b'-') {
            return Err(ApplyError::BadBody { line: header_line });
        }

        if let Some(end) = previous_old_end {
            if old_start < end || new_start != old_start + preceding_delta {
                return Err(ApplyError::Misordered { line: header_line });
            }
        } else if new_start != old_start {
            return Err(ApplyError::Misordered { line: header_line });
        }

        if hunks.len() >= 16 {
            return Err(ApplyError::TooLarge);
        }

        previous_old_end = Some(old_start + b as isize);
        preceding_delta += d as isize - b as isize;
        hunks.push(Hunk {
            old_start: old_start as usize,
            old_count: b,
            new_start: new_start as usize,
            new_count: d,
            rows,
            header_line,
        });
    }

    Ok(hunks)
}

fn validate_source(source: &str) -> Result<Vec<String>, ApplyError> {
    if let Some(byte) = source.as_bytes().iter().position(|&b| b == b'\r') {
        return Err(ApplyError::SourceFormat { byte });
    }
    if !source.is_empty() && !source.ends_with('\n') {
        return Err(ApplyError::SourceMissingLf);
    }
    if source.is_empty() {
        return Ok(Vec::new());
    }
    let lines: Vec<String> = source[..source.len() - 1]
        .split('\n')
        .map(str::to_string)
        .collect();
    if lines.len() > 128 {
        return Err(ApplyError::TooLarge);
    }
    Ok(lines)
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
    let mut working = validate_source(source)?;

    let mut net: isize = 0;
    let mut drift: isize = 0;

    for (hunk_index, hunk) in hunks.iter().enumerate() {
        let mut lead = 0usize;
        while lead < hunk.rows.len() && hunk.rows[lead].prefix == b' ' {
            lead += 1;
        }
        let mut trail = 0usize;
        while trail < hunk.rows.len() && hunk.rows[hunk.rows.len() - 1 - trail].prefix == b' ' {
            trail += 1;
        }

        let expected = hunk.old_start as isize + net + drift;
        let max_lead = options.fuzz.min(lead);
        let max_trail = options.fuzz.min(trail);

        let mut best_rank: Option<(usize, usize)> = None;
        let mut best: Vec<(usize, usize, usize)> = Vec::new();

        for start in 0..=working.len() {
            if start + hunk.old_count > working.len() {
                continue;
            }
            let distance = (start as isize - expected).abs() as usize;
            if distance > options.max_offset {
                continue;
            }

            for left_drop in 0..=max_lead {
                for right_drop in 0..=max_trail {
                    if left_drop + right_drop > hunk.old_count {
                        continue;
                    }
                    let mut old_payloads = Vec::new();
                    let mut old_index = 0usize;
                    for (row_index, row) in hunk.rows.iter().enumerate() {
                        if row.prefix == b' ' || row.prefix == b'-' {
                            let dropped = (row.prefix == b' ')
                                && ((row_index < left_drop)
                                    || (row_index >= hunk.rows.len().saturating_sub(right_drop)));
                            if !dropped {
                                old_payloads.push(row.payload.as_str());
                            }
                            old_index += 1;
                        }
                    }
                    let _ = old_index;

                    let mut source_index = start + left_drop;
                    let mut matches = true;
                    for payload in old_payloads {
                        if source_index >= start + hunk.old_count - right_drop
                            || working[source_index] != payload
                        {
                            matches = false;
                            break;
                        }
                        source_index += 1;
                    }
                    if !matches {
                        continue;
                    }

                    let rank = (left_drop + right_drop, distance);
                    match best_rank {
                        None => {
                            best_rank = Some(rank);
                            best.clear();
                            best.push((start, left_drop, right_drop));
                        }
                        Some(current) if rank < current => {
                            best_rank = Some(rank);
                            best.clear();
                            best.push((start, left_drop, right_drop));
                        }
                        Some(current) if rank == current => {
                            best.push((start, left_drop, right_drop));
                        }
                        _ => {}
                    }
                }
            }
        }

        if best.is_empty() {
            return Err(ApplyError::NoMatch {
                hunk: hunk_index + 1,
            });
        }

        let mut starts: Vec<usize> = best.iter().map(|choice| choice.0).collect();
        starts.sort_unstable();
        starts.dedup();
        if starts.len() >= 2 {
            return Err(ApplyError::Ambiguous {
                hunk: hunk_index + 1,
                first: starts[0],
                second: starts[1],
            });
        }

        let start = starts[0];
        let mut choices: Vec<(usize, usize)> = best
            .iter()
            .filter(|choice| choice.0 == start)
            .map(|choice| (choice.1, choice.2))
            .collect();
        choices.sort_unstable();
        let (left_drop, right_drop) = choices[0];

        let mut replacement = Vec::new();
        for (row_index, row) in hunk.rows.iter().enumerate() {
            if row.prefix == b' ' {
                let dropped = row_index < left_drop
                    || row_index >= hunk.rows.len().saturating_sub(right_drop);
                if !dropped {
                    replacement.push(row.payload.clone());
                }
            } else if row.prefix == b'+' {
                replacement.push(row.payload.clone());
            }
        }

        let replace_start = start + left_drop;
        let replace_end = start + hunk.old_count - right_drop;
        working.splice(replace_start..replace_end, replacement);

        if working.len() > 128 {
            return Err(ApplyError::TooLarge);
        }

        drift = start as isize - (hunk.old_start as isize + net);
        net += hunk.new_count as isize - hunk.old_count as isize;
    }

    let mut output = String::new();
    for line in working {
        output.push_str(&line);
        output.push('\n');
    }
    Ok(output)
}