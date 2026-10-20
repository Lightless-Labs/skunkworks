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
}

struct Hunk {
    line: usize,
    old_start: usize,
    old_count: usize,
    new_start: usize,
    new_count: usize,
    rows: Vec<Row>,
}

fn patch_lines(patch: &str) -> Vec<&str> {
    let bytes = patch.as_bytes();
    let mut result = Vec::new();
    let mut begin = 0;
    for (i, &byte) in bytes.iter().enumerate() {
        if byte == b'\n' {
            result.push(&patch[begin..i]);
            begin = i + 1;
        }
    }
    result
}

fn parse_decimal(s: &str) -> Option<usize> {
    if s.is_empty()
        || (s.len() > 1 && s.starts_with('0'))
        || !s.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let mut n = 0usize;
    for b in s.bytes() {
        n = n.checked_mul(10)?.checked_add((b - b'0') as usize)?;
        if n > 128 {
            return None;
        }
    }
    Some(n)
}

fn parse_hunk_header(s: &str) -> Option<(usize, usize, usize, usize)> {
    let inner = s.strip_prefix("@@ -")?.strip_suffix(" @@")?;
    let (old, new) = inner.split_once(" +")?;
    let (a, b) = old.split_once(',')?;
    let (c, d) = new.split_once(',')?;
    if b.contains(',') || d.contains(',') {
        return None;
    }
    let (a, b, c, d) = (
        parse_decimal(a)?,
        parse_decimal(b)?,
        parse_decimal(c)?,
        parse_decimal(d)?,
    );
    if (b > 0 && a == 0) || (d > 0 && c == 0) {
        return None;
    }
    Some((a, b, c, d))
}

fn full_start(start: usize, count: usize) -> i128 {
    if count > 0 {
        start as i128 - 1
    } else {
        start as i128
    }
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
    for (idx, expected) in ["--- before", "+++ after"].iter().enumerate() {
        if lines.get(idx).copied() != Some(*expected) {
            return Err(ApplyError::BadHeader { line: idx + 1 });
        }
    }
    if lines.len() <= 2 || !lines[2].starts_with("@@") {
        return Err(ApplyError::BadHunkHeader { line: 3 });
    }

    let mut hunks = Vec::new();
    let mut i = 2usize;
    let mut previous_old_end = 0i128;
    let mut accumulated_delta = 0i128;

    while i < lines.len() {
        let header_line = i + 1;
        if !lines[i].starts_with("@@") {
            return Err(ApplyError::BadHunkHeader { line: header_line });
        }
        let (a, b, c, d) =
            parse_hunk_header(lines[i]).ok_or(ApplyError::BadHunkHeader {
                line: header_line,
            })?;
        let old_start = full_start(a, b);
        let new_start = full_start(c, d);

        i += 1;
        let body_begin = i;
        let mut rows = Vec::new();
        let (mut old_seen, mut new_seen) = (0usize, 0usize);
        while i < lines.len() && !lines[i].starts_with("@@") {
            let line = lines[i];
            let bytes = line.as_bytes();
            if bytes.is_empty()
                || !matches!(bytes[0], b' ' | b'-' | b'+')
                || bytes[1..].contains(&b'\r')
            {
                return Err(ApplyError::BadBody { line: i + 1 });
            }
            let kind = bytes[0];
            let payload = &line[1..];
            rows.push(Row {
                kind,
                payload: payload.to_string(),
            });
            if kind == b' ' || kind == b'-' {
                old_seen += 1;
            }
            if kind == b' ' || kind == b'+' {
                new_seen += 1;
            }
            i += 1;
        }
        let _ = body_begin;

        if old_seen != b || new_seen != d {
            return Err(ApplyError::CountMismatch { line: header_line });
        }
        if !rows.iter().any(|r| r.kind == b'+' || r.kind == b'-') {
            return Err(ApplyError::BadBody { line: header_line });
        }
        if old_start < previous_old_end
            || new_start != old_start + accumulated_delta
        {
            return Err(ApplyError::Misordered { line: header_line });
        }
        previous_old_end = old_start + b as i128;
        accumulated_delta += d as i128 - b as i128;

        if hunks.len() == 16 {
            return Err(ApplyError::TooLarge);
        }
        hunks.push(Hunk {
            line: header_line,
            old_start: old_start as usize,
            old_count: b,
            new_start: new_start as usize,
            new_count: d,
            rows,
        });
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

fn abs_diff(a: i128, b: i128) -> i128 {
    (a - b).abs()
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
    let mut net = 0i128;
    let mut drift = 0i128;

    for (hunk_index, hunk) in hunks.iter().enumerate() {
        let mut lead = 0usize;
        while lead < hunk.rows.len() && hunk.rows[lead].kind == b' ' {
            lead += 1;
        }
        let mut trail = 0usize;
        while trail < hunk.rows.len() - lead
            && hunk.rows[hunk.rows.len() - 1 - trail].kind == b' '
        {
            trail += 1;
        }

        let expected = hunk.old_start as i128 + net + drift;
        let max_lead = options.fuzz.min(lead);
        let max_trail = options.fuzz.min(trail);

        let mut best_rank: Option<(usize, i128)> = None;
        let mut best_starts = Vec::<usize>::new();
        let mut best_by_start = Vec::<(usize, usize, usize)>::new();

        for dropped_left in 0..=max_lead {
            for dropped_right in 0..=max_trail {
                let mut old_payloads = Vec::<&str>::new();
                for row in hunk
                    .rows
                    .iter()
                    .skip(dropped_left)
                    .take(hunk.rows.len().saturating_sub(dropped_left + dropped_right))
                {
                    if row.kind == b' ' || row.kind == b'-' {
                        old_payloads.push(&row.payload);
                    }
                }
                let retained_old = old_payloads.len();
                if retained_old + dropped_left + dropped_right != hunk.old_count {
                    continue;
                }

                if hunk.old_count > working.len() {
                    continue;
                }
                for start in 0..=working.len() - hunk.old_count {
                    let distance = abs_diff(start as i128, expected);
                    if distance > options.max_offset as i128 {
                        continue;
                    }
                    let region_start = start + dropped_left;
                    let region_end = start + hunk.old_count - dropped_right;
                    if region_end - region_start != retained_old {
                        continue;
                    }
                    if !old_payloads
                        .iter()
                        .zip(&working[region_start..region_end])
                        .all(|(a, b)| *a == b)
                    {
                        continue;
                    }

                    let rank = (dropped_left + dropped_right, distance);
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
                    if !best_starts.contains(&start) {
                        best_starts.push(start);
                    }
                    best_by_start.push((start, dropped_left, dropped_right));
                }
            }
        }

        if best_starts.is_empty() {
            return Err(ApplyError::NoMatch {
                hunk: hunk_index + 1,
            });
        }
        best_starts.sort_unstable();
        if best_starts.len() > 1 {
            return Err(ApplyError::Ambiguous {
                hunk: hunk_index + 1,
                first: best_starts[0],
                second: best_starts[1],
            });
        }

        let start = best_starts[0];
        let (_, dropped_left, dropped_right) = best_by_start
            .iter()
            .filter(|(s, _, _)| *s == start)
            .min_by_key(|(_, left, _)| *left)
            .copied()
            .unwrap();

        let trim_end = hunk.rows.len() - dropped_right;
        let mut replacement = Vec::new();
        for row in &hunk.rows[dropped_left..trim_end] {
            if row.kind == b' ' || row.kind == b'+' {
                replacement.push(row.payload.clone());
            }
        }
        let replace_start = start + dropped_left;
        let replace_end = start + hunk.old_count - dropped_right;
        working.splice(replace_start..replace_end, replacement);

        if working.len() > 128 {
            return Err(ApplyError::TooLarge);
        }

        let previous_net = net;
        net += hunk.new_count as i128 - hunk.old_count as i128;
        drift = start as i128 - (hunk.old_start as i128 + previous_net);
    }

    let mut output = String::new();
    for line in working {
        output.push_str(&line);
        output.push('\n');
    }
    Ok(output)
}