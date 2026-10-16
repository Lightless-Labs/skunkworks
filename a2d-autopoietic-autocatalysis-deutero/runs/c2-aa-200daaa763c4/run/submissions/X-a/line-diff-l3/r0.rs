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
    old_start: i64,
    old_count: usize,
    new_start: i64,
    new_count: usize,
    rows: Vec<Row>,
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

fn parse_hunk_header(s: &str) -> Option<(usize, usize, usize, usize)> {
    let body = s.strip_prefix("@@ -")?.strip_suffix(" @@")?;
    let (old, new) = body.split_once(" +")?;
    if new.contains(' ') || old.contains(' ') {
        return None;
    }
    let (a, b) = old.split_once(',')?;
    let (c, d) = new.split_once(',')?;
    if b.contains(',') || d.contains(',') {
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

fn full_start(start: usize, count: usize) -> i64 {
    if count > 0 {
        (start - 1) as i64
    } else {
        start as i64
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

    let lines: Vec<&str> = patch[..patch.len() - 1].split('\n').collect();
    for (i, expected) in ["--- before", "+++ after"].iter().enumerate() {
        if lines.get(i) != Some(expected) {
            return Err(ApplyError::BadHeader { line: i + 1 });
        }
    }
    if lines.len() < 3 {
        return Err(ApplyError::BadHunkHeader { line: 3 });
    }

    let mut hunks = Vec::new();
    let mut idx = 2;
    let mut prev_old_end: i64 = 0;
    let mut preceding_net: i64 = 0;

    while idx < lines.len() {
        let header_line = idx + 1;
        let Some((a, b, c, d)) = parse_hunk_header(lines[idx]) else {
            return Err(ApplyError::BadHunkHeader { line: header_line });
        };
        let old_start = full_start(a, b);
        let new_start = full_start(c, d);
        let mut rows = Vec::new();
        idx += 1;

        while idx < lines.len() && !lines[idx].starts_with("@@") {
            let line = lines[idx];
            let row_line = idx + 1;
            let Some((&prefix, payload)) = line.as_bytes().split_first() else {
                return Err(ApplyError::BadBody { line: row_line });
            };
            if !matches!(prefix, b' ' | b'-' | b'+') || payload.contains(&b'\r') {
                return Err(ApplyError::BadBody { line: row_line });
            }
            rows.push(Row {
                prefix,
                payload: line[1..].to_owned(),
            });
            idx += 1;
        }

        let old_seen = rows.iter().filter(|r| r.prefix == b' ' || r.prefix == b'-').count();
        let new_seen = rows.iter().filter(|r| r.prefix == b' ' || r.prefix == b'+').count();
        if old_seen != b || new_seen != d {
            return Err(ApplyError::CountMismatch { line: header_line });
        }
        if !rows.iter().any(|r| r.prefix == b'+' || r.prefix == b'-') {
            return Err(ApplyError::BadBody { line: header_line });
        }

        let old_count = b;
        let new_count = d;
        if old_start < prev_old_end || new_start != old_start + preceding_net {
            return Err(ApplyError::Misordered { line: header_line });
        }
        prev_old_end = old_start + old_count as i64;
        preceding_net += new_count as i64 - old_count as i64;

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
    Ok(hunks)
}

fn source_lines(source: &str) -> Result<Vec<String>, ApplyError> {
    if let Some(byte) = source.bytes().position(|b| b == b'\r') {
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

fn abs_distance(a: i64, b: i64) -> u64 {
    (a - b).unsigned_abs()
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
    let mut net: i64 = 0;
    let mut drift: i64 = 0;

    for (hunk_index, hunk) in hunks.iter().enumerate() {
        let lead = hunk.rows.iter().take_while(|r| r.prefix == b' ').count();
        let trail = hunk.rows.iter().rev().take_while(|r| r.prefix == b' ').count();
        let max_l = options.fuzz.min(lead);
        let max_r = options.fuzz.min(trail);
        let expected = hunk.old_start + net + drift;

        let mut best_rank: Option<(usize, u64)> = None;
        let mut best_starts = Vec::<usize>::new();
        let mut best_choice: Option<(usize, usize, usize)> = None;

        for l in 0..=max_l {
            for r in 0..=max_r {
                if l + r > lead + trail {
                    continue;
                }
                let mut pattern = Vec::<&str>::new();
                let mut leading = l;
                let mut trailing = r;
                for row in &hunk.rows {
                    if row.prefix == b' ' && leading > 0 {
                        leading -= 1;
                        continue;
                    }
                    if row.prefix == b' ' {
                        pattern.push(&row.payload);
                    } else if row.prefix == b'-' {
                        pattern.push(&row.payload);
                    }
                }
                if trailing > 0 {
                    let mut to_remove = trailing;
                    while to_remove > 0 && !pattern.is_empty() {
                        // Only edge context rows may be removed. Rebuild via row slicing below.
                        to_remove -= 1;
                        pattern.pop();
                    }
                }

                let consumed = hunk.old_count.saturating_sub(l + r);
                if pattern.len() != consumed {
                    continue;
                }

                if hunk.old_count > working.len() {
                    continue;
                }
                for s in 0..=working.len() - hunk.old_count {
                    let dist = abs_distance(s as i64, expected);
                    if dist > options.max_offset as u64 {
                        continue;
                    }
                    let start = s + l;
                    let end = s + hunk.old_count - r;
                    if end > working.len() || start > end {
                        continue;
                    }
                    if !pattern
                        .iter()
                        .zip(&working[start..end])
                        .all(|(p, actual)| *p == actual)
                    {
                        continue;
                    }

                    let rank = (l + r, dist);
                    match best_rank {
                        None => {
                            best_rank = Some(rank);
                            best_starts.clear();
                            best_starts.push(s);
                            best_choice = Some((s, l, r));
                        }
                        Some(old_rank) if rank < old_rank => {
                            best_rank = Some(rank);
                            best_starts.clear();
                            best_starts.push(s);
                            best_choice = Some((s, l, r));
                        }
                        Some(old_rank) if rank == old_rank => {
                            if !best_starts.contains(&s) {
                                best_starts.push(s);
                            }
                            if best_choice.is_none_or(|(bs, bl, _)| s < bs || (s == bs && l < bl)) {
                                best_choice = Some((s, l, r));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        let Some((s, l, r)) = best_choice else {
            return Err(ApplyError::NoMatch { hunk: hunk_index + 1 });
        };
        best_starts.sort_unstable();
        if best_starts.len() > 1 {
            return Err(ApplyError::Ambiguous {
                hunk: hunk_index + 1,
                first: best_starts[0],
                second: best_starts[1],
            });
        }

        let mut trimmed = Vec::<&Row>::new();
        let mut skip_lead = l;
        let mut skip_trail = r;
        for row in &hunk.rows {
            if row.prefix == b' ' && skip_lead > 0 {
                skip_lead -= 1;
            } else {
                trimmed.push(row);
            }
        }
        while skip_trail > 0 {
            if let Some(pos) = trimmed.iter().rposition(|row| row.prefix == b' ') {
                trimmed.remove(pos);
                skip_trail -= 1;
            } else {
                break;
            }
        }

        let replace_start = s + l;
        let replace_end = s + hunk.old_count - r;
        let replacement: Vec<String> = trimmed
            .iter()
            .filter(|row| row.prefix == b' ' || row.prefix == b'+')
            .map(|row| row.payload.clone())
            .collect();
        working.splice(replace_start..replace_end, replacement);

        if working.len() > 128 {
            return Err(ApplyError::TooLarge);
        }

        let previous_net = net;
        net += hunk.new_count as i64 - hunk.old_count as i64;
        drift = s as i64 - (hunk.old_start + previous_net);
    }

    let mut output = String::new();
    for line in working {
        output.push_str(&line);
        output.push('\n');
    }
    Ok(output)
}