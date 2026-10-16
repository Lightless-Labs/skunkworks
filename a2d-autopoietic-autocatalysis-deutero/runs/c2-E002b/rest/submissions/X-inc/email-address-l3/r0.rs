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

fn fold_end(bytes: &[u8], i: usize) -> Option<usize> {
    if bytes.get(i) != Some(&b'\r')
        || bytes.get(i + 1) != Some(&b'\n')
        || !bytes.get(i + 2).copied().is_some_and(is_hws)
    {
        return None;
    }

    let mut end = i + 2;
    while end < bytes.len() && is_hws(bytes[end]) {
        end += 1;
    }
    Some(end)
}

fn parse_email_inner(input: &str) -> Result<Address, EmailError> {
    let bytes = input.as_bytes();

    if bytes.is_empty() {
        return Err(EmailError::EmptyInput);
    }
    if let Some(at) = bytes.iter().position(|&b| b >= 128) {
        return Err(EmailError::NonAscii { at });
    }

    let mut clean = Vec::<u8>::with_capacity(bytes.len());
    let mut positions = Vec::<usize>::with_capacity(bytes.len());
    let mut runs = Vec::<CfwsRun>::new();
    let mut i = 0usize;
    let mut quote_protected = false;
    let mut quote_closed = false;

    while i < bytes.len() {
        let b = bytes[i];

        if quote_protected && !quote_closed {
            clean.push(b);
            positions.push(i);
            if b == b'\\' {
                if i + 1 < bytes.len() {
                    clean.push(bytes[i + 1]);
                    positions.push(i + 1);
                    i += 2;
                    continue;
                }
            } else if b == b'"' {
                quote_closed = true;
            }
            i += 1;
            continue;
        }

        if !quote_protected && clean.is_empty() && b == b'"' {
            quote_protected = true;
            clean.push(b);
            positions.push(i);
            i += 1;
            continue;
        }

        if is_hws(b) || b == b'(' || b == b'\r' || b == b'\n' {
            let run_start = i;
            let mut depth = 0usize;
            let mut outer_open = None;

            loop {
                if i >= bytes.len() {
                    break;
                }

                let c = bytes[i];
                if depth > 0 {
                    if c == b'\\' {
                        if i + 1 >= bytes.len() || !is_printable(bytes[i + 1]) {
                            return Err(EmailError::CommentEscape { at: i });
                        }
                        i += 2;
                        continue;
                    }
                    if c == b'(' {
                        if depth == 3 {
                            return Err(EmailError::CommentDepth { at: i });
                        }
                        depth += 1;
                        i += 1;
                        continue;
                    }
                    if c == b')' {
                        depth -= 1;
                        i += 1;
                        continue;
                    }
                    if c == b'\r' || c == b'\n' {
                        if let Some(end) = fold_end(bytes, i) {
                            i = end;
                            continue;
                        }
                        return Err(EmailError::BadFold { at: i });
                    }
                    if !(is_printable(c) || c == b'\t') {
                        return Err(EmailError::CommentChar { at: i });
                    }
                    i += 1;
                    continue;
                }

                if is_hws(c) {
                    i += 1;
                    continue;
                }
                if c == b'\r' || c == b'\n' {
                    if let Some(end) = fold_end(bytes, i) {
                        i = end;
                        continue;
                    }
                    return Err(EmailError::BadFold { at: i });
                }
                if c == b'(' {
                    outer_open = Some(i);
                    depth = 1;
                    i += 1;
                    continue;
                }
                break;
            }

            if depth != 0 {
                return Err(EmailError::UnclosedComment {
                    at: outer_open.unwrap_or(run_start),
                });
            }

            runs.push(CfwsRun {
                start: run_start,
                end: i,
            });
            continue;
        }

        if b == b')' {
            return Err(EmailError::UnexpectedClose { at: i });
        }

        clean.push(b);
        positions.push(i);
        i += 1;
    }

    let offset = |idx: usize| positions.get(idx).copied().unwrap_or(bytes.len());

    let mut quote_close = None;
    if clean.first() == Some(&b'"') {
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
    }

    for run in runs {
        let left_boundary = run.start == 0
            || clean.iter().enumerate().any(|(idx, &b)| {
                b == b'@' && positions[idx] + 1 == run.start
            });
        let right_boundary = run.end == bytes.len()
            || clean.iter().enumerate().any(|(idx, &b)| {
                b == b'@' && positions[idx] == run.end
            });

        if !left_boundary && !right_boundary {
            return Err(EmailError::CfwsBoundary { at: run.start });
        }
    }

    if clean.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let mut protected_end = None;
    if clean.first() == Some(&b'"') {
        let mut j = 1usize;
        while j < clean.len() {
            if clean[j] == b'\\' {
                if j + 1 < clean.len() {
                    j += 2;
                } else {
                    break;
                }
            } else if clean[j] == b'"' {
                protected_end = Some(j);
                break;
            } else {
                j += 1;
            }
        }

        if protected_end.is_none() {
            return Err(EmailError::UnclosedQuote { at: offset(0) });
        }
    }

    let mut separators = Vec::new();
    for (idx, &b) in clean.iter().enumerate() {
        if b == b'@' && !protected_end.is_some_and(|end| idx <= end) {
            separators.push(idx);
        }
    }

    if separators.is_empty() {
        return Err(EmailError::MissingAt);
    }
    if separators.len() > 1 {
        return Err(EmailError::MultipleAt {
            at: offset(separators[1]),
        });
    }

    let sep = separators[0];
    if sep == 0 {
        return Err(EmailError::EmptyLocal { at: offset(sep) });
    }

    let local_bytes = &clean[..sep];
    let local_positions = &positions[..sep];

    let (local, quoted) = if local_bytes.first() == Some(&b'"') {
        let close = protected_end.expect("closed quoted prefix");
        if close + 1 != sep {
            return Err(EmailError::TrailingLocal {
                at: offset(close + 1),
            });
        }

        let mut decoded = Vec::new();
        let mut origins = Vec::new();
        let mut j = 1usize;
        while j < close {
            let b = local_bytes[j];
            if b == b'\\' {
                if j + 1 >= close {
                    return Err(EmailError::BadEscape {
                        at: local_positions[j],
                    });
                }
                let next = local_bytes[j + 1];
                if next != b'"' && next != b'\\' && next != b'@' {
                    return Err(EmailError::BadEscape {
                        at: local_positions[j],
                    });
                }
                decoded.push(next);
                origins.push(local_positions[j]);
                j += 2;
            } else {
                if !is_printable(b) {
                    return Err(EmailError::QuotedChar {
                        at: local_positions[j],
                    });
                }
                decoded.push(b);
                origins.push(local_positions[j]);
                j += 1;
            }
        }

        if decoded.is_empty() {
            return Err(EmailError::EmptyLocal {
                at: local_positions[close],
            });
        }
        if decoded.len() > 64 {
            return Err(EmailError::LocalTooLong { at: origins[64] });
        }

        (String::from_utf8(decoded).expect("ASCII local"), true)
    } else {
        if local_bytes.len() > 64 {
            return Err(EmailError::LocalTooLong {
                at: local_positions[64],
            });
        }

        let mut previous_dot = false;
        for (idx, &b) in local_bytes.iter().enumerate() {
            if b == b'.' {
                if idx == 0 || idx + 1 == local_bytes.len() || previous_dot {
                    return Err(EmailError::LocalDot {
                        at: local_positions[idx],
                    });
                }
                previous_dot = true;
                continue;
            }

            previous_dot = false;
            if !b.is_ascii_alphanumeric() && !b"!#$%&'*+-/=?^_`{|}~".contains(&b) {
                return Err(EmailError::LocalChar {
                    at: local_positions[idx],
                });
            }
        }

        (String::from_utf8(local_bytes.to_vec()).expect("ASCII local"), false)
    };

    let domain_start = sep + 1;
    let mut domain_end = clean.len();
    let root_dot = if clean.last() == Some(&b'.') {
        domain_end -= 1;
        Some(offset(domain_end))
    } else {
        None
    };

    if domain_end == domain_start {
        return Err(EmailError::EmptyDomain {
            at: offset(domain_start),
        });
    }

    if domain_end - domain_start > 253 {
        return Err(EmailError::DomainTooLong {
            at: offset(domain_start + 253),
        });
    }

    let mut labels = Vec::<(usize, usize)>::new();
    let mut label_start = domain_start;
    for idx in domain_start..=domain_end {
        if idx == domain_end || clean[idx] == b'.' {
            if label_start == idx {
                let at = if idx == domain_end {
                    root_dot.unwrap_or_else(|| offset(idx))
                } else {
                    offset(idx)
                };
                return Err(EmailError::EmptyLabel { at });
            }
            labels.push((label_start, idx));
            label_start = idx + 1;
        }
    }

    for &(start, end) in &labels {
        if end - start > 63 {
            return Err(EmailError::LabelTooLong {
                at: offset(start + 63),
            });
        }
        if clean[start] == b'-' {
            return Err(EmailError::LabelHyphen { at: offset(start) });
        }
        if clean[end - 1] == b'-' {
            return Err(EmailError::LabelHyphen { at: offset(end - 1) });
        }
        for idx in start..end {
            let b = clean[idx];
            if !b.is_ascii_alphanumeric() && b != b'-' {
                return Err(EmailError::DomainChar { at: offset(idx) });
            }
        }
    }

    if labels.len() < 2 {
        return Err(EmailError::DomainNeedsDot {
            at: offset(domain_start),
        });
    }

    let (last_start, last_end) = *labels.last().expect("at least one label");
    if clean[last_start..last_end].iter().all(u8::is_ascii_digit) {
        return Err(EmailError::NumericTld {
            at: offset(last_start),
        });
    }

    let domain = clean[domain_start..domain_end]
        .iter()
        .map(|&b| b.to_ascii_lowercase() as char)
        .collect();

    Ok(Address {
        local,
        domain,
        quoted,
    })
}

pub fn parse_email(input: &str) -> Result<Address, EmailError> {
    parse_email_inner(input)
}