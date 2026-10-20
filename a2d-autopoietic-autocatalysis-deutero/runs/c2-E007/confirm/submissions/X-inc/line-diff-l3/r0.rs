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

struct Hunk {
    old_start: usize,
    old_count: usize,
    new_start: usize,
    new_count: usize,
    header_line: usize,
    rows: Vec<Row>,
}

fn parse_number(s: &str) -> Option<usize> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if s.len() > 1 && s.starts_with('0') {
        return None;
    }
    let n = s.parse::<usize>().ok()?;
    (n <= 128).then_some(n)
}

fn parse_hunk_header(line: &str) -> Option<(usize, usize, usize, usize)> {
    let inner = line.strip_prefix("@@ -")?.strip_suffix(" @@")?;
    let (old, new) = inner.split_once(" +")?;
    let (a, b) = old.split_once(',')?;
    let (c, d) = new.split_once(',')?;
    if a.contains(',') || b.contains(',') || c.contains(',') {
        return None;
    }
    let (a, b, c, d) = (
        parse_number(a)?,
        parse_number(b)?,
        parse_number(c)?,
        parse_number(d)?,
    );
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

    let lines: Vec<&str> = patch[..patch.len() - 1].split('\n').collect();
    for (i, expected) in ["--- before", "+++ after"].iter().enumerate() {
        if lines.get(i).copied() != Some(*expected) {
            return Err(ApplyError::BadHeader { line: i + 1 });
        }
    }
    if lines.len() < 3 {
        return Err(ApplyError::BadHunkHeader { line: 3 });
    }

    let mut hunks = Vec::new();
    let mut i = 2;
    let mut previous_old_end = 0usize;
    let mut cumulative_delta: i128 = 0;

    while i < lines.len() {
        let header_line = i + 1;
        let Some((a, b, c, d)) = parse_hunk_header(lines[i]) else {
            return Err(ApplyError::BadHunkHeader { line: header_line });
        };
        let old_start = full_start(a, b);
        let new_start = full_start(c, d);
        i += 1;

        let mut rows = Vec::new();
        let mut old_seen = 0usize;
        let mut new_seen = 0usize;
        while i < lines.len() && !lines[i].starts_with("@@") {
            let row_line = i + 1;
            let row = lines[i];
            if row.is_empty() {
                return Err(ApplyError::BadBody { line: row_line });
            }
            let (kind, payload) = match row.as_bytes()[0] {
                b' ' => (b' ', &row[1..]),
                b'-' => (b'-', &row[1..]),
                b'+' => (b'+', &row[1..]),
                _ => return Err(ApplyError::BadBody { line: row_line }),
            };
            if payload.contains('\r') {
                return Err(ApplyError::BadBody { line: row_line });
            }
            if kind != b'+' {
                old_seen += 1;
            }
            if kind != b'-' {
                new_seen += 1;
            }
            rows.push(Row {
                kind,
                payload: payload.to_owned(),
                line: row_line,
            });
            i += 1;
        }

        if old_seen != b || new_seen != d {
            return Err(ApplyError::CountMismatch { line: header_line });
        }
        if !rows.iter().any(|r| r.kind == b'+' || r.kind == b'-') {
            return Err(ApplyError::BadBody { line: header_line });
        }

        if old_start < previous_old_end {
            return Err(ApplyError::Misordered { line: header_line });
        }
        let required_new = old_start as i128 + cumulative_delta;
        if required_new < 0 || new_start as i128 != required_new {
            return Err(ApplyError::Misordered { line: header_line });
        }
        previous_old_end = old_start + b;
        cumulative_delta += d as i128 - b as i128;

        hunks.push(Hunk {
            old_start,
            old_count: b,
            new_start,
            new_count: d,
            header_line,
            rows,
        });
        if hunks.len() > 16 {
            return Err(ApplyError::TooLarge);
        }
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

fn abs_distance(a: i128, b: i128) -> i128 {
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
    let mut net: i128 = 0;
    let mut drift: i128 = 0;

    for (hunk_index, hunk) in hunks.iter().enumerate() {
        let mut lead = 0usize;
        while lead < hunk.rows.len() && hunk.rows[lead].kind == b' ' {
            lead += 1;
        }
        let mut trail = 0usize;
        while trail < hunk.rows.len()
            && hunk.rows[hunk.rows.len() - 1 - trail].kind == b' '
        {
            trail += 1;
        }

        let expected = hunk.old_start as i128 + net + drift;
        let mut best_rank: Option<(usize, i128)> = None;
        let mut best_positions = Vec::<(usize, usize, usize)>::new();

        for left in 0..=options.fuzz.min(lead) {
            for right in 0..=options.fuzz.min(trail) {
                if left + right > hunk.rows.len() {
                    continue;
                }
                let mut old_payloads = Vec::new();
                for row in &hunk.rows[left..hunk.rows.len() - right] {
                    if row.kind != b'+' {
                        old_payloads.push(row.payload.as_str());
                    }
                }

                if old_payloads.len() != hunk.old_count - left - right {
                    continue;
                }

                if hunk.old_count > working.len() {
                    continue;
                }
                for s in 0..=working.len() - hunk.old_count {
                    let distance = abs_distance(s as i128, expected);
                    if distance > options.max_offset as i128 {
                        continue;
                    }
                    let source_start = s + left;
                    let source_end = s + hunk.old_count - right;
                    if source_end > working.len() {
                        continue;
                    }
                    if old_payloads
                        .iter()
                        .zip(&working[source_start..source_end])
                        .any(|(expected_line, actual)| *expected_line != actual)
                    {
                        continue;
                    }

                    let rank = (left + right, distance);
                    match best_rank {
                        None => {
                            best_rank = Some(rank);
                            best_positions.clear();
                            best_positions.push((s, left, right));
                        }
                        Some(current) if rank < current => {
                            best_rank = Some(rank);
                            best_positions.clear();
                            best_positions.push((s, left, right));
                        }
                        Some(current) if rank == current => {
                            best_positions.push((s, left, right));
                        }
                        _ => {}
                    }
                }
            }
        }

        if best_positions.is_empty() {
            return Err(ApplyError::NoMatch {
                hunk: hunk_index + 1,
            });
        }

        best_positions.sort_by_key(|&(s, left, _)| (s, left));
        let mut distinct = best_positions.iter().map(|x| x.0).collect::<Vec<_>>();
        distinct.sort_unstable();
        distinct.dedup();
        if distinct.len() >= 2 {
            return Err(ApplyError::Ambiguous {
                hunk: hunk_index + 1,
                first: distinct[0],
                second: distinct[1],
            });
        }

        let (s, left, right) = best_positions[0];
        let mut replacement = Vec::new();
        for row in &hunk.rows[left..hunk.rows.len() - right] {
            if row.kind != b'-' {
                replacement.push(row.payload.clone());
            }
        }
        let replace_start = s + left;
        let replace_end = s + hunk.old_count - right;
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