#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    pub local: String,
    pub domain: String,
    pub quoted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmailError {
    EmptyInput,
    NonAscii { at: usize },
    MissingAt,
    MultipleAt { at: usize },
    EmptyLocal { at: usize },
    LocalTooLong { at: usize },
    LocalDot { at: usize },
    LocalChar { at: usize },
    EmptyDomain { at: usize },
    DomainTooLong { at: usize },
    EmptyLabel { at: usize },
    LabelTooLong { at: usize },
    LabelHyphen { at: usize },
    DomainChar { at: usize },
    DomainNeedsDot { at: usize },
    NumericTld { at: usize },
    UnclosedQuote { at: usize },
    TrailingLocal { at: usize },
    BadEscape { at: usize },
    QuotedChar { at: usize },
    BadFold { at: usize },
    UnclosedComment { at: usize },
    CommentDepth { at: usize },
    CommentChar { at: usize },
    CommentEscape { at: usize },
    UnexpectedClose { at: usize },
    CfwsBoundary { at: usize },
}

#[derive(Clone, Copy)]
struct Run {
    start: usize,
    end: usize,
}

fn is_hws(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

fn fold_end(bytes: &[u8], i: usize) -> Option<usize> {
    if i + 2 >= bytes.len() || bytes[i] != b'\r' || bytes[i + 1] != b'\n' || !is_hws(bytes[i + 2]) {
        return None;
    }
    let mut j = i + 2;
    while j < bytes.len() && is_hws(bytes[j]) {
        j += 1;
    }
    Some(j)
}

fn cleanup(input: &[u8]) -> Result<(Vec<u8>, Vec<usize>, Vec<Run>), EmailError> {
    let mut clean = Vec::with_capacity(input.len());
    let mut map = Vec::with_capacity(input.len());
    let mut runs = Vec::new();
    let mut i = 0;
    let mut protected = false;
    let mut escaped_quote = false;

    while i < input.len() {
        if protected {
            let b = input[i];
            clean.push(b);
            map.push(i);
            i += 1;
            if escaped_quote {
                escaped_quote = false;
            } else if b == b'\\' {
                if i < input.len() {
                    clean.push(input[i]);
                    map.push(i);
                    i += 1;
                    escaped_quote = false;
                }
            } else if b == b'"' {
                protected = false;
            }
            continue;
        }

        let b = input[i];
        if b == b'"' && clean.is_empty() {
            protected = true;
            clean.push(b);
            map.push(i);
            i += 1;
            continue;
        }

        if b == b' ' || b == b'\t' || b == b'\r' || b == b'\n' || b == b'(' || b == b')' {
            let start = i;
            let mut depth = 0usize;
            let mut in_comment = false;
            let mut outer = 0usize;

            loop {
                if i >= input.len() {
                    if in_comment {
                        return Err(EmailError::UnclosedComment { at: outer });
                    }
                    break;
                }
                let c = input[i];
                if in_comment {
                    if c == b'\\' {
                        if i + 1 >= input.len() || !(0x20..=0x7e).contains(&input[i + 1]) {
                            return Err(EmailError::CommentEscape { at: i });
                        }
                        i += 2;
                    } else if c == b'\r' || c == b'\n' {
                        if let Some(end) = fold_end(input, i) {
                            i = end;
                        } else {
                            return Err(EmailError::BadFold { at: i });
                        }
                    } else if c == b'(' {
                        depth += 1;
                        if depth > 3 {
                            return Err(EmailError::CommentDepth { at: i });
                        }
                        i += 1;
                    } else if c == b')' {
                        depth -= 1;
                        i += 1;
                        if depth == 0 {
                            in_comment = false;
                        }
                    } else if is_hws(c) || (0x20..=0x7e).contains(&c) {
                        i += 1;
                    } else {
                        return Err(EmailError::CommentChar { at: i });
                    }
                } else if is_hws(c) {
                    i += 1;
                } else if c == b'\r' || c == b'\n' {
                    if let Some(end) = fold_end(input, i) {
                        i = end;
                    } else {
                        return Err(EmailError::BadFold { at: i });
                    }
                } else if c == b'(' {
                    outer = i;
                    depth = 1;
                    in_comment = true;
                    i += 1;
                } else if c == b')' {
                    return Err(EmailError::UnexpectedClose { at: i });
                } else {
                    break;
                }
            }
            runs.push(Run { start, end: i });
            continue;
        }

        clean.push(b);
        map.push(i);
        i += 1;
    }

    Ok((clean, map, runs))
}

pub fn parse_email(input: &str) -> Result<Address, EmailError> {
    if input.is_empty() {
        return Err(EmailError::EmptyInput);
    }
    let bytes = input.as_bytes();
    if let Some(at) = bytes.iter().position(|&b| b >= 128) {
        return Err(EmailError::NonAscii { at });
    }

    let (clean, map, runs) = cleanup(bytes)?;
    for run in &runs {
        let before = run.start == 0 || clean.get(run.start.wrapping_sub(1)) == Some(&b'@');
        let after = run.end == bytes.len() || clean.get(run.start).is_none()
            || clean.get(run.start) == Some(&b'@');
        let at_start = run.start == 0;
        let at_end = run.end == bytes.len();
        let adjacent_left = run.start > 0 && clean.get(run.start.wrapping_sub(1)) == Some(&b'@');
        let adjacent_right = map.get(run.end).map(|_| false).unwrap_or(true)
            || (run.end < bytes.len()
                && clean.get(map.partition_point(|&p| p < run.end)) == Some(&b'@'));
        if !(at_start || at_end || adjacent_left || adjacent_right) {
            return Err(EmailError::CfwsBoundary { at: run.start });
        }
        let _ = (before, after);
    }
    if clean.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let mut quote_end = None;
    if clean[0] == b'"' {
        let mut i = 1;
        while i < clean.len() {
            if clean[i] == b'\\' {
                i += 2;
            } else if clean[i] == b'"' {
                quote_end = Some(i);
                break;
            } else {
                i += 1;
            }
        }
        if quote_end.is_none() {
            return Err(EmailError::UnclosedQuote { at: map[0] });
        }
    }

    let mut separators = Vec::new();
    let mut i = 0;
    while i < clean.len() {
        if let Some(end) = quote_end {
            if i <= end {
                i = end + 1;
                continue;
            }
        }
        if clean[i] == b'@' {
            separators.push(i);
        }
        i += 1;
    }
    if separators.is_empty() {
        return Err(EmailError::MissingAt);
    }
    if separators.len() > 1 {
        return Err(EmailError::MultipleAt { at: map[separators[1]] });
    }
    let sep = separators[0];
    if sep == 0 {
        return Err(EmailError::EmptyLocal { at: map[sep] });
    }
    let local_bytes = &clean[..sep];
    let local_map = &map[..sep];
    let quoted = local_bytes[0] == b'"';
    let local = if !quoted {
        if local_bytes.len() > 64 {
            return Err(EmailError::LocalTooLong { at: local_map[64] });
        }
        for (i, &b) in local_bytes.iter().enumerate() {
            if b == b'.' {
                if i == 0 || i + 1 == local_bytes.len() || local_bytes[i - 1] == b'.' {
                    return Err(EmailError::LocalDot { at: local_map[i] });
                }
            } else if !b.is_ascii_alphanumeric() && !b"!#$%&'*+-/=?^_`{|}~".contains(&b) {
                return Err(EmailError::LocalChar { at: local_map[i] });
            }
        }
        String::from_utf8(local_bytes.to_vec()).unwrap()
    } else {
        let end = quote_end.unwrap();
        if end + 1 != sep {
            return Err(EmailError::TrailingLocal { at: map[end + 1] });
        }
        let mut decoded = Vec::new();
        let mut origins = Vec::new();
        let mut i = 1;
        while i < end {
            let b = clean[i];
            if b == b'\\' {
                if i + 1 >= end || !matches!(clean[i + 1], b'"' | b'\\' | b'@') {
                    return Err(EmailError::BadEscape { at: map[i] });
                }
                decoded.push(clean[i + 1]);
                origins.push(map[i]);
                i += 2;
            } else {
                if !(0x20..=0x7e).contains(&b) {
                    return Err(EmailError::QuotedChar { at: map[i] });
                }
                decoded.push(b);
                origins.push(map[i]);
                i += 1;
            }
        }
        if decoded.is_empty() {
            return Err(EmailError::EmptyLocal { at: map[end] });
        }
        if decoded.len() > 64 {
            return Err(EmailError::LocalTooLong { at: origins[64] });
        }
        String::from_utf8(decoded).unwrap()
    };

    let domain_start = sep + 1;
    let mut domain_end = clean.len();
    let mut root_pos = None;
    if domain_end > domain_start && clean[domain_end - 1] == b'.' {
        domain_end -= 1;
        root_pos = Some(map[domain_end]);
    }
    if domain_start == domain_end {
        return Err(EmailError::EmptyDomain {
            at: if domain_start < map.len() { map[domain_start] } else { input.len() },
        });
    }
    let domain_len = domain_end - domain_start;
    if domain_len > 253 {
        return Err(EmailError::DomainTooLong { at: map[domain_start + 253] });
    }

    let mut labels: Vec<(usize, usize)> = Vec::new();
    let mut start = domain_start;
    for i in domain_start..domain_end {
        if clean[i] == b'.' {
            if i == start {
                return Err(EmailError::EmptyLabel { at: map[i] });
            }
            labels.push((start, i));
            start = i + 1;
        }
    }
    if start == domain_end {
        return Err(EmailError::EmptyLabel {
            at: root_pos.unwrap_or(input.len()),
        });
    }
    labels.push((start, domain_end));

    for &(a, b) in &labels {
        if b - a > 63 {
            return Err(EmailError::LabelTooLong { at: map[a + 63] });
        }
        if clean[a] == b'-' {
            return Err(EmailError::LabelHyphen { at: map[a] });
        }
        if clean[b - 1] == b'-' {
            return Err(EmailError::LabelHyphen { at: map[b - 1] });
        }
        for i in a..b {
            if !clean[i].is_ascii_alphanumeric() && clean[i] != b'-' {
                return Err(EmailError::DomainChar { at: map[i] });
            }
        }
    }
    if labels.len() < 2 {
        return Err(EmailError::DomainNeedsDot { at: map[domain_start] });
    }
    let (last_start, last_end) = *labels.last().unwrap();
    if clean[last_start..last_end].iter().all(u8::is_ascii_digit) {
        return Err(EmailError::NumericTld { at: map[last_start] });
    }

    let domain = clean[domain_start..domain_end]
        .iter()
        .map(|b| b.to_ascii_lowercase() as char)
        .collect::<String>();
    Ok(Address { local, domain, quoted })
}

fn main() {}