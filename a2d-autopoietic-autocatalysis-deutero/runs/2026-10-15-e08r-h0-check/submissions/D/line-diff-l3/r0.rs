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

#[derive(Debug, Clone)]
struct Row {
    kind: u8,
    payload: String,
}

#[derive(Debug, Clone)]
struct Hunk {
    line: usize,
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
    if !s.bytes().all(|b| b.is_ascii_digit()) {
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
    if a.contains(',') || b.contains(',') || c.contains(',') || d.contains(',') {
        return None;
    }
    Some((
        parse_decimal(a)?,
        parse_decimal(b)?,
        parse_decimal(c)?,
        parse_decimal(d)?,
    ))
}

fn physical_lines(s: &str) -> Vec<&str> {
    if s.is_empty() {
        return Vec::new();
    }
    s[..s.len() - 1].split('\n').collect()
}

fn full_start(start: usize, count: usize) -> usize {
    if count > 0 { start - 1 } else { start }
}

fn apply_unified(
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
        if lines.get(0).copied() != Some("--- before") {
            return Err(ApplyError::BadHeader { line: 1 });
        }
        if lines.get(1).copied() != Some("+++ after") {
            return Err(ApplyError::BadHeader { line: 2 });
        }
        if lines.len() < 3 || !lines[2].starts_with("@@") {
            return Err(ApplyError::BadHunkHeader { line: 3 });
        }

        let mut index = 2;
        let mut previous_old_end = 0usize;
        let mut cumulative_delta: i128 = 0;

        while index < lines.len() {
            if !lines[index].starts_with("@@") {
                return Err(ApplyError::BadHunkHeader { line: index + 1 });
            }
            let header_line = index + 1;
            let (a, b, c, d) = parse_hunk_header(lines[index])
                .ok_or(ApplyError::BadHunkHeader { line: header_line })?;
            let old_start = if b > 0 {
                if a == 0 {
                    return Err(ApplyError::BadHunkHeader { line: header_line });
                }
                a - 1
            } else {
                a
            };
            let new_start = if d > 0 {
                if c == 0 {
                    return Err(ApplyError::BadHunkHeader { line: header_line });
                }
                c - 1
            } else {
                c
            };

            index += 1;
            let mut rows = Vec::new();
            let mut old_seen = 0usize;
            let mut new_seen = 0usize;
            while index < lines.len() && !lines[index].starts_with("@@") {
                let row_line = index + 1;
                let bytes = lines[index].as_bytes();
                if bytes.is_empty() || !matches!(bytes[0], b' ' | b'-' | b'+') {
                    return Err(ApplyError::BadBody { line: row_line });
                }
                if lines[index].contains('\r') {
                    return Err(ApplyError::BadBody { line: row_line });
                }
                let payload = &lines[index][1..];
                let kind = bytes[0];
                if kind != b'+' {
                    old_seen += 1;
                }
                if kind != b'-' {
                    new_seen += 1;
                }
                rows.push(Row {
                    kind,
                    payload: payload.to_string(),
                });
                index += 1;
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

    let mut net: i128 = 0;
    let mut drift: i128 = 0;

    for (hunk_index, hunk) in hunks.iter().enumerate() {
        let lead = hunk.rows.iter().take_while(|r| r.kind == b' ').count();
        let trail = hunk.rows.iter().rev().take_while(|r| r.kind == b' ').count();
        let max_l = options.fuzz.min(lead);
        let max_r = options.fuzz.min(trail);
        let expected = hunk.old_start as i128 + net + drift;

        let mut best_rank: Option<(usize, i128)> = None;
        let mut best_starts = Vec::new();
        let mut best_l_by_start = Vec::new();

        for l in 0..=max_l {
            for r in 0..=max_r {
                if l + r > hunk.rows.len() {
                    continue;
                }
                let mut old_payloads = Vec::new();
                let mut new_payloads = Vec::new();
                for row in hunk.rows.iter().skip(l).take(hunk.rows.len() - l - r) {
                    if row.kind != b'+' {
                        old_payloads.push(row.payload.as_str());
                    }
                    if row.kind != b'-' {
                        new_payloads.push(row.payload.as_str());
                    }
                }
                let old_len = hunk.old_count - l - r;
                if old_payloads.len() != old_len {
                    continue;
                }

                if old_len > working.len() {
                    continue;
                }
                for s in 0..=working.len() - hunk.old_count {
                    let distance = (s as i128 - expected).abs();
                    if distance > options.max_offset as i128 {
                        continue;
                    }
                    let slice_start = s + l;
                    let slice_end = s + hunk.old_count - r;
                    if old_payloads
                        .iter()
                        .zip(&working[slice_start..slice_end])
                        .any(|(expected, actual)| *expected != actual)
                    {
                        continue;
                    }

                    let rank = (l + r, distance);
                    match best_rank {
                        None => {
                            best_rank = Some(rank);
                            best_starts.clear();
                            best_l_by_start.clear();
                        }
                        Some(current) if rank < current => {
                            best_rank = Some(rank);
                            best_starts.clear();
                            best_l_by_start.clear();
                        }
                        Some(current) if rank > current => continue,
                        _ => {}
                    }
                    if let Some(pos) = best_starts.iter().position(|&x| x == s) {
                        if l < best_l_by_start[pos] {
                            best_l_by_start[pos] = l;
                        }
                    } else {
                        best_starts.push(s);
                        best_l_by_start.push(l);
                    }
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

        let s = best_starts[0];
        let l = best_l_by_start[0];
        let r = (0..=max_r)
            .find(|&r| {
                l + r <= hunk.rows.len()
                    && hunk.rows.len() - l - r
                        == hunk.rows.iter().skip(l).take_while(|_| true).count()
                    && {
                        let payloads: Vec<&str> = hunk
                            .rows
                            .iter()
                            .skip(l)
                            .take(hunk.rows.len() - l - r)
                            .filter(|row| row.kind != b'+')
                            .map(|row| row.payload.as_str())
                            .collect();
                        let start = s + l;
                        let end = s + hunk.old_count - r;
                        payloads.len() == end - start
                            && payloads
                                .iter()
                                .zip(&working[start..end])
                                .all(|(a, b)| *a == b)
                    }
            })
            .unwrap_or(0);

        let replacement: Vec<String> = hunk
            .rows
            .iter()
            .skip(l)
            .take(hunk.rows.len() - l - r)
            .filter(|row| row.kind != b'-')
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

    let mut result = String::new();
    for line in working {
        result.push_str(&line);
        result.push('\n');
    }
    Ok(result)
}