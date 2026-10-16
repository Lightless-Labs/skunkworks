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
enum Row {
    Context(String),
    Remove(String),
    Add(String),
}

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
    let n = s.parse::<usize>().ok()?;
    (n <= 128).then_some(n)
}

fn parse_hunk_header(line: &str) -> Option<(usize, usize, usize, usize)> {
    let inner = line.strip_prefix("@@ -")?.strip_suffix(" @@")?;
    let (old, new) = inner.split_once(" +")?;
    if new.contains(" +") || old.contains(',') && old.matches(',').count() != 1 {
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
    if b > 0 && a == 0 || d > 0 && c == 0 {
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
        if lines.get(i) != Some(expected) {
            return Err(ApplyError::BadHeader { line: i + 1 });
        }
    }
    if lines.len() < 3 {
        return Err(ApplyError::BadHunkHeader { line: 3 });
    }

    let mut hunks = Vec::new();
    let mut previous_old_end = 0usize;
    let mut net: i128 = 0;
    let mut i = 2usize;

    while i < lines.len() {
        let header_line = i + 1;
        let (a, b, c, d) = parse_hunk_header(lines[i])
            .ok_or(ApplyError::BadHunkHeader { line: header_line })?;
        let old_start = full_start(a, b);
        let new_start = full_start(c, d);

        i += 1;
        let body_begin = i;
        let mut rows = Vec::new();
        let (mut old_seen, mut new_seen) = (0usize, 0usize);
        while i < lines.len() && !lines[i].starts_with("@@") {
            let line = lines[i];
            let payload = line.get(1..).filter(|_| !line.is_empty());
            let Some(payload) = payload else {
                return Err(ApplyError::BadBody { line: i + 1 });
            };
            if payload.contains('\r') {
                return Err(ApplyError::BadBody { line: i + 1 });
            }
            match line.as_bytes()[0] {
                b' ' => {
                    old_seen += 1;
                    new_seen += 1;
                    rows.push(Row::Context(payload.to_owned()));
                }
                b'-' => {
                    old_seen += 1;
                    rows.push(Row::Remove(payload.to_owned()));
                }
                b'+' => {
                    new_seen += 1;
                    rows.push(Row::Add(payload.to_owned()));
                }
                _ => return Err(ApplyError::BadBody { line: i + 1 }),
            }
            i += 1;
        }

        if old_seen != b || new_seen != d {
            return Err(ApplyError::CountMismatch { line: header_line });
        }
        if !rows.iter().any(|r| matches!(r, Row::Add(_) | Row::Remove(_))) {
            return Err(ApplyError::BadBody { line: header_line });
        }
        if old_start < previous_old_end {
            return Err(ApplyError::Misordered { line: header_line });
        }
        let required_new = old_start as i128 + net;
        if required_new < 0 || new_start as i128 != required_new {
            return Err(ApplyError::Misordered { line: header_line });
        }

        previous_old_end = old_start.saturating_add(b);
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

        // The body parser always advances from the header, including for an empty body.
        let _ = body_begin;
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
        while lead < hunk.rows.len() && matches!(hunk.rows[lead], Row::Context(_)) {
            lead += 1;
        }
        let mut trail = 0usize;
        while trail < hunk.rows.len()
            && matches!(hunk.rows[hunk.rows.len() - 1 - trail], Row::Context(_))
        {
            trail += 1;
        }

        let expected = hunk.old_start as i128 + net + drift;
        let max_l = options.fuzz.min(lead);
        let max_r = options.fuzz.min(trail);
        let mut best_rank: Option<(usize, i128)> = None;
        let mut best_starts = Vec::<(usize, usize, usize)>::new();

        for l in 0..=max_l {
            for r in 0..=max_r {
                if l + r > hunk.old_count {
                    continue;
                }
                let mut old_payloads = Vec::new();
                for row in &hunk.rows[l..hunk.rows.len() - r] {
                    match row {
                        Row::Context(s) | Row::Remove(s) => old_payloads.push(s.as_str()),
                        Row::Add(_) => {}
                    }
                }
                let kept_old = hunk.old_count - l - r;
                for s in 0..=working.len().saturating_sub(hunk.old_count) {
                    if s + hunk.old_count > working.len() {
                        continue;
                    }
                    let distance = (s as i128 - expected).abs();
                    if distance > options.max_offset as i128 {
                        continue;
                    }
                    if old_payloads.len() != kept_old
                        || old_payloads
                            .iter()
                            .enumerate()
                            .any(|(j, p)| working[s + l + j] != *p)
                    {
                        continue;
                    }
                    let rank = (l + r, distance);
                    match best_rank {
                        None => {
                            best_rank = Some(rank);
                            best_starts.clear();
                            best_starts.push((s, l, r));
                        }
                        Some(old) if rank < old => {
                            best_rank = Some(rank);
                            best_starts.clear();
                            best_starts.push((s, l, r));
                        }
                        Some(old) if rank == old => best_starts.push((s, l, r)),
                        _ => {}
                    }
                }
            }
        }

        if best_starts.is_empty() {
            return Err(ApplyError::NoMatch { hunk: index + 1 });
        }
        best_starts.sort_by_key(|&(s, l, _)| (s, l));
        let mut distinct = best_starts.iter().map(|x| x.0).collect::<Vec<_>>();
        distinct.sort_unstable();
        distinct.dedup();
        if distinct.len() > 1 {
            return Err(ApplyError::Ambiguous {
                hunk: index + 1,
                first: distinct[0],
                second: distinct[1],
            });
        }

        let (s, l, r) = best_starts[0];
        let mut replacement = Vec::new();
        for row in &hunk.rows[l..hunk.rows.len() - r] {
            match row {
                Row::Context(v) | Row::Add(v) => replacement.push(v.clone()),
                Row::Remove(_) => {}
            }
        }
        let range_start = s + l;
        let range_end = s + hunk.old_count - r;
        working.splice(range_start..range_end, replacement);
        if working.len() > 128 {
            return Err(ApplyError::TooLarge);
        }

        drift = s as i128 - (hunk.old_start as i128 + net);
        net += hunk.new_count as i128 - hunk.old_count as i128;
    }

    let mut output = String::new();
    for line in working {
        output.push_str(&line);
        output.push('\n');
    }
    Ok(output)
}