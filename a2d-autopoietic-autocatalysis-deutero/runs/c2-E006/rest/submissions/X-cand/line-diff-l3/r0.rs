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

fn parse_number(s: &str) -> Option<usize> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if s.len() > 1 && s.starts_with('0') {
        return None;
    }
    let value = s.parse::<usize>().ok()?;
    (value <= 128).then_some(value)
}

fn parse_hunk_header(line: &str) -> Option<(usize, usize, usize, usize)> {
    let inner = line.strip_prefix("@@ -")?.strip_suffix(" @@")?;
    let mut parts = inner.split(' ');
    let old = parts.next()?;
    let new = parts.next()?;
    if parts.next().is_some() {
        return None;
    }

    let (a, b) = old.split_once(',')?;
    let (c, d) = new.strip_prefix('+')?.split_once(',')?;
    let a = parse_number(a)?;
    let b = parse_number(b)?;
    let c = parse_number(c)?;
    let d = parse_number(d)?;

    if old.bytes().filter(|&x| x == b',').count() != 1
        || new.bytes().filter(|&x| x == b',').count() != 1
        || (b > 0 && a == 0)
        || (d > 0 && c == 0)
    {
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
            line: 1 + patch.bytes().filter(|&b| b == b'\n').count(),
        });
    }

    let content = &patch[..patch.len() - 1];
    let lines: Vec<&str> = content.split('\n').collect();

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
    let mut pos = 2usize;
    let mut previous_old_end = 0usize;
    let mut preceding_delta: i128 = 0;
    let mut have_previous = false;

    while pos < lines.len() {
        let header_index = pos;
        let header_line = pos + 1;
        let Some((a, b, c, d)) = parse_hunk_header(lines[pos]) else {
            return Err(ApplyError::BadHunkHeader { line: header_line });
        };
        let old_start = full_start(a, b);
        let new_start = full_start(c, d);
        pos += 1;

        let mut rows = Vec::new();
        let mut old_seen = 0usize;
        let mut new_seen = 0usize;

        while pos < lines.len() && !lines[pos].starts_with("@@") {
            let row_line = pos + 1;
            let text = lines[pos];
            let Some((&prefix, payload)) = text.as_bytes().split_first() else {
                return Err(ApplyError::BadBody { line: row_line });
            };
            if !matches!(prefix, b' ' | b'-' | b'+') || payload.contains(&b'\r') {
                return Err(ApplyError::BadBody { line: row_line });
            }
            let payload = std::str::from_utf8(payload).unwrap().to_owned();
            if prefix == b' ' || prefix == b'-' {
                old_seen += 1;
            }
            if prefix == b' ' || prefix == b'+' {
                new_seen += 1;
            }
            rows.push(Row { prefix, payload });
            pos += 1;
        }

        if old_seen != b || new_seen != d {
            return Err(ApplyError::CountMismatch { line: header_line });
        }
        if !rows.iter().any(|r| r.prefix == b'+' || r.prefix == b'-') {
            return Err(ApplyError::BadBody { line: header_line });
        }

        if have_previous && old_start < previous_old_end {
            return Err(ApplyError::Misordered { line: header_line });
        }
        let required_new = old_start as i128 + preceding_delta;
        if required_new < 0 || new_start as i128 != required_new {
            return Err(ApplyError::Misordered { line: header_line });
        }

        if hunks.len() == 16 {
            return Err(ApplyError::TooLarge);
        }

        previous_old_end = old_start + b;
        have_previous = true;
        preceding_delta += d as i128 - b as i128;
        hunks.push(Hunk {
            line: header_index + 1,
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
        .map(str::to_owned)
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
    let mut working = source_lines(source)?;
    let mut net: i128 = 0;
    let mut drift: i128 = 0;

    for (index, hunk) in hunks.iter().enumerate() {
        let mut lead = 0usize;
        while lead < hunk.rows.len() && hunk.rows[lead].prefix == b' ' {
            lead += 1;
        }
        let mut trail = 0usize;
        while trail < hunk.rows.len()
            && hunk.rows[hunk.rows.len() - 1 - trail].prefix == b' '
        {
            trail += 1;
        }

        let expected = hunk.old_start as i128 + net + drift;
        let max_left = options.fuzz.min(lead);
        let max_right = options.fuzz.min(trail);
        let mut best_rank: Option<(usize, i128)> = None;
        let mut best_choices: Vec<(usize, usize, usize)> = Vec::new();

        for left in 0..=max_left {
            for right in 0..=max_right {
                if left + right > hunk.old_count {
                    continue;
                }

                let mut old_payloads = Vec::new();
                for row in &hunk.rows[left..hunk.rows.len() - right] {
                    if row.prefix == b' ' || row.prefix == b'-' {
                        old_payloads.push(row.payload.as_str());
                    }
                }

                for start in 0..=working.len() {
                    if start + hunk.old_count > working.len() {
                        continue;
                    }
                    let distance = (start as i128 - expected).abs();
                    if distance > options.max_offset as i128 {
                        continue;
                    }

                    let match_start = start + left;
                    let match_end = start + hunk.old_count - right;
                    if old_payloads.len() != match_end - match_start
                        || !old_payloads
                            .iter()
                            .zip(&working[match_start..match_end])
                            .all(|(expected, actual)| *expected == actual)
                    {
                        continue;
                    }

                    let rank = (left + right, distance);
                    match best_rank {
                        None => {
                            best_rank = Some(rank);
                            best_choices.clear();
                            best_choices.push((start, left, right));
                        }
                        Some(best) if rank < best => {
                            best_rank = Some(rank);
                            best_choices.clear();
                            best_choices.push((start, left, right));
                        }
                        Some(best) if rank == best => {
                            best_choices.push((start, left, right));
                        }
                        _ => {}
                    }
                }
            }
        }

        if best_choices.is_empty() {
            return Err(ApplyError::NoMatch { hunk: index + 1 });
        }

        best_choices.sort_by_key(|&(start, left, right)| (start, left, right));
        let mut starts: Vec<usize> = best_choices.iter().map(|choice| choice.0).collect();
        starts.sort_unstable();
        starts.dedup();
        if starts.len() >= 2 {
            return Err(ApplyError::Ambiguous {
                hunk: index + 1,
                first: starts[0],
                second: starts[1],
            });
        }

        let (start, left, right) = best_choices[0];
        let previous_net = net;
        let replace_start = start + left;
        let replace_end = start + hunk.old_count - right;
        let trimmed = &hunk.rows[left..hunk.rows.len() - right];
        let replacement: Vec<String> = trimmed
            .iter()
            .filter(|row| row.prefix == b' ' || row.prefix == b'+')
            .map(|row| row.payload.clone())
            .collect();

        working.splice(replace_start..replace_end, replacement);
        if working.len() > 128 {
            return Err(ApplyError::TooLarge);
        }

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