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

fn horizontal(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

fn printable(b: u8) -> bool {
    (0x20..=0x7e).contains(&b)
}

fn is_atom_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$%&'*+-/=?^_`{|}~".contains(&b)
}

pub fn parse_email(input: &str) -> Result<Address, EmailError> {
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
    let mut first_retained = true;
    let mut quoted_prefix = false;

    while i < bytes.len() {
        if quoted_prefix {
            let b = bytes[i];
            clean.push(b);
            map.push(i);
            i += 1;
            if b == b'\\' {
                if i < bytes.len() {
                    clean.push(bytes[i]);
                    map.push(i);
                    i += 1;
                }
            } else if b == b'"' {
                quoted_prefix = false;
            }
            continue;
        }

        let b = bytes[i];
        if b == b'(' {
            let run_start = i;
            let outer = i;
            let mut depth = 0usize;
            let mut j = i;
            while j < bytes.len() {
                let c = bytes[j];
                if c == b'\\' {
                    if j + 1 >= bytes.len() || !printable(bytes[j + 1]) {
                        return Err(EmailError::CommentEscape { at: j });
                    }
                    j += 2;
                    continue;
                }
                if c == b'\r' || c == b'\n' {
                    if c == b'\r'
                        && j + 2 < bytes.len()
                        && bytes[j + 1] == b'\n'
                        && horizontal(bytes[j + 2])
                    {
                        j += 2;
                        while j < bytes.len() && horizontal(bytes[j]) {
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
                if !(printable(c) || c == b'\t') {
                    return Err(EmailError::CommentChar { at: j });
                }
                j += 1;
            }
            if depth != 0 {
                return Err(EmailError::UnclosedComment { at: outer });
            }
            i = j;
            runs.push(Run { start: run_start, end: i });
            continue;
        }

        if b == b')' {
            return Err(EmailError::UnexpectedClose { at: i });
        }

        if b == b'\r' || b == b'\n' {
            if b == b'\r'
                && i + 2 < bytes.len()
                && bytes[i + 1] == b'\n'
                && horizontal(bytes[i + 2])
            {
                let start = i;
                i += 2;
                while i < bytes.len() && horizontal(bytes[i]) {
                    i += 1;
                }
                runs.push(Run { start, end: i });
                continue;
            }
            return Err(EmailError::BadFold { at: i });
        }

        if horizontal(b) {
            let start = i;
            while i < bytes.len() && horizontal(bytes[i]) {
                i += 1;
            }
            runs.push(Run { start, end: i });
            continue;
        }

        clean.push(b);
        map.push(i);
        i += 1;
        if first_retained {
            first_retained = false;
            if b == b'"' {
                quoted_prefix = true;
            }
        }
    }

    let end_position = input.len();
    for run in &runs {
        let left_at = map.iter().rposition(|&p| p < run.start);
        let right_at = map.iter().position(|&p| p >= run.end);
        let left_is_at = left_at.map_or(false, |k| clean[k] == b'@');
        let right_is_at = right_at.map_or(false, |k| clean[k] == b'@');
        let boundary = left_at.is_none() || right_at.is_none();
        if !boundary && !left_is_at && !right_is_at {
            return Err(EmailError::CfwsBoundary { at: run.start });
        }
    }

    if clean.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let mut quote_close = None;
    if clean[0] == b'"' {
        let mut q = 1usize;
        while q < clean.len() {
            if clean[q] == b'\\' {
                q += 2;
            } else if clean[q] == b'"' {
                quote_close = Some(q);
                break;
            } else {
                q += 1;
            }
        }
        if quote_close.is_none() {
            return Err(EmailError::UnclosedQuote { at: map[0] });
        }
    }

    let mut ats = Vec::new();
    for k in 0..clean.len() {
        if clean[k] == b'@' && !quote_close.map_or(false, |q| k <= q) {
            ats.push(k);
        }
    }
    if ats.is_empty() {
        return Err(EmailError::MissingAt);
    }
    if ats.len() >= 2 {
        return Err(EmailError::MultipleAt { at: map[ats[1]] });
    }
    let sep = ats[0];
    if sep == 0 {
        return Err(EmailError::EmptyLocal { at: map[sep] });
    }

    let (local, quoted) = if clean[0] == b'"' {
        let close = quote_close.unwrap();
        if close + 1 != sep {
            let at = if close + 1 < map.len() { map[close + 1] } else { end_position };
            return Err(EmailError::TrailingLocal { at });
        }

        let mut decoded = Vec::<u8>::new();
        let mut origins = Vec::<usize>::new();
        let mut p = 1usize;
        while p < close {
            let b = clean[p];
            if b == b'\\' {
                if p + 1 >= close
                    || !(clean[p + 1] == b'"' || clean[p + 1] == b'\\' || clean[p + 1] == b'@')
                {
                    return Err(EmailError::BadEscape { at: map[p] });
                }
                decoded.push(clean[p + 1]);
                origins.push(map[p]);
                p += 2;
            } else {
                if !printable(b) {
                    return Err(EmailError::QuotedChar { at: map[p] });
                }
                decoded.push(b);
                origins.push(map[p]);
                p += 1;
            }
        }
        if decoded.is_empty() {
            return Err(EmailError::EmptyLocal { at: map[close] });
        }
        if decoded.len() > 64 {
            return Err(EmailError::LocalTooLong { at: origins[64] });
        }
        (String::from_utf8(decoded).unwrap(), true)
    } else {
        let local_bytes = &clean[..sep];
        if local_bytes.len() > 64 {
            return Err(EmailError::LocalTooLong { at: map[64] });
        }
        for (k, &b) in local_bytes.iter().enumerate() {
            if b == b'.' {
                if k == 0 || k + 1 == local_bytes.len() || local_bytes[k - 1] == b'.' {
                    return Err(EmailError::LocalDot { at: map[k] });
                }
            } else if !is_atom_char(b) {
                return Err(EmailError::LocalChar { at: map[k] });
            }
        }
        (String::from_utf8(local_bytes.to_vec()).unwrap(), false)
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
            at: map.get(domain_start).copied().unwrap_or(end_position),
        });
    }
    if domain_end - domain_start > 253 {
        return Err(EmailError::DomainTooLong { at: map[domain_start + 253] });
    }

    let mut label_starts = Vec::<usize>::new();
    let mut label_ends = Vec::<usize>::new();
    let mut start = domain_start;
    let mut p = domain_start;
    while p <= domain_end {
        if p == domain_end || clean[p] == b'.' {
            if p == start {
                let at = if p == domain_end {
                    removed_dot.unwrap_or_else(|| map.get(p).copied().unwrap_or(end_position))
                } else {
                    map[p]
                };
                return Err(EmailError::EmptyLabel { at });
            }
            let len = p - start;
            if len > 63 {
                return Err(EmailError::LabelTooLong { at: map[start + 63] });
            }
            if clean[start] == b'-' {
                return Err(EmailError::LabelHyphen { at: map[start] });
            }
            if clean[p - 1] == b'-' {
                return Err(EmailError::LabelHyphen { at: map[p - 1] });
            }
            for k in start..p {
                let b = clean[k];
                if !(b.is_ascii_alphanumeric() || b == b'-') {
                    return Err(EmailError::DomainChar { at: map[k] });
                }
            }
            label_starts.push(start);
            label_ends.push(p);
            start = p + 1;
        }
        p += 1;
    }

    if label_starts.len() < 2 {
        return Err(EmailError::DomainNeedsDot { at: map[domain_start] });
    }

    let last_start = *label_starts.last().unwrap();
    let last_end = *label_ends.last().unwrap();
    if clean[last_start..last_end].iter().all(u8::is_ascii_digit) {
        return Err(EmailError::NumericTld { at: map[last_start] });
    }

    let domain = String::from_utf8(clean[domain_start..domain_end].to_vec())
        .unwrap()
        .to_ascii_lowercase();

    let _ = raw_domain;
    Ok(Address {
        local,
        domain,
        quoted,
    })
}