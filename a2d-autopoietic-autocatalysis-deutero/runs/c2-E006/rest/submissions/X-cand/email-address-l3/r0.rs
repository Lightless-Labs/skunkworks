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

pub fn parse_email(input: &str) -> Result<Address, EmailError> {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return Err(EmailError::EmptyInput);
    }
    if let Some(at) = bytes.iter().position(|&b| b >= 128) {
        return Err(EmailError::NonAscii { at });
    }

    let mut clean = Vec::<(u8, usize)>::new();
    let mut runs = Vec::<Run>::new();
    let mut i = 0;
    let mut protected_quote = false;
    let mut quote_open = 0;

    while i < bytes.len() {
        if protected_quote {
            let b = bytes[i];
            clean.push((b, i));
            i += 1;
            if b == b'\\' && i < bytes.len() {
                clean.push((bytes[i], i));
                i += 1;
            } else if b == b'"' {
                protected_quote = false;
            }
            continue;
        }

        let b = bytes[i];
        if b == b'"' && clean.is_empty() {
            protected_quote = true;
            quote_open = i;
            clean.push((b, i));
            i += 1;
            continue;
        }

        let cfws_start = i;
        let mut consumed = false;
        while i < bytes.len() {
            if is_hws(bytes[i]) {
                consumed = true;
                i += 1;
                continue;
            }
            if bytes[i] == b'\r' || bytes[i] == b'\n' {
                if let Some(end) = fold_end(bytes, i) {
                    consumed = true;
                    i = end;
                    continue;
                }
                return Err(EmailError::BadFold { at: i });
            }
            if bytes[i] == b'(' {
                consumed = true;
                let outer = i;
                let mut depth = 1usize;
                i += 1;
                while i < bytes.len() && depth > 0 {
                    let c = bytes[i];
                    if c == b'\\' {
                        let slash = i;
                        if i + 1 >= bytes.len()
                            || !(0x20..=0x7e).contains(&bytes[i + 1])
                        {
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
                    } else if c == b'\t' || (0x20..=0x7e).contains(&c) {
                        i += 1;
                    } else {
                        return Err(EmailError::CommentChar { at: i });
                    }
                }
                if depth > 0 {
                    return Err(EmailError::UnclosedComment { at: outer });
                }
                continue;
            }
            break;
        }
        if consumed {
            runs.push(Run {
                start: cfws_start,
                end: i,
            });
            continue;
        }

        if b == b')' {
            return Err(EmailError::UnexpectedClose { at: i });
        }
        clean.push((b, i));
        i += 1;
    }

    let clean_bytes: Vec<u8> = clean.iter().map(|x| x.0).collect();
    for run in &runs {
        let left = clean.iter().rposition(|x| x.1 < run.start);
        let right = clean.iter().position(|x| x.1 >= run.end);
        let at_edge = left.is_none() || right.is_none();
        let beside_at = left.is_some_and(|n| clean[n].0 == b'@')
            || right.is_some_and(|n| clean[n].0 == b'@');
        if !at_edge && !beside_at {
            return Err(EmailError::CfwsBoundary { at: run.start });
        }
    }

    if clean.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let mut quote_end = None;
    if clean_bytes.first() == Some(&b'"') {
        let mut q = 1usize;
        while q < clean_bytes.len() {
            if clean_bytes[q] == b'\\' {
                q += 2;
            } else if clean_bytes[q] == b'"' {
                quote_end = Some(q);
                break;
            } else {
                q += 1;
            }
        }
        if quote_end.is_none() {
            return Err(EmailError::UnclosedQuote { at: quote_open });
        }
    }

    let mut ats = Vec::new();
    let mut j = 0;
    while j < clean_bytes.len() {
        if let Some(end) = quote_end {
            if j <= end {
                j += 1;
                continue;
            }
        }
        if clean_bytes[j] == b'@' {
            ats.push(j);
        }
        j += 1;
    }
    if ats.is_empty() {
        return Err(EmailError::MissingAt);
    }
    if ats.len() > 1 {
        return Err(EmailError::MultipleAt {
            at: clean[ats[1]].1,
        });
    }

    let sep = ats[0];
    if sep == 0 {
        return Err(EmailError::EmptyLocal { at: clean[sep].1 });
    }

    let quoted = clean_bytes[0] == b'"';
    let local: String;
    if quoted {
        let end = quote_end.expect("quoted prefix has closing quote");
        if end + 1 != sep {
            return Err(EmailError::TrailingLocal {
                at: clean[end + 1].1,
            });
        }
        let mut decoded = Vec::new();
        let mut origins = Vec::new();
        let mut k = 1;
        while k < end {
            let b = clean_bytes[k];
            if b == b'\\' {
                if k + 1 >= end
                    || !matches!(clean_bytes[k + 1], b'"' | b'\\' | b'@')
                {
                    return Err(EmailError::BadEscape { at: clean[k].1 });
                }
                decoded.push(clean_bytes[k + 1]);
                origins.push(clean[k].1);
                k += 2;
            } else {
                if !(0x20..=0x7e).contains(&b) {
                    return Err(EmailError::QuotedChar { at: clean[k].1 });
                }
                decoded.push(b);
                origins.push(clean[k].1);
                k += 1;
            }
        }
        if decoded.is_empty() {
            return Err(EmailError::EmptyLocal { at: clean[end].1 });
        }
        if decoded.len() > 64 {
            return Err(EmailError::LocalTooLong { at: origins[64] });
        }
        local = String::from_utf8(decoded).expect("ASCII decoded bytes");
    } else {
        if sep > 64 {
            return Err(EmailError::LocalTooLong { at: clean[64].1 });
        }
        for k in 0..sep {
            let b = clean_bytes[k];
            if b == b'.' {
                if k == 0 || k + 1 == sep || clean_bytes[k - 1] == b'.' {
                    return Err(EmailError::LocalDot { at: clean[k].1 });
                }
            } else if !b.is_ascii_alphanumeric()
                && !b"!#$%&'*+-/=?^_`{|}~".contains(&b)
            {
                return Err(EmailError::LocalChar { at: clean[k].1 });
            }
        }
        local = String::from_utf8(clean_bytes[..sep].to_vec()).expect("ASCII local");
    }

    let domain_start = sep + 1;
    let domain_raw = &clean_bytes[domain_start..];
    let domain_positions = &clean[domain_start..];
    let mut domain_len = domain_raw.len();
    let root_dot = if domain_len > 0 && domain_raw[domain_len - 1] == b'.' {
        domain_len -= 1;
        Some(domain_positions[domain_len].1)
    } else {
        None
    };
    let domain_start_pos = clean
        .get(domain_start)
        .map(|x| x.1)
        .or(root_dot)
        .unwrap_or(input.len());

    if domain_len == 0 {
        return Err(EmailError::EmptyDomain {
            at: domain_start_pos,
        });
    }
    if domain_len > 253 {
        return Err(EmailError::DomainTooLong {
            at: domain_positions[253].1,
        });
    }

    let mut labels: Vec<(usize, usize)> = Vec::new();
    let mut start = 0;
    for k in 0..=domain_len {
        if k == domain_len || domain_raw[k] == b'.' {
            labels.push((start, k));
            start = k + 1;
        }
    }

    for &(a, z) in &labels {
        if a == z {
            let pos = if a < domain_len {
                domain_positions[a].1
            } else {
                root_dot.unwrap_or(input.len())
            };
            return Err(EmailError::EmptyLabel { at: pos });
        }
        if z - a > 63 {
            return Err(EmailError::LabelTooLong {
                at: domain_positions[a + 63].1,
            });
        }
        if domain_raw[a] == b'-' {
            return Err(EmailError::LabelHyphen {
                at: domain_positions[a].1,
            });
        }
        if domain_raw[z - 1] == b'-' {
            return Err(EmailError::LabelHyphen {
                at: domain_positions[z - 1].1,
            });
        }
        for k in a..z {
            let b = domain_raw[k];
            if !b.is_ascii_alphanumeric() && b != b'-' {
                return Err(EmailError::DomainChar {
                    at: domain_positions[k].1,
                });
            }
        }
    }

    if labels.len() < 2 {
        return Err(EmailError::DomainNeedsDot {
            at: domain_start_pos,
        });
    }

    let (last_start, last_end) = *labels.last().expect("at least one label");
    if domain_raw[last_start..last_end]
        .iter()
        .all(u8::is_ascii_digit)
    {
        return Err(EmailError::NumericTld {
            at: domain_positions[last_start].1,
        });
    }

    let domain = String::from_utf8(domain_raw[..domain_len].to_vec())
        .expect("ASCII domain")
        .to_ascii_lowercase();

    Ok(Address {
        local,
        domain,
        quoted,
    })
}