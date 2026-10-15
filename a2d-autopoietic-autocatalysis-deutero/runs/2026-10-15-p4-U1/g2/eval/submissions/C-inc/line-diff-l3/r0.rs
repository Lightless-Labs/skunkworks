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
struct BodyRow {
    prefix: u8,
    payload: String,
}

struct Hunk {
    line: usize,
    old_start: i64,
    old_count: usize,
    new_start: i64,
    new_count: usize,
    rows: Vec<BodyRow>,
}

fn parse_number(text: &str) -> Option<usize> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if text.len() > 1 && text.starts_with('0') {
        return None;
    }
    let value = text.parse::<usize>().ok()?;
    (value <= 128).then_some(value)
}

fn parse_hunk_header(line: &str) -> Option<(usize, usize, usize, usize)> {
    let rest = line.strip_prefix("@@ -")?;
    let (old_start, rest) = rest.split_once(',')?;
    let (old_count, rest) = rest.split_once(" +")?;
    let (new_start, rest) = rest.split_once(',')?;
    let new_count = rest.strip_suffix(" @@")?;

    Some((
        parse_number(old_start)?,
        parse_number(old_count)?,
        parse_number(new_start)?,
        parse_number(new_count)?,
    ))
}

fn physical_lines(patch: &str) -> Vec<&str> {
    let mut lines: Vec<&str> = patch.split('\n').collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    lines
}

fn source_lines(source: &str) -> Vec<String> {
    if source.is_empty() {
        Vec::new()
    } else {
        source[..source.len() - 1]
            .split('\n')
            .map(str::to_owned)
            .collect()
    }
}

fn abs_distance(a: i64, b: i64) -> i64 {
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

    let hunks = if patch.is_empty() {
        Vec::new()
    } else {
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

        let mut parsed = Vec::new();
        let mut index = 2;
        let mut previous_old_end = 0i64;
        let mut cumulative_delta = 0i64;

        while index < lines.len() {
            if parsed.len() == 16 {
                break;
            }
            let header_index = index;
            let header_line = header_index + 1;
            let Some((a, b, c, d)) = parse_hunk_header(lines[index]) else {
                return Err(ApplyError::BadHunkHeader { line: header_line });
            };
            if (b > 0 && a == 0) || (d > 0 && c == 0) {
                return Err(ApplyError::BadHunkHeader { line: header_line });
            }

            let old_start = if b > 0 { (a - 1) as i64 } else { a as i64 };
            let new_start = if d > 0 { (c - 1) as i64 } else { c as i64 };

            index += 1;
            let mut rows = Vec::new();
            let mut old_seen = 0usize;
            let mut new_seen = 0usize;

            while index < lines.len() && !lines[index].starts_with("@@") {
                let row_line = index + 1;
                let row = lines[index];
                let Some(prefix) = row.as_bytes().first().copied() else {
                    return Err(ApplyError::BadBody { line: row_line });
                };
                if !matches!(prefix, b' ' | b'-' | b'+') || row.as_bytes().contains(&b'\r') {
                    return Err(ApplyError::BadBody { line: row_line });
                }
                let payload = &row[1..];
                if payload.as_bytes().contains(&b'\r') {
                    return Err(ApplyError::BadBody { line: row_line });
                }
                if prefix != b'+' {
                    old_seen += 1;
                }
                if prefix != b'-' {
                    new_seen += 1;
                }
                rows.push(BodyRow {
                    prefix,
                    payload: payload.to_owned(),
                });
                index += 1;
            }

            if old_seen != b || new_seen != d {
                return Err(ApplyError::CountMismatch { line: header_line });
            }
            if !rows.iter().any(|r| r.prefix == b'+' || r.prefix == b'-') {
                return Err(ApplyError::BadBody { line: header_line });
            }

            if old_start < previous_old_end
                || new_start != old_start + cumulative_delta
                || new_start < 0
            {
                return Err(ApplyError::Misordered { line: header_line });
            }
            previous_old_end = old_start + b as i64;
            cumulative_delta += d as i64 - b as i64;

            parsed.push(Hunk {
                line: header_line,
                old_start,
                old_count: b,
                new_start,
                new_count: d,
                rows,
            });
        }

        if parsed.is_empty() {
            return Err(ApplyError::BadHunkHeader { line: 3 });
        }
        if parsed.len() == 16 && index < lines.len() {
            // The seventeenth header is checked through its ordering before TooLarge.
            let header_line = index + 1;
            let Some((a, b, c, d)) = parse_hunk_header(lines[index]) else {
                return Err(ApplyError::BadHunkHeader { line: header_line });
            };
            if (b > 0 && a == 0) || (d > 0 && c == 0) {
                return Err(ApplyError::BadHunkHeader { line: header_line });
            }
            let old_start = if b > 0 { (a - 1) as i64 } else { a as i64 };
            let new_start = if d > 0 { (c - 1) as i64 } else { c as i64 };
            if old_start < previous_old_end
                || new_start != old_start + cumulative_delta
                || new_start < 0
            {
                return Err(ApplyError::Misordered { line: header_line });
            }
            return Err(ApplyError::TooLarge);
        }
        parsed
    };

    if let Some(byte) = source.as_bytes().iter().position(|&b| b == b'\r') {
        return Err(ApplyError::SourceFormat { byte });
    }
    if !source.is_empty() && !source.ends_with('\n') {
        return Err(ApplyError::SourceMissingLf);
    }

    let mut working = source_lines(source);
    if working.len() > 128 {
        return Err(ApplyError::TooLarge);
    }

    let mut net = 0i64;
    let mut drift = 0i64;

    for (hunk_index, hunk) in hunks.iter().enumerate() {
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

        let expected = hunk.old_start + net + drift;
        let mut best_rank: Option<(usize, i64)> = None;
        let mut best_by_start: Vec<(usize, usize, usize)> = Vec::new();

        for left in 0..=options.fuzz.min(lead) {
            for right in 0..=options.fuzz.min(trail) {
                if left + right > hunk.old_count {
                    continue;
                }

                let mut match_payloads = Vec::new();
                let mut old_position = 0usize;
                for row in &hunk.rows {
                    if row.prefix == b'+' {
                        continue;
                    }
                    let drop_row =
                        (old_position < left) || (old_position >= hunk.old_count - right);
                    if !drop_row {
                        match_payloads.push(row.payload.as_str());
                    }
                    old_position += 1;
                }

                for start in 0..=working.len().saturating_sub(hunk.old_count) {
                    if start + hunk.old_count > working.len() {
                        continue;
                    }
                    let distance = abs_distance(start as i64, expected);
                    if distance > options.max_offset as i64 {
                        continue;
                    }

                    let mut source_index = start + left;
                    let end = start + hunk.old_count - right;
                    let mut matches = true;
                    for payload in &match_payloads {
                        if source_index >= end || working[source_index] != *payload {
                            matches = false;
                            break;
                        }
                        source_index += 1;
                    }
                    if !matches || source_index != end {
                        continue;
                    }

                    let rank = (left + right, distance);
                    match best_rank {
                        None => {
                            best_rank = Some(rank);
                            best_by_start.clear();
                            best_by_start.push((start, left, right));
                        }
                        Some(current) if rank < current => {
                            best_rank = Some(rank);
                            best_by_start.clear();
                            best_by_start.push((start, left, right));
                        }
                        Some(current) if rank == current => {
                            if let Some(entry) =
                                best_by_start.iter_mut().find(|entry| entry.0 == start)
                            {
                                if left < entry.1 {
                                    *entry = (start, left, right);
                                }
                            } else {
                                best_by_start.push((start, left, right));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        if best_by_start.is_empty() {
            return Err(ApplyError::NoMatch {
                hunk: hunk_index + 1,
            });
        }
        best_by_start.sort_by_key(|entry| entry.0);
        if best_by_start.len() > 1 {
            return Err(ApplyError::Ambiguous {
                hunk: hunk_index + 1,
                first: best_by_start[0].0,
                second: best_by_start[1].0,
            });
        }

        let (start, left, right) = best_by_start[0];
        let mut replacement = Vec::new();
        let mut old_position = 0usize;
        for row in &hunk.rows {
            match row.prefix {
                b'+' => replacement.push(row.payload.clone()),
                b'-' => old_position += 1,
                b' ' => {
                    let is_dropped =
                        old_position < left || old_position >= hunk.old_count - right;
                    if !is_dropped {
                        replacement.push(row.payload.clone());
                    }
                    old_position += 1;
                }
                _ => unreachable!(),
            }
        }

        let replace_start = start + left;
        let replace_end = start + hunk.old_count - right;
        working.splice(replace_start..replace_end, replacement);

        if working.len() > 128 {
            return Err(ApplyError::TooLarge);
        }

        let previous_net = net;
        net += hunk.new_count as i64 - hunk.old_count as i64;
        drift = start as i64 - (hunk.old_start + previous_net);
    }

    let mut output = String::new();
    for line in working {
        output.push_str(&line);
        output.push('\n');
    }
    Ok(output)
}

fn main() {}