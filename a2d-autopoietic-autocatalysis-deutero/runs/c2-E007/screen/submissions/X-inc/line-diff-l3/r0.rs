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

#[derive(Debug, Clone, PartialEq, Eq)]
enum RowKind {
    Context,
    Remove,
    Add,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Row {
    kind: RowKind,
    payload: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

fn parse_hunk_header(line: &str, line_no: usize) -> Result<(usize, usize, usize, usize), ApplyError> {
    let bad = || ApplyError::BadHunkHeader { line: line_no };
    let inner = line.strip_prefix("@@ -").ok_or_else(bad)?;
    let (old, rest) = inner.split_once(" +").ok_or_else(bad)?;
    let (new, suffix) = rest.split_once(" @@").ok_or_else(bad)?;
    if suffix != "" || old.matches(',').count() != 1 || new.matches(',').count() != 1 {
        return Err(bad());
    }
    let (a, b) = old.split_once(',').ok_or_else(bad)?;
    let (c, d) = new.split_once(',').ok_or_else(bad)?;
    let a = parse_number(a).ok_or_else(bad)?;
    let b = parse_number(b).ok_or_else(bad)?;
    let c = parse_number(c).ok_or_else(bad)?;
    let d = parse_number(d).ok_or_else(bad)?;
    if (b > 0 && a == 0) || (d > 0 && c == 0) {
        return Err(bad());
    }
    let old_start = if b > 0 { a - 1 } else { a };
    let new_start = if d > 0 { c - 1 } else { c };
    Ok((old_start, b, new_start, d))
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
    if lines.first().copied() != Some("--- before") {
        return Err(ApplyError::BadHeader { line: 1 });
    }
    if lines.get(1).copied() != Some("+++ after") {
        return Err(ApplyError::BadHeader { line: 2 });
    }
    if lines.len() < 3 {
        return Err(ApplyError::BadHunkHeader { line: 3 });
    }

    let mut hunks = Vec::new();
    let mut i = 2;
    let mut previous_end = 0usize;
    let mut net: i128 = 0;

    while i < lines.len() {
        let header_line = i + 1;
        if !lines[i].starts_with("@@") {
            return Err(ApplyError::BadHunkHeader { line: header_line });
        }

        let (old_start, old_count, new_start, new_count) =
            parse_hunk_header(lines[i], header_line)?;
        i += 1;

        let mut rows = Vec::new();
        let mut old_seen = 0usize;
        let mut new_seen = 0usize;

        while i < lines.len() && !lines[i].starts_with("@@") {
            let row_line = i + 1;
            let line = lines[i];
            let (kind, payload) = match line.as_bytes().first().copied() {
                Some(b' ') => (RowKind::Context, &line[1..]),
                Some(b'-') => (RowKind::Remove, &line[1..]),
                Some(b'+') => (RowKind::Add, &line[1..]),
                _ => return Err(ApplyError::BadBody { line: row_line }),
            };
            if payload.contains('\r') {
                return Err(ApplyError::BadBody { line: row_line });
            }
            match kind {
                RowKind::Context => {
                    old_seen += 1;
                    new_seen += 1;
                }
                RowKind::Remove => old_seen += 1,
                RowKind::Add => new_seen += 1,
            }
            rows.push(Row {
                kind,
                payload: payload.to_string(),
            });
            i += 1;
        }

        if old_seen != old_count || new_seen != new_count {
            return Err(ApplyError::CountMismatch { line: header_line });
        }
        if !rows.iter().any(|r| r.kind != RowKind::Context) {
            return Err(ApplyError::BadBody { line: header_line });
        }

        if old_start < previous_end {
            return Err(ApplyError::Misordered { line: header_line });
        }
        let required_new = old_start as i128 + net;
        if required_new < 0 || required_new != new_start as i128 {
            return Err(ApplyError::Misordered { line: header_line });
        }

        if hunks.len() == 16 {
            return Err(ApplyError::TooLarge);
        }

        previous_end = old_start + old_count;
        net += new_count as i128 - old_count as i128;
        hunks.push(Hunk {
            line: header_line,
            old_start,
            old_count,
            new_start,
            new_count,
            rows,
        });
    }

    if hunks.is_empty() {
        return Err(ApplyError::BadHunkHeader { line: 3 });
    }
    Ok(hunks)
}

fn split_source(source: &str) -> Result<Vec<String>, ApplyError> {
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
    let mut working = split_source(source)?;
    let mut net: i128 = 0;
    let mut drift: i128 = 0;

    for (idx, hunk) in hunks.iter().enumerate() {
        let mut lead = 0usize;
        while lead < hunk.rows.len() && hunk.rows[lead].kind == RowKind::Context {
            lead += 1;
        }
        let mut trail = 0usize;
        while trail < hunk.rows.len() && hunk.rows[hunk.rows.len() - 1 - trail].kind == RowKind::Context {
            trail += 1;
        }

        let expected = hunk.old_start as i128 + net + drift;
        let mut best_rank: Option<(usize, i128)> = None;
        let mut starts = Vec::<(usize, usize, usize)>::new();

        for l in 0..=options.fuzz.min(lead) {
            for r in 0..=options.fuzz.min(trail) {
                if l + r > hunk.old_count {
                    continue;
                }
                let mut old_payloads = Vec::new();
                for (row_idx, row) in hunk.rows.iter().enumerate() {
                    if (row_idx < l) || (row_idx >= hunk.rows.len().saturating_sub(r) && r > 0) {
                        continue;
                    }
                    if row.kind != RowKind::Add {
                        old_payloads.push(row.payload.as_str());
                    }
                }

                for s in 0..=working.len() {
                    if s + hunk.old_count > working.len() {
                        continue;
                    }
                    let distance = abs_distance(s as i128, expected);
                    if distance > options.max_offset as i128 {
                        continue;
                    }
                    let from = s + l;
                    let to = s + hunk.old_count - r;
                    if from > to || old_payloads.len() != to - from {
                        continue;
                    }
                    if old_payloads
                        .iter()
                        .zip(&working[from..to])
                        .all(|(want, got)| *want == got)
                    {
                        let rank = (l + r, distance);
                        match best_rank {
                            None => {
                                best_rank = Some(rank);
                                starts.clear();
                                starts.push((s, l, r));
                            }
                            Some(current) if rank < current => {
                                best_rank = Some(rank);
                                starts.clear();
                                starts.push((s, l, r));
                            }
                            Some(current) if rank == current => starts.push((s, l, r)),
                            _ => {}
                        }
                    }
                }
            }
        }

        if starts.is_empty() {
            return Err(ApplyError::NoMatch { hunk: idx + 1 });
        }

        let mut distinct = starts.iter().map(|x| x.0).collect::<Vec<_>>();
        distinct.sort_unstable();
        distinct.dedup();
        if distinct.len() > 1 {
            return Err(ApplyError::Ambiguous {
                hunk: idx + 1,
                first: distinct[0],
                second: distinct[1],
            });
        }

        let start = distinct[0];
        let (_, l, r) = starts
            .into_iter()
            .filter(|x| x.0 == start)
            .min_by_key(|x| x.1)
            .unwrap();

        let mut replacement = Vec::new();
        for (row_idx, row) in hunk.rows.iter().enumerate() {
            if row_idx < l || (r > 0 && row_idx >= hunk.rows.len() - r) {
                continue;
            }
            if row.kind != RowKind::Remove {
                replacement.push(row.payload.clone());
            }
        }

        let from = start + l;
        let to = start + hunk.old_count - r;
        working.splice(from..to, replacement);

        if working.len() > 128 {
            return Err(ApplyError::TooLarge);
        }

        drift = start as i128 - (hunk.old_start as i128 + net);
        net += hunk.new_count as i128 - hunk.old_count as i128;
    }

    let mut output = String::new();
    for line in working {
        output.push_str(&line);
        output.push('\n');
    }
    Ok(output)
}