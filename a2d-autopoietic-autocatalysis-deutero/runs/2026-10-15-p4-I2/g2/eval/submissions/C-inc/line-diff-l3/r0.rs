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
    kind: u8,
    payload: String,
    line: usize,
}

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
    let inner = line.strip_prefix("@@ -").ok_or_else(bad)?;
    let (old, rest) = inner.split_once(" +").ok_or_else(bad)?;
    let (new, suffix) = rest.split_once(" @@").ok_or_else(bad)?;
    if suffix != "" {
        return Err(bad());
    }
    let (a, b) = old.split_once(',').ok_or_else(bad)?;
    let (c, d) = new.split_once(',').ok_or_else(bad)?;
    let a = parse_decimal(a).ok_or_else(bad)?;
    let b = parse_decimal(b).ok_or_else(bad)?;
    let c = parse_decimal(c).ok_or_else(bad)?;
    let d = parse_decimal(d).ok_or_else(bad)?;
    if (b > 0 && a == 0) || (d > 0 && c == 0) {
        return Err(bad());
    }
    Ok((a, b, c, d))
}

fn full_start(start: usize, count: usize) -> usize {
    if count > 0 { start - 1 } else { start }
}

fn patch_lines(patch: &str) -> Vec<&str> {
    patch.split_terminator('\n').collect()
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
    if lines.get(0).copied() != Some("--- before") {
        return Err(ApplyError::BadHeader { line: 1 });
    }
    if lines.get(1).copied() != Some("+++ after") {
        return Err(ApplyError::BadHeader { line: 2 });
    }
    if lines.len() < 3 || !lines[2].starts_with("@@") {
        return Err(ApplyError::BadHunkHeader { line: 3 });
    }

    let mut hunks = Vec::new();
    let mut i = 2usize;
    let mut previous_old_end = 0usize;
    let mut cumulative_delta: i128 = 0;

    while i < lines.len() {
        if !lines[i].starts_with("@@") {
            return Err(ApplyError::BadHunkHeader { line: i + 1 });
        }
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
            if bytes.is_empty() || !matches!(bytes[0], b' ' | b'-' | b'+') || row.contains('\r') {
                return Err(ApplyError::BadBody { line: row_line });
            }
            let kind = bytes[0];
            let payload = &row[1..];
            match kind {
                b' ' => {
                    old_seen += 1;
                    new_seen += 1;
                }
                b'-' => old_seen += 1,
                b'+' => new_seen += 1,
                _ => unreachable!(),
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
            rows,
            header_line,
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

    for (index, hunk) in hunks.iter().enumerate() {
        let lead = hunk.rows.iter().take_while(|r| r.kind == b' ').count();
        let trail = hunk.rows.iter().rev().take_while(|r| r.kind == b' ').count();
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
                let mut old_payloads = Vec::new();
                let mut new_payloads = Vec::new();
                let mut context_pos = 0usize;
                for row in &hunk.rows {
                    match row.kind {
                        b' ' => {
                            let dropped = (context_pos < l)
                                || (context_pos >= hunk.old_count - r);
                            context_pos += 1;
                            if !dropped {
                                old_payloads.push(row.payload.as_str());
                                new_payloads.push(row.payload.as_str());
                            }
                        }
                        b'-' => old_payloads.push(row.payload.as_str()),
                        b'+' => new_payloads.push(row.payload.as_str()),
                        _ => unreachable!(),
                    }
                }

                for s in 0..=working.len() {
                    if s + hunk.old_count > working.len() {
                        continue;
                    }
                    if (s as i128 - expected).abs() > options.max_offset as i128 {
                        continue;
                    }
                    if s + l > s + hunk.old_count - r {
                        continue;
                    }
                    let matched = old_payloads.len() == hunk.old_count - l - r
                        && old_payloads
                            .iter()
                            .enumerate()
                            .all(|(j, p)| working[s + l + j] == *p);
                    if !matched {
                        continue;
                    }

                    let rank = (l + r, abs_distance(s as i128, expected));
                    match best_rank {
                        None => {
                            best_rank = Some(rank);
                            best_by_start.clear();
                            best_by_start.push((s, l, r));
                        }
                        Some(old_rank) if rank < old_rank => {
                            best_rank = Some(rank);
                            best_by_start.clear();
                            best_by_start.push((s, l, r));
                        }
                        Some(old_rank) if rank == old_rank => {
                            if let Some(existing) = best_by_start.iter_mut().find(|x| x.0 == s) {
                                if l < existing.1 {
                                    *existing = (s, l, r);
                                }
                            } else {
                                best_by_start.push((s, l, r));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        if best_by_start.is_empty() {
            return Err(ApplyError::NoMatch { hunk: index + 1 });
        }
        best_by_start.sort_by_key(|x| x.0);
        if best_by_start.len() > 1 {
            return Err(ApplyError::Ambiguous {
                hunk: index + 1,
                first: best_by_start[0].0,
                second: best_by_start[1].0,
            });
        }

        let (s, l, r) = best_by_start[0];
        let mut replacement = Vec::new();
        let mut context_pos = 0usize;
        for row in &hunk.rows {
            match row.kind {
                b' ' => {
                    let dropped = (context_pos < l)
                        || (context_pos >= hunk.old_count - r);
                    context_pos += 1;
                    if !dropped {
                        replacement.push(row.payload.clone());
                    }
                }
                b'+' => replacement.push(row.payload.clone()),
                b'-' => {}
                _ => unreachable!(),
            }
        }

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

    let mut output = String::new();
    for line in working {
        output.push_str(&line);
        output.push('\n');
    }
    Ok(output)
}

fn main() {}