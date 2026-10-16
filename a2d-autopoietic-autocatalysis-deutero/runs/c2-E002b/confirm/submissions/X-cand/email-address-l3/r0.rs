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
    if bytes.get(i) != Some(&b'\r') || bytes.get(i + 1) != Some(&b'\n') {
        return None;
    }
    let mut j = i + 2;
    if !bytes.get(j).copied().is_some_and(is_hws) {
        return None;
    }
    while bytes.get(j).copied().is_some_and(is_hws) {
        j += 1;
    }
    Some(j)
}

fn printable(b: u8) -> bool {
    (0x20..=0x7e).contains(&b)
}

pub fn parse_email(input: &str) -> Result<Address, EmailError> {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return Err(EmailError::EmptyInput);
    }
    if let Some(at) = bytes.iter().position(|&b| b >= 128) {
        return Err(EmailError::NonAscii { at });
    }

    let mut clean = Vec::<u8>::with_capacity(bytes.len());
    let mut positions = Vec::<usize>::with_capacity(bytes.len());
    let mut runs = Vec::<Run>::new();
    let mut i = 0usize;
    let mut protected = false;
    let mut quote_open = false;

    while i < bytes.len() {
        if protected {
            let b = bytes[i];
            clean.push(b);
            positions.push(i);
            if b == b'\\' {
                i += 1;
                if i < bytes.len() {
                    clean.push(bytes[i]);
                    positions.push(i);
                    i += 1;
                }
            } else {
                if b == b'"' {
                    quote_open = false;
                    protected = false;
                }
                i += 1;
            }
            continue;
        }

        let b = bytes[i];
        if b == b'"' && clean.is_empty() {
            protected = true;
            quote_open = true;
            clean.push(b);
            positions.push(i);
            i += 1;
            continue;
        }

        let run_start = i;
        let mut consumed = false;
        while i < bytes.len() {
            let b = bytes[i];
            if is_hws(b) {
                i += 1;
                consumed = true;
            } else if b == b'\r' || b == b'\n' {
                if let Some(end) = fold_end(bytes, i) {
                    i = end;
                    consumed = true;
                } else {
                    return Err(EmailError::BadFold { at: i });
                }
            } else if b == b'(' {
                consumed = true;
                let outer = i;
                let mut depth = 1usize;
                i += 1;
                while i < bytes.len() {
                    let c = bytes[i];
                    if c == b'\\' {
                        let slash = i;
                        if i + 1 >= bytes.len() || !printable(bytes[i + 1]) {
                            return Err(EmailError::CommentEscape { at: slash });
                        }
                        i += 2;
                    } else if c == b'\r' || c == b'\n' {
                        if let Some(end) = fold_end(bytes, i) {
                            i = end;
                        } else {
                            return Err(EmailError::BadFold { at: i });
                        }
                    } else if c == b'(' {
                        if depth == 3 {
                            return Err(EmailError::CommentDepth { at: i });
                        }
                        depth += 1;
                        i += 1;
                    } else if c == b')' {
                        depth -= 1;
                        i += 1;
                        if depth == 0 {
                            break;
                        }
                    } else {
                        let valid = printable(c) || c == b'\t';
                        if !valid {
                            return Err(EmailError::CommentChar { at: i });
                        }
                        i += 1;
                    }
                }
                if depth != 0 {
                    return Err(EmailError::UnclosedComment { at: outer });
                }
            } else {
                break;
            }
        }

        if consumed {
            runs.push(Run { start: run_start, end: i });
            continue;
        }

        if b == b')' {
            return Err(EmailError::UnexpectedClose { at: i });
        }
        clean.push(b);
        positions.push(i);
        i += 1;
    }

    let _quote_open_at_eof = quote_open;

    for run in &runs {
        let left_at = run.start > 0 && bytes[run.start - 1] == b'@';
        let right_at = run.end < bytes.len() && bytes[run.end] == b'@';
        let before_clean = positions.first().is_some_and(|&p| run.end <= p);
        let after_clean = positions.last().is_some_and(|&p| run.start > p);
        if !before_clean && !after_clean && !left_at && !right_at {
            return Err(EmailError::CfwsBoundary { at: run.start });
        }
    }

    if clean.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let mut quote_end = None;
    if clean[0] == b'"' {
        let mut q = 1usize;
        while q < clean.len() {
            if clean[q] == b'\\' {
                q += 2;
            } else if clean[q] == b'"' {
                quote_end = Some(q);
                break;
            } else {
                q += 1;
            }
        }
        if quote_end.is_none() {
            return Err(EmailError::UnclosedQuote { at: positions[0] });
        }
    }

    let mut ats = Vec::new();
    for idx in 0..clean.len() {
        if clean[idx] == b'@' && !quote_end.is_some_and(|end| idx <= end) {
            ats.push(idx);
        }
    }
    if ats.is_empty() {
        return Err(EmailError::MissingAt);
    }
    if ats.len() > 1 {
        return Err(EmailError::MultipleAt {
            at: positions[ats[1]],
        });
    }
    let sep = ats[0];
    if sep == 0 {
        return Err(EmailError::EmptyLocal { at: positions[sep] });
    }

    let local_bytes = &clean[..sep];
    let local;
    let quoted;
    if local_bytes[0] == b'"' {
        let end = quote_end.expect("quoted prefix has a closing quote");
        if end + 1 < sep {
            return Err(EmailError::TrailingLocal {
                at: positions[end + 1],
            });
        }
        let mut decoded = Vec::new();
        let mut origins = Vec::new();
        let mut p = 1usize;
        while p < end {
            let b = local_bytes[p];
            if b == b'\\' {
                if p + 1 >= end {
                    return Err(EmailError::BadEscape { at: positions[p] });
                }
                let escaped = local_bytes[p + 1];
                if !matches!(escaped, b'"' | b'\\' | b'@') {
                    return Err(EmailError::BadEscape { at: positions[p] });
                }
                decoded.push(escaped);
                origins.push(positions[p]);
                p += 2;
            } else {
                if !printable(b) {
                    return Err(EmailError::QuotedChar { at: positions[p] });
                }
                decoded.push(b);
                origins.push(positions[p]);
                p += 1;
            }
        }
        if decoded.is_empty() {
            return Err(EmailError::EmptyLocal { at: positions[end] });
        }
        if decoded.len() > 64 {
            return Err(EmailError::LocalTooLong { at: origins[64] });
        }
        local = String::from_utf8(decoded).expect("ASCII local");
        quoted = true;
    } else {
        if local_bytes.len() > 64 {
            return Err(EmailError::LocalTooLong { at: positions[64] });
        }
        for idx in 0..local_bytes.len() {
            let b = local_bytes[idx];
            if b == b'.' {
                if idx == 0 || idx + 1 == local_bytes.len() || local_bytes[idx - 1] == b'.' {
                    return Err(EmailError::LocalDot { at: positions[idx] });
                }
            } else if !(b.is_ascii_alphanumeric()
                || b"!#$%&'*+-/=?^_`{|}~".contains(&b))
            {
                return Err(EmailError::LocalChar { at: positions[idx] });
            }
        }
        local = String::from_utf8(local_bytes.to_vec()).expect("ASCII local");
        quoted = false;
    }

    let domain_start = sep + 1;
    let domain_raw = &clean[domain_start..];
    let mut domain_end = clean.len();
    let root_dot = if domain_raw.last() == Some(&b'.') {
        domain_end -= 1;
        Some(positions[domain_end])
    } else {
        None
    };
    if domain_end == domain_start {
        return Err(EmailError::EmptyDomain {
            at: positions.get(domain_start).copied().unwrap_or(input.len()),
        });
    }
    let domain_len = domain_end - domain_start;
    if domain_len > 253 {
        return Err(EmailError::DomainTooLong {
            at: positions[domain_start + 253],
        });
    }

    let mut labels = Vec::<(usize, usize)>::new();
    let mut label_start = domain_start;
    for idx in domain_start..domain_end {
        if clean[idx] == b'.' {
            if idx == label_start {
                return Err(EmailError::EmptyLabel { at: positions[idx] });
            }
            labels.push((label_start, idx));
            label_start = idx + 1;
        }
    }
    if label_start == domain_end {
        return Err(EmailError::EmptyLabel {
            at: root_dot.unwrap_or_else(|| positions.get(domain_end).copied().unwrap_or(input.len())),
        });
    }
    labels.push((label_start, domain_end));

    for &(start, end) in &labels {
        if end - start > 63 {
            return Err(EmailError::LabelTooLong {
                at: positions[start + 63],
            });
        }
        if clean[start] == b'-' {
            return Err(EmailError::LabelHyphen { at: positions[start] });
        }
        if clean[end - 1] == b'-' {
            return Err(EmailError::LabelHyphen { at: positions[end - 1] });
        }
        for idx in start..end {
            let b = clean[idx];
            if !(b.is_ascii_alphanumeric() || b == b'-') {
                return Err(EmailError::DomainChar { at: positions[idx] });
            }
        }
    }

    if labels.len() < 2 {
        return Err(EmailError::DomainNeedsDot {
            at: positions[domain_start],
        });
    }
    let (last_start, last_end) = labels[labels.len() - 1];
    if clean[last_start..last_end].iter().all(u8::is_ascii_digit) {
        return Err(EmailError::NumericTld {
            at: positions[last_start],
        });
    }

    let domain = clean[domain_start..domain_end]
        .iter()
        .map(|b| b.to_ascii_lowercase() as char)
        .collect::<String>();

    Ok(Address {
        local,
        domain,
        quoted,
    })
}