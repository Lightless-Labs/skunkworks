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
    prefix: u8,
    payload: String,
}

#[derive(Debug, Clone)]
struct Hunk {
    header_line: usize,
    old_start: usize,
    old_count: usize,
    new_start: usize,
    new_count: usize,
    rows: Vec<Row>,
}

fn physical_lines(text: &str) -> Vec<&str> {
    text.split_terminator('\n').collect()
}

fn parse_number(text: &str) -> Option<usize> {
    if text.is_empty()
        || (text.len() > 1 && text.starts_with('0'))
        || !text.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let value = text.parse::<usize>().ok()?;
    (value <= 128).then_some(value)
}

fn parse_hunk_header(line: &str, line_no: usize) -> Result<(usize, usize, usize, usize), ApplyError> {
    let bad = || ApplyError::BadHunkHeader { line: line_no };
    if !line.starts_with("@@ -") || !line.ends_with(" @@") {
        return Err(bad());
    }
    let inner = &line[4..line.len() - 3];
    let (old, new) = inner.split_once(" +").ok_or_else(bad)?;
    if new.contains(' ') {
        return Err(bad());
    }
    let (a, b) = old.split_once(',').ok_or_else(bad)?;
    let (c, d) = new.split_once(',').ok_or_else(bad)?;
    let (a, b, c, d) = (
        parse_number(a).ok_or_else(bad)?,
        parse_number(b).ok_or_else(bad)?,
        parse_number(c).ok_or_else(bad)?,
        parse_number(d).ok_or_else(bad)?,
    );
    if (b > 0 && a == 0) || (d > 0 && c == 0) {
        return Err(bad());
    }
    Ok((a, b, c, d))
}

fn nominal_start(start: usize, count: usize) -> usize {
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
    let lines = physical_lines(patch);
    for (i, expected) in ["--- before", "+++ after"].iter().enumerate() {
        if lines.get(i).copied() != Some(*expected) {
            return Err(ApplyError::BadHeader { line: i + 1 });
        }
    }
    if lines.len() < 3 || !lines[2].starts_with("@@") {
        return Err(ApplyError::BadHunkHeader { line: 3 });
    }

    let mut hunks = Vec::new();
    let mut i = 2;
    let mut previous_old_end = 0usize;
    let mut net: i128 = 0;

    while i < lines.len() {
        if !lines[i].starts_with("@@") {
            return Err(ApplyError::BadHunkHeader { line: i + 1 });
        }
        let header_line = i + 1;
        let (a, b, c, d) = parse_hunk_header(lines[i], header_line)?;
        let old_start = nominal_start(a, b);
        let new_start = nominal_start(c, d);
        i += 1;

        let mut rows = Vec::new();
        let mut old_seen = 0usize;
        let mut new_seen = 0usize;
        while i < lines.len() && !lines[i].starts_with("@@") {
            let bytes = lines[i].as_bytes();
            if bytes.is_empty() || !matches!(bytes[0], b' ' | b'-' | b'+') {
                return Err(ApplyError::BadBody { line: i + 1 });
            }
            let payload = &lines[i][1..];
            if payload.contains('\r') {
                return Err(ApplyError::BadBody { line: i + 1 });
            }
            let prefix = bytes[0];
            if prefix == b' ' || prefix == b'-' {
                old_seen += 1;
            }
            if prefix == b' ' || prefix == b'+' {
                new_seen += 1;
            }
            rows.push(Row { prefix, payload: payload.to_owned() });
            i += 1;
        }
        if old_seen != b || new_seen != d {
            return Err(ApplyError::CountMismatch { line: header_line });
        }
        if !rows.iter().any(|r| r.prefix == b'+' || r.prefix == b'-') {
            return Err(ApplyError::BadBody { line: header_line });
        }

        if old_start < previous_old_end
            || (new_start as i128) != (old_start as i128 + net)
        {
            return Err(ApplyError::Misordered { line: header_line });
        }
        previous_old_end = old_start.saturating_add(b);
        net += d as i128 - b as i128;

        hunks.push(Hunk {
            header_line,
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
    Ok(hunks)
}

fn apply_one(
    source: &mut Vec<String>,
    hunk: &Hunk,
    hunk_no: usize,
    options: &PatchOptions,
    expected: i128,
) -> Result<(), ApplyError> {
    let lead = hunk.rows.iter().take_while(|r| r.prefix == b' ').count();
    let trail = hunk.rows.iter().rev().take_while(|r| r.prefix == b' ').count();
    let max_l = options.fuzz.min(lead);
    let max_r = options.fuzz.min(trail);

    let mut best_rank: Option<(usize, i128)> = None;
    let mut best_starts = Vec::<(usize, usize, usize)>::new();

    for l in 0..=max_l {
        for r in 0..=max_r {
            if l + r > hunk.old_count {
                continue;
            }
            let mut expected_payloads = Vec::new();
            let mut space_index = 0usize;
            for row in &hunk.rows {
                if row.prefix == b' ' {
                    let is_dropped = space_index < l
                        || space_index >= lead + (lead.saturating_sub(l) + (hunk.old_count.saturating_sub(lead + trail)));
                    // Edge-context eligibility is determined by row position among context rows.
                    let trailing_start = hunk.old_count - r;
                    let is_dropped = (space_index < l) || (space_index >= trailing_start && r > 0);
                    if !is_dropped {
                        expected_payloads.push(row.payload.as_str());
                    }
                    space_index += 1;
                } else if row.prefix == b'-' {
                    expected_payloads.push(row.payload.as_str());
                    space_index += 1;
                }
            }

            for s in 0..=source.len().saturating_sub(hunk.old_count) {
                if s + hunk.old_count > source.len() {
                    continue;
                }
                let distance = (s as i128 - expected).abs();
                if distance > options.max_offset as i128 {
                    continue;
                }
                if s + l > s + hunk.old_count - r {
                    continue;
                }
                let actual = &source[s + l..s + hunk.old_count - r];
                if actual.iter().map(String::as_str).eq(expected_payloads.iter().copied()) {
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
    }

    if best_starts.is_empty() {
        return Err(ApplyError::NoMatch { hunk: hunk_no });
    }
    best_starts.sort_by_key(|&(s, l, r)| (s, l, r));
    let mut distinct = best_starts.iter().map(|x| x.0).collect::<Vec<_>>();
    distinct.sort_unstable();
    distinct.dedup();
    if distinct.len() > 1 {
        return Err(ApplyError::Ambiguous {
            hunk: hunk_no,
            first: distinct[0],
            second: distinct[1],
        });
    }
    let (s, l, r) = best_starts[0];

    let mut replacement = Vec::new();
    let mut space_index = 0usize;
    for row in &hunk.rows {
        match row.prefix {
            b' ' => {
                let dropped = space_index < l || (space_index >= hunk.old_count - r && r > 0);
                if !dropped {
                    replacement.push(row.payload.clone());
                }
                space_index += 1;
            }
            b'-' => {
                space_index += 1;
            }
            b'+' => replacement.push(row.payload.clone()),
            _ => unreachable!(),
        }
    }
    source.splice(s + l..s + hunk.old_count - r, replacement);
    if source.len() > 128 {
        return Err(ApplyError::TooLarge);
    }
    Ok(())
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
    let mut lines: Vec<String> = if source.is_empty() {
        Vec::new()
    } else {
        source.strip_suffix('\n').unwrap().split('\n').map(str::to_owned).collect()
    };
    if lines.len() > 128 {
        return Err(ApplyError::TooLarge);
    }

    let mut net: i128 = 0;
    let mut drift: i128 = 0;
    for (idx, hunk) in hunks.iter().enumerate() {
        let expected = hunk.old_start as i128 + net + drift;
        apply_one(&mut lines, hunk, idx + 1, &options, expected)?;
        let previous_net = net;
        net += hunk.new_count as i128 - hunk.old_count as i128;
        drift = (hunk.old_start as i128 + drift + (expected - (hunk.old_start as i128 + previous_net)));
        // Record the accepted displacement from the nominal position.
        // The chosen start is recovered from the applied replacement's location below.
        let _ = drift;
    }

    let mut output = String::new();
    for line in lines {
        output.push_str(&line);
        output.push('\n');
    }
    Ok(output)
}