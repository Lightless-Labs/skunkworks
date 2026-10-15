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

struct Hunk {
    line: usize,
    old_start: usize,
    old_count: usize,
    new_start: usize,
    new_count: usize,
    rows: Vec<Row>,
}

fn physical_lines(s: &str) -> Vec<&str> {
    s.split_terminator('\n').collect()
}

fn parse_number(s: &str) -> Option<usize> {
    if s.is_empty() || (s.len() > 1 && s.starts_with('0')) {
        return None;
    }
    let mut value = 0usize;
    for b in s.bytes() {
        if !b.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add((b - b'0') as usize)?;
        if value > 128 {
            return None;
        }
    }
    Some(value)
}

fn parse_hunk_header(line: &str) -> Option<(usize, usize, usize, usize)> {
    let rest = line.strip_prefix("@@ -")?;
    let (old, rest) = rest.split_once(" +")?;
    let (new, suffix) = rest.split_once(" @@")?;
    if !suffix.is_empty() {
        return None;
    }
    let (a, b) = old.split_once(',')?;
    let (c, d) = new.split_once(',')?;
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

fn nominal_start(start: usize, count: usize) -> usize {
    if count > 0 {
        start - 1
    } else {
        start
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

    let lines = physical_lines(patch);
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
    let mut previous_old_end: Option<usize> = None;
    let mut cumulative_delta: i128 = 0;

    while i < lines.len() {
        let header_line = i + 1;
        let Some((a, b, c, d)) = parse_hunk_header(lines[i]) else {
            return Err(ApplyError::BadHunkHeader { line: header_line });
        };
        let old_start = nominal_start(a, b);
        let new_start = nominal_start(c, d);

        i += 1;
        let mut rows = Vec::new();
        let mut old_seen = 0usize;
        let mut new_seen = 0usize;

        while i < lines.len() && !lines[i].starts_with("@@") {
            let row_line = i + 1;
            let row = lines[i];
            let Some((&prefix, payload)) = row.as_bytes().split_first() else {
                return Err(ApplyError::BadBody { line: row_line });
            };
            if !matches!(prefix, b' ' | b'-' | b'+') || row.as_bytes().contains(&b'\r') {
                return Err(ApplyError::BadBody { line: row_line });
            }
            let payload = &row[1..];
            match prefix {
                b' ' => {
                    old_seen += 1;
                    new_seen += 1;
                }
                b'-' => old_seen += 1,
                b'+' => new_seen += 1,
                _ => unreachable!(),
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

        let old_end = old_start.checked_add(b).ok_or(ApplyError::TooLarge)?;
        if previous_old_end.is_some_and(|end| old_start < end)
            || (new_start as i128) != (old_start as i128 + cumulative_delta)
        {
            return Err(ApplyError::Misordered { line: header_line });
        }

        if hunks.len() == 16 {
            return Err(ApplyError::TooLarge);
        }

        previous_old_end = Some(old_end);
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
    Ok(hunks)
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
            .map(str::to_owned)
            .collect()
    };
    if working.len() > 128 {
        return Err(ApplyError::TooLarge);
    }
    if hunks.is_empty() {
        return Ok(source.to_owned());
    }

    let mut net: i128 = 0;
    let mut drift: i128 = 0;

    for (index, hunk) in hunks.iter().enumerate() {
        let lead = hunk.rows.iter().take_while(|r| r.prefix == b' ').count();
        let trail = hunk.rows.iter().rev().take_while(|r| r.prefix == b' ').count();
        let max_l = options.fuzz.min(lead);
        let max_r = options.fuzz.min(trail);
        let expected = hunk.old_start as i128 + net + drift;

        let mut best_rank: Option<(usize, i128)> = None;
        let mut best_by_start: Vec<(usize, usize, usize)> = Vec::new();

        for l in 0..=max_l {
            for r in 0..=max_r {
                if l + r > hunk.old_count {
                    continue;
                }

                let mut match_payloads = Vec::new();
                for row in hunk.rows.iter().skip(l).take(hunk.rows.len() - l - r) {
                    if row.prefix == b' ' || row.prefix == b'-' {
                        match_payloads.push(row.payload.as_str());
                    }
                }
                let trimmed_old = hunk.old_count - l - r;

                for s in 0..=working.len().saturating_sub(hunk.old_count) {
                    if s + hunk.old_count > working.len() {
                        continue;
                    }
                    let distance = (s as i128 - expected).abs();
                    if distance > options.max_offset as i128 {
                        continue;
                    }
                    if match_payloads.len() != trimmed_old
                        || !match_payloads
                            .iter()
                            .enumerate()
                            .all(|(j, p)| working[s + l + j] == *p)
                    {
                        continue;
                    }

                    let rank = (l + r, distance);
                    match best_rank {
                        None => {
                            best_rank = Some(rank);
                            best_by_start.clear();
                        }
                        Some(old_rank) if rank < old_rank => {
                            best_rank = Some(rank);
                            best_by_start.clear();
                        }
                        Some(old_rank) if rank > old_rank => continue,
                        _ => {}
                    }
                    best_by_start.push((s, l, r));
                }
            }
        }

        if best_by_start.is_empty() {
            return Err(ApplyError::NoMatch { hunk: index + 1 });
        }

        best_by_start.sort_by_key(|&(s, l, _)| (s, l));
        let mut distinct_starts = Vec::new();
        for &(s, _, _) in &best_by_start {
            if distinct_starts.last().copied() != Some(s) {
                distinct_starts.push(s);
            }
        }
        if distinct_starts.len() > 1 {
            return Err(ApplyError::Ambiguous {
                hunk: index + 1,
                first: distinct_starts[0],
                second: distinct_starts[1],
            });
        }

        let (s, l, r) = best_by_start[0];
        let mut replacement = Vec::new();
        for row in hunk.rows.iter().skip(l).take(hunk.rows.len() - l - r) {
            if row.prefix == b' ' || row.prefix == b'+' {
                replacement.push(row.payload.clone());
            }
        }
        let begin = s + l;
        let end = s + hunk.old_count - r;
        working.splice(begin..end, replacement);

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

fn main() {}