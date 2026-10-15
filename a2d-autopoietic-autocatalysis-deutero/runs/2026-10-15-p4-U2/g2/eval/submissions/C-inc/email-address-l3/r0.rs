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

#[derive(Clone)]
struct Run {
    start: usize,
    end: usize,
}

fn is_hws(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

fn is_printable(b: u8) -> bool {
    (0x20..=0x7e).contains(&b)
}

fn parse_email(input: &str) -> Result<Address, EmailError> {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return Err(EmailError::EmptyInput);
    }
    if let Some(at) = bytes.iter().position(|&b| b >= 128) {
        return Err(EmailError::NonAscii { at });
    }

    let mut clean = Vec::<u8>::new();
    let mut map = Vec::<usize>::new();
    let mut runs = Vec::<Run>::new();
    let mut i = 0usize;
    let mut quote_mode = false;
    let mut quote_escaped = false;

    while i < bytes.len() {
        let b = bytes[i];

        if quote_mode {
            clean.push(b);
            map.push(i);
            i += 1;
            if quote_escaped {
                quote_escaped = false;
            } else if b == b'\\' {
                quote_escaped = true;
            } else if b == b'"' {
                quote_mode = false;
            }
            continue;
        }

        let mut consumed = 0usize;
        if b == b' ' || b == b'\t' {
            consumed = 1;
            while i + consumed < bytes.len() && is_hws(bytes[i + consumed]) {
                consumed += 1;
            }
        } else if b == b'\r' || b == b'\n' {
            if b == b'\r'
                && i + 2 < bytes.len()
                && bytes[i + 1] == b'\n'
                && is_hws(bytes[i + 2])
            {
                consumed = 2;
                while i + consumed < bytes.len() && is_hws(bytes[i + consumed]) {
                    consumed += 1;
                }
            } else {
                return Err(EmailError::BadFold { at: i });
            }
        } else if b == b'(' {
            let outer = i;
            let mut depth = 1usize;
            let mut j = i + 1;
            while j < bytes.len() {
                let c = bytes[j];
                if c == b'\\' {
                    if j + 1 >= bytes.len() || !is_printable(bytes[j + 1]) {
                        return Err(EmailError::CommentEscape { at: j });
                    }
                    j += 2;
                    continue;
                }
                if c == b'\r' || c == b'\n' {
                    if c == b'\r'
                        && j + 2 < bytes.len()
                        && bytes[j + 1] == b'\n'
                        && is_hws(bytes[j + 2])
                    {
                        j += 2;
                        while j < bytes.len() && is_hws(bytes[j]) {
                            j += 1;
                        }
                        continue;
                    }
                    return Err(EmailError::BadFold { at: j });
                }
                if c == b'(' {
                    depth += 1;
                    if depth > 3 {
                        return Err(EmailError::CommentDepth { at: j });
                    }
                    j += 1;
                    continue;
                }
                if c == b')' {
                    depth -= 1;
                    j += 1;
                    if depth == 0 {
                        break;
                    }
                    continue;
                }
                if !(is_printable(c) || c == b'\t') {
                    return Err(EmailError::CommentChar { at: j });
                }
                j += 1;
            }
            if depth != 0 {
                return Err(EmailError::UnclosedComment { at: outer });
            }
            consumed = j - i;
        } else if b == b')' {
            return Err(EmailError::UnexpectedClose { at: i });
        }

        if consumed > 0 {
            runs.push(Run {
                start: i,
                end: i + consumed,
            });
            i += consumed;
            continue;
        }

        let is_first_retained = clean.is_empty();
        clean.push(b);
        map.push(i);
        i += 1;
        if is_first_retained && b == b'"' {
            quote_mode = true;
            quote_escaped = false;
        }
    }

    for run in &runs {
        let left_ok = clean.is_empty()
            || run.start == 0
            || map.iter().position(|&p| p >= run.start).map_or(true, |idx| {
                idx == 0 || clean[idx - 1] == b'@'
            });
        let right_idx = map.iter().position(|&p| p >= run.end);
        let right_ok = right_idx.map_or(true, |idx| clean[idx] == b'@');
        if !(left_ok || right_ok) {
            return Err(EmailError::CfwsBoundary { at: run.start });
        }
    }

    if clean.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let mut quote_close = None;
    if clean[0] == b'"' {
        let mut j = 1usize;
        while j < clean.len() {
            if clean[j] == b'\\' {
                j += 2;
            } else if clean[j] == b'"' {
                quote_close = Some(j);
                break;
            } else {
                j += 1;
            }
        }
        if quote_close.is_none() {
            return Err(EmailError::UnclosedQuote { at: map[0] });
        }
    }

    let mut separators = Vec::new();
    let mut j = 0usize;
    while j < clean.len() {
        if let Some(close) = quote_close {
            if j <= close {
                j = close + 1;
                continue;
            }
        }
        if clean[j] == b'@' {
            separators.push(j);
        }
        j += 1;
    }
    if separators.is_empty() {
        return Err(EmailError::MissingAt);
    }
    if separators.len() >= 2 {
        return Err(EmailError::MultipleAt {
            at: map[separators[1]],
        });
    }
    let sep = separators[0];
    if sep == 0 {
        return Err(EmailError::EmptyLocal { at: map[sep] });
    }

    let local_bytes = &clean[..sep];
    let local = if local_bytes[0] == b'"' {
        let close = quote_close.unwrap();
        if close + 1 != sep {
            return Err(EmailError::TrailingLocal { at: map[close + 1] });
        }
        let mut decoded = Vec::new();
        let mut origins = Vec::new();
        let mut k = 1usize;
        while k < close {
            let b = clean[k];
            if b == b'\\' {
                if k + 1 >= close
                    || !(clean[k + 1] == b'"' || clean[k + 1] == b'\\' || clean[k + 1] == b'@')
                {
                    return Err(EmailError::BadEscape { at: map[k] });
                }
                decoded.push(clean[k + 1]);
                origins.push(map[k]);
                k += 2;
            } else {
                if !is_printable(b) {
                    return Err(EmailError::QuotedChar { at: map[k] });
                }
                decoded.push(b);
                origins.push(map[k]);
                k += 1;
            }
        }
        if decoded.is_empty() {
            return Err(EmailError::EmptyLocal { at: map[close] });
        }
        if decoded.len() > 64 {
            return Err(EmailError::LocalTooLong { at: origins[64] });
        }
        String::from_utf8(decoded).unwrap()
    } else {
        if local_bytes.len() > 64 {
            return Err(EmailError::LocalTooLong { at: map[64] });
        }
        for k in 0..local_bytes.len() {
            let b = local_bytes[k];
            if b == b'.' {
                if k == 0 || k + 1 == local_bytes.len() || local_bytes[k - 1] == b'.' {
                    return Err(EmailError::LocalDot { at: map[k] });
                }
            } else if !(b.is_ascii_alphanumeric()
                || b"!#$%&'*+-/=?^_`{|}~".contains(&b))
            {
                return Err(EmailError::LocalChar { at: map[k] });
            }
        }
        String::from_utf8(local_bytes.to_vec()).unwrap()
    };

    let domain_start = sep + 1;
    let raw_domain = &clean[domain_start..];
    let mut domain_end = clean.len();
    let mut removed_dot = None;
    if domain_end > domain_start && clean[domain_end - 1] == b'.' {
        domain_end -= 1;
        removed_dot = Some(map[domain_end]);
    }
    if domain_end == domain_start {
        return Err(EmailError::EmptyDomain {
            at: map.get(domain_start).copied().unwrap_or(input.len()),
        });
    }
    if domain_end - domain_start > 253 {
        return Err(EmailError::DomainTooLong { at: map[domain_start + 253] });
    }

    let mut labels: Vec<(usize, usize)> = Vec::new();
    let mut start = domain_start;
    for p in domain_start..domain_end {
        if clean[p] == b'.' {
            if p == start {
                return Err(EmailError::EmptyLabel { at: map[p] });
            }
            labels.push((start, p));
            start = p + 1;
        }
    }
    if start == domain_end {
        return Err(EmailError::EmptyLabel {
            at: removed_dot.unwrap_or_else(|| {
                map.get(domain_end).copied().unwrap_or(input.len())
            }),
        });
    }
    labels.push((start, domain_end));

    for &(a, b) in &labels {
        let len = b - a;
        if len > 63 {
            return Err(EmailError::LabelTooLong { at: map[a + 63] });
        }
        if clean[a] == b'-' {
            return Err(EmailError::LabelHyphen { at: map[a] });
        }
        if clean[b - 1] == b'-' {
            return Err(EmailError::LabelHyphen { at: map[b - 1] });
        }
        for p in a..b {
            let c = clean[p];
            if !(c.is_ascii_alphanumeric() || c == b'-') {
                return Err(EmailError::DomainChar { at: map[p] });
            }
        }
    }

    if labels.len() < 2 {
        return Err(EmailError::DomainNeedsDot {
            at: map.get(domain_start).copied().unwrap_or(input.len()),
        });
    }
    let (last_a, last_b) = *labels.last().unwrap();
    if clean[last_a..last_b].iter().all(u8::is_ascii_digit) {
        return Err(EmailError::NumericTld { at: map[last_a] });
    }

    let domain = clean[domain_start..domain_end]
        .iter()
        .map(|&b| b.to_ascii_lowercase() as char)
        .collect::<String>();
    let _ = raw_domain;

    Ok(Address {
        local,
        domain,
        quoted: local_bytes[0] == b'"',
    })
}

fn main() {}