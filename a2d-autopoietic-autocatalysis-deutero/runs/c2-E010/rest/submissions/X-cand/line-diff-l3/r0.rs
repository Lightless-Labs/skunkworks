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
struct Hunk {
    line: usize,
    old_start: usize,
    old_count: usize,
    new_start: usize,
    new_count: usize,
    rows: Vec<(u8, String)>,
}

fn parse_decimal(s: &str) -> Option<usize> {
    if s.is_empty()
        || !s.bytes().all(|b| b.is_ascii_digit())
        || (s.len() > 1 && s.as_bytes()[0] == b'0')
    {
        return None;
    }
    let mut n = 0usize;
    for b in s.bytes() {
        n = n.checked_mul(10)?.checked_add((b - b'0') as usize)?;
        if n > 128 {
            return None;
        }
    }
    Some(n)
}

fn parse_hunk_header(line: &str) -> Option<(usize, usize, usize, usize)> {
    let rest = line.strip_prefix("@@ -")?;
    let (old, rest) = rest.split_once(" +")?;
    let (new, ending) = rest.split_once(" @@")?;
    if ending != "" || line != format!("@@ -{} +{} @@", old, new) {
        return None;
    }
    let (a, b) = old.split_once(',')?;
    let (c, d) = new.split_once(',')?;
    if a.contains(',') || b.contains(',') || c.contains(',') || d.contains(',') {
        return None;
    }
    Some((parse_decimal(a)?, parse_decimal(b)?, parse_decimal(c)?, parse_decimal(d)?))
}

fn nominal(start: usize, count: usize) -> Option<i64> {
    if count > 0 {
        if start == 0 {
            return None;
        }
        Some((start - 1) as i64)
    } else {
        Some(start as i64)
    }
}

pub fn apply_unified(
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
                line: 1 + patch.bytes().filter(|&b| b == b'\n').count(),
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

        let mut i = 2;
        let mut previous_old_end = 0i64;
        let mut cumulative_delta = 0i64;
        while i < lines.len() {
            let header_line = i + 1;
            let Some((a, b, c, d)) = parse_hunk_header(lines[i]) else {
                return Err(ApplyError::BadHunkHeader { line: header_line });
            };
            let Some(old_start) = nominal(a, b) else {
                return Err(ApplyError::BadHunkHeader { line: header_line });
            };
            let Some(new_start) = nominal(c, d) else {
                return Err(ApplyError::BadHunkHeader { line: header_line });
            };

            i += 1;
            let mut rows = Vec::new();
            let (mut old_seen, mut new_seen) = (0usize, 0usize);
            while i < lines.len() && !lines[i].starts_with("@@") {
                let row = lines[i];
                let Some((&prefix, payload)) = row.as_bytes().split_first() else {
                    return Err(ApplyError::BadBody { line: i + 1 });
                };
                if !matches!(prefix, b' ' | b'-' | b'+') || payload.contains(&b'\r') {
                    return Err(ApplyError::BadBody { line: i + 1 });
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
                rows.push((prefix, payload.to_owned()));
                i += 1;
            }

            if old_seen != b || new_seen != d {
                return Err(ApplyError::CountMismatch { line: header_line });
            }
            if !rows.iter().any(|(p, _)| *p == b'+' || *p == b'-') {
                return Err(ApplyError::BadBody { line: header_line });
            }

            if old_start < previous_old_end
                || new_start != old_start + cumulative_delta
            {
                return Err(ApplyError::Misordered { line: header_line });
            }
            previous_old_end = old_start + b as i64;
            cumulative_delta += d as i64 - b as i64;

            hunks.push(Hunk {
                line: header_line,
                old_start: old_start as usize,
                old_count: b,
                new_start: new_start as usize,
                new_count: d,
                rows,
            });

            if hunks.len() > 16 {
                return Err(ApplyError::TooLarge);
            }
        }
    }

    if let Some(byte) = source.as_bytes().iter().position(|&b| b == b'\r') {
        return Err(ApplyError::SourceFormat { byte });
    }
    if !source.is_empty() && !source.ends_with('\n') {
        return Err(ApplyError::SourceMissingLf);
    }
    let mut work: Vec<String> = if source.is_empty() {
        Vec::new()
    } else {
        source[..source.len() - 1].split('\n').map(str::to_owned).collect()
    };
    if work.len() > 128 {
        return Err(ApplyError::TooLarge);
    }

    let (mut net, mut drift) = (0i64, 0i64);
    for (idx, h) in hunks.iter().enumerate() {
        let lead = h.rows.iter().take_while(|(p, _)| *p == b' ').count();
        let trail = h.rows.iter().rev().take_while(|(p, _)| *p == b' ').count();
        let expected = h.old_start as i64 + net + drift;

        let mut best_rank: Option<(usize, i64)> = None;
        let mut best: Vec<(usize, usize, usize)> = Vec::new();

        for l in 0..=options.fuzz.min(lead) {
            for r in 0..=options.fuzz.min(trail) {
                if l + r > h.old_count {
                    continue;
                }
                let mut old_payloads = Vec::new();
                for (ri, (p, text)) in h.rows.iter().enumerate() {
                    if (ri < l) || (ri >= h.rows.len().saturating_sub(r) && r > 0) {
                        continue;
                    }
                    if *p == b' ' || *p == b'-' {
                        old_payloads.push(text.as_str());
                    }
                }
                let low = l;
                let high = h.old_count - r;
                if old_payloads.len() != high - low {
                    continue;
                }

                for s in 0..=work.len().saturating_sub(h.old_count) {
                    if s + h.old_count > work.len() {
                        continue;
                    }
                    let dist = (s as i64 - expected).abs();
                    if dist > options.max_offset as i64 {
                        continue;
                    }
                    if work[s + low..s + high]
                        .iter()
                        .map(String::as_str)
                        .ne(old_payloads.iter().copied())
                    {
                        continue;
                    }
                    let rank = (l + r, dist);
                    match best_rank {
                        None => {
                            best_rank = Some(rank);
                            best.clear();
                            best.push((s, l, r));
                        }
                        Some(prev) if rank < prev => {
                            best_rank = Some(rank);
                            best.clear();
                            best.push((s, l, r));
                        }
                        Some(prev) if rank == prev => best.push((s, l, r)),
                        _ => {}
                    }
                }
            }
        }

        if best.is_empty() {
            return Err(ApplyError::NoMatch { hunk: idx + 1 });
        }

        let mut starts: Vec<usize> = best.iter().map(|x| x.0).collect();
        starts.sort_unstable();
        starts.dedup();
        if starts.len() > 1 {
            return Err(ApplyError::Ambiguous {
                hunk: idx + 1,
                first: starts[0],
                second: starts[1],
            });
        }
        let s = starts[0];
        let (_, l, r) = best
            .iter()
            .filter(|x| x.0 == s)
            .min_by_key(|x| x.1)
            .copied()
            .unwrap();

        let mut replacement = Vec::new();
        for (ri, (p, text)) in h.rows.iter().enumerate() {
            if ri < l || (r > 0 && ri >= h.rows.len() - r) {
                continue;
            }
            if *p == b' ' || *p == b'+' {
                replacement.push(text.clone());
            }
        }

        let begin = s + l;
        let end = s + h.old_count - r;
        work.splice(begin..end, replacement);
        if work.len() > 128 {
            return Err(ApplyError::TooLarge);
        }

        drift = s as i64 - (h.old_start as i64 + net);
        net += h.new_count as i64 - h.old_count as i64;
    }

    let mut out = String::new();
    for line in work {
        out.push_str(&line);
        out.push('\n');
    }
    Ok(out)
}
