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
struct CfwsRun {
    start: usize,
    end: usize,
}

fn is_hws(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

fn is_printable(b: u8) -> bool {
    (0x20..=0x7e).contains(&b)
}

fn is_fold_at(bytes: &[u8], i: usize) -> bool {
    i + 2 < bytes.len()
        && bytes[i] == b'\r'
        && bytes[i + 1] == b'\n'
        && is_hws(bytes[i + 2])
}

fn parse_email(input: &str) -> Result<Address, EmailError> {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return Err(EmailError::EmptyInput);
    }
    if let Some(at) = bytes.iter().position(|&b| b >= 128) {
        return Err(EmailError::NonAscii { at });
    }

    let mut cleaned = Vec::<u8>::with_capacity(bytes.len());
    let mut positions = Vec::<usize>::with_capacity(bytes.len());
    let mut runs = Vec::<CfwsRun>::new();
    let mut i = 0usize;
    let mut quoted_prefix = false;
    let mut in_quote = false;

    while i < bytes.len() {
        if in_quote {
            let b = bytes[i];
            cleaned.push(b);
            positions.push(i);
            if b == b'\\' {
                i += 1;
                if i < bytes.len() {
                    cleaned.push(bytes[i]);
                    positions.push(i);
                    i += 1;
                }
            } else {
                if b == b'"' {
                    in_quote = false;
                }
                i += 1;
            }
            continue;
        }

        if !quoted_prefix && cleaned.is_empty() && bytes[i] == b'"' {
            quoted_prefix = true;
            in_quote = true;
            cleaned.push(bytes[i]);
            positions.push(i);
            i += 1;
            continue;
        }

        let begins_run = is_hws(bytes[i]) || bytes[i] == b'(' || is_fold_at(bytes, i);
        if begins_run {
            let start = i;
            loop {
                if i >= bytes.len() {
                    break;
                }
                if is_hws(bytes[i]) {
                    i += 1;
                    continue;
                }
                if is_fold_at(bytes, i) {
                    i += 2;
                    while i < bytes.len() && is_hws(bytes[i]) {
                        i += 1;
                    }
                    continue;
                }
                if bytes[i] == b'\r' || bytes[i] == b'\n' {
                    return Err(EmailError::BadFold { at: i });
                }
                if bytes[i] == b'(' {
                    let outer = i;
                    let mut depth = 0usize;
                    while i < bytes.len() {
                        let b = bytes[i];
                        if b == b'\\' {
                            let slash = i;
                            if i + 1 >= bytes.len() || !is_printable(bytes[i + 1]) {
                                return Err(EmailError::CommentEscape { at: slash });
                            }
                            i += 2;
                            continue;
                        }
                        if b == b'\r' || b == b'\n' {
                            if !is_fold_at(bytes, i) {
                                return Err(EmailError::BadFold { at: i });
                            }
                            i += 2;
                            while i < bytes.len() && is_hws(bytes[i]) {
                                i += 1;
                            }
                            continue;
                        }
                        if b == b'(' {
                            depth += 1;
                            if depth > 3 {
                                return Err(EmailError::CommentDepth { at: i });
                            }
                            i += 1;
                            continue;
                        }
                        if b == b')' {
                            depth -= 1;
                            i += 1;
                            if depth == 0 {
                                break;
                            }
                            continue;
                        }
                        if !is_printable(b) && b != b'\t' {
                            return Err(EmailError::CommentChar { at: i });
                        }
                        i += 1;
                    }
                    if depth != 0 {
                        return Err(EmailError::UnclosedComment { at: outer });
                    }
                    continue;
                }
                break;
            }
            runs.push(CfwsRun { start, end: i });
            continue;
        }

        if bytes[i] == b'\r' || bytes[i] == b'\n' {
            return Err(EmailError::BadFold { at: i });
        }
        if bytes[i] == b')' {
            return Err(EmailError::UnexpectedClose { at: i });
        }

        cleaned.push(bytes[i]);
        positions.push(i);
        i += 1;
    }

    let mut at_positions = Vec::new();
    for (idx, &b) in cleaned.iter().enumerate() {
        if b == b'@' {
            at_positions.push(idx);
        }
    }
    for run in runs {
        let left_at = run.start > 0
            && positions.iter().any(|&p| p + 1 == run.start)
            && cleaned
                .get(positions.iter().position(|&p| p + 1 == run.start).unwrap_or(usize::MAX))
                == Some(&b'@');
        let right_at = run.end < bytes.len()
            && positions.iter().any(|&p| p == run.end)
            && cleaned
                .get(positions.iter().position(|&p| p == run.end).unwrap_or(usize::MAX))
                == Some(&b'@');
        let at_begin = !positions.iter().any(|&p| p < run.start);
        let at_end = !positions.iter().any(|&p| p >= run.end);
        if !(at_begin || at_end || left_at || right_at) {
            return Err(EmailError::CfwsBoundary { at: run.start });
        }
    }

    if cleaned.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let mut quoted_end = None;
    if cleaned.first() == Some(&b'"') {
        let mut j = 1usize;
        while j < cleaned.len() {
            if cleaned[j] == b'\\' {
                j += 2;
            } else if cleaned[j] == b'"' {
                quoted_end = Some(j);
                break;
            } else {
                j += 1;
            }
        }
        if quoted_end.is_none() {
            return Err(EmailError::UnclosedQuote {
                at: positions[0],
            });
        }
    }

    let mut separators = Vec::new();
    let mut idx = 0usize;
    while idx < cleaned.len() {
        if let Some(end) = quoted_end {
            if idx <= end {
                idx = end + 1;
                continue;
            }
        }
        if cleaned[idx] == b'@' {
            separators.push(idx);
        }
        idx += 1;
    }

    if separators.is_empty() {
        return Err(EmailError::MissingAt);
    }
    if separators.len() > 1 {
        return Err(EmailError::MultipleAt {
            at: positions[separators[1]],
        });
    }

    let sep = separators[0];
    if sep == 0 {
        return Err(EmailError::EmptyLocal {
            at: positions[sep],
        });
    }

    let mut local = String::new();
    let quoted = cleaned[0] == b'"';
    if !quoted {
        let local_bytes = &cleaned[..sep];
        if local_bytes.len() > 64 {
            return Err(EmailError::LocalTooLong {
                at: positions[64],
            });
        }
        for j in 0..local_bytes.len() {
            let b = local_bytes[j];
            if b == b'.' {
                if j == 0 || j + 1 == local_bytes.len() || local_bytes[j - 1] == b'.' {
                    return Err(EmailError::LocalDot { at: positions[j] });
                }
            } else if !(b.is_ascii_alphanumeric()
                || b"!#$%&'*+-/=?^_`{|}~".contains(&b))
            {
                return Err(EmailError::LocalChar { at: positions[j] });
            }
            local.push(b as char);
        }
    } else {
        let end = quoted_end.unwrap();
        if end + 1 != sep {
            let bad = end + 1;
            return Err(EmailError::TrailingLocal {
                at: positions.get(bad).copied().unwrap_or(input.len()),
            });
        }
        let mut decoded = Vec::<(u8, usize)>::new();
        let mut j = 1usize;
        while j < end {
            let b = cleaned[j];
            if b == b'\\' {
                let origin = positions[j];
                if j + 1 >= end {
                    return Err(EmailError::BadEscape { at: origin });
                }
                let escaped = cleaned[j + 1];
                if escaped != b'"' && escaped != b'\\' && escaped != b'@' {
                    return Err(EmailError::BadEscape { at: origin });
                }
                decoded.push((escaped, origin));
                j += 2;
            } else {
                if !is_printable(b) {
                    return Err(EmailError::QuotedChar { at: positions[j] });
                }
                decoded.push((b, positions[j]));
                j += 1;
            }
        }
        if decoded.is_empty() {
            return Err(EmailError::EmptyLocal {
                at: positions[end],
            });
        }
        if decoded.len() > 64 {
            return Err(EmailError::LocalTooLong {
                at: decoded[64].1,
            });
        }
        for (b, _) in decoded {
            local.push(b as char);
        }
    }

    let domain_start_idx = sep + 1;
    let domain_start = positions.get(domain_start_idx).copied().unwrap_or(input.len());
    if domain_start_idx >= cleaned.len() {
        return Err(EmailError::EmptyDomain { at: domain_start });
    }

    let mut domain_end = cleaned.len();
    let mut root_dot = None;
    if domain_end > domain_start_idx && cleaned[domain_end - 1] == b'.' {
        root_dot = Some(positions[domain_end - 1]);
        domain_end -= 1;
    }
    if domain_end == domain_start_idx {
        return Err(EmailError::EmptyDomain { at: domain_start });
    }

    let domain_len = domain_end - domain_start_idx;
    if domain_len > 253 {
        return Err(EmailError::DomainTooLong {
            at: positions[domain_start_idx + 253],
        });
    }

    let domain_bytes = &cleaned[domain_start_idx..domain_end];
    let mut labels: Vec<(usize, usize)> = Vec::new();
    let mut start = 0usize;
    for j in 0..=domain_bytes.len() {
        if j == domain_bytes.len() || domain_bytes[j] == b'.' {
            labels.push((start, j));
            start = j + 1;
        }
    }

    for (n, &(a, b)) in labels.iter().enumerate() {
        if a == b {
            let at = if a < domain_bytes.len() {
                positions[domain_start_idx + a]
            } else if n > 0 && b > 0 {
                positions[domain_start_idx + b - 1]
            } else {
                root_dot.unwrap_or(domain_start)
            };
            return Err(EmailError::EmptyLabel { at });
        }
        if b - a > 63 {
            return Err(EmailError::LabelTooLong {
                at: positions[domain_start_idx + a + 63],
            });
        }
        if domain_bytes[a] == b'-' {
            return Err(EmailError::LabelHyphen {
                at: positions[domain_start_idx + a],
            });
        }
        if domain_bytes[b - 1] == b'-' {
            return Err(EmailError::LabelHyphen {
                at: positions[domain_start_idx + b - 1],
            });
        }
        for j in a..b {
            let ch = domain_bytes[j];
            if !(ch.is_ascii_alphanumeric() || ch == b'-') {
                return Err(EmailError::DomainChar {
                    at: positions[domain_start_idx + j],
                });
            }
        }
    }

    if labels.len() < 2 {
        return Err(EmailError::DomainNeedsDot { at: domain_start });
    }

    let (last_start, last_end) = *labels.last().unwrap();
    if domain_bytes[last_start..last_end]
        .iter()
        .all(|b| b.is_ascii_digit())
    {
        return Err(EmailError::NumericTld {
            at: positions[domain_start_idx + last_start],
        });
    }

    let domain = domain_bytes
        .iter()
        .map(|b| b.to_ascii_lowercase() as char)
        .collect::<String>();

    Ok(Address {
        local,
        domain,
        quoted,
    })
}