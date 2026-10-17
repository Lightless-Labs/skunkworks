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

fn fold_end(bytes: &[u8], at: usize) -> Option<usize> {
    if bytes.get(at) != Some(&b'\r') || bytes.get(at + 1) != Some(&b'\n') {
        return None;
    }
    let mut end = at + 2;
    if !bytes.get(end).copied().is_some_and(horizontal) {
        return None;
    }
    while bytes.get(end).copied().is_some_and(horizontal) {
        end += 1;
    }
    Some(end)
}

fn printable(b: u8) -> bool {
    (0x20..=0x7e).contains(&b)
}

pub fn parse_email(input: &str) -> Result<Address, EmailError> {
    if input.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let bytes = input.as_bytes();
    if let Some(at) = bytes.iter().position(|&b| b >= 128) {
        return Err(EmailError::NonAscii { at });
    }

    let mut clean = Vec::<u8>::with_capacity(bytes.len());
    let mut positions = Vec::<usize>::with_capacity(bytes.len());
    let mut runs = Vec::<Run>::new();
    let mut i = 0usize;
    let mut protected_quote = false;
    let mut quote_closed = false;

    while i < bytes.len() {
        let b = bytes[i];

        if protected_quote {
            clean.push(b);
            positions.push(i);
            if b == b'\\' {
                if i + 1 < bytes.len() {
                    clean.push(bytes[i + 1]);
                    positions.push(i + 1);
                    i += 2;
                } else {
                    i += 1;
                }
            } else {
                if b == b'"' {
                    protected_quote = false;
                    quote_closed = true;
                }
                i += 1;
            }
            continue;
        }

        if !quote_closed && clean.is_empty() && b == b'"' {
            protected_quote = true;
            clean.push(b);
            positions.push(i);
            i += 1;
            continue;
        }

        if b == b' ' || b == b'\t' || b == b'\r' || b == b'\n' || b == b'(' {
            let run_start = i;
            let mut j = i;
            let mut depth = 0usize;
            let mut outer_open = None;

            loop {
                if j >= bytes.len() {
                    if depth != 0 {
                        return Err(EmailError::UnclosedComment {
                            at: outer_open.unwrap_or(run_start),
                        });
                    }
                    break;
                }

                let c = bytes[j];
                if depth > 0 {
                    match c {
                        b'\\' => {
                            if j + 1 >= bytes.len() || !printable(bytes[j + 1]) {
                                return Err(EmailError::CommentEscape { at: j });
                            }
                            j += 2;
                        }
                        b'(' => {
                            if depth == 3 {
                                return Err(EmailError::CommentDepth { at: j });
                            }
                            depth += 1;
                            j += 1;
                        }
                        b')' => {
                            depth -= 1;
                            j += 1;
                        }
                        b'\r' | b'\n' => {
                            if c == b'\r' {
                                if let Some(end) = fold_end(bytes, j) {
                                    j = end;
                                } else {
                                    return Err(EmailError::BadFold { at: j });
                                }
                            } else {
                                return Err(EmailError::BadFold { at: j });
                            }
                        }
                        _ if printable(c) || c == b'\t' => j += 1,
                        _ => return Err(EmailError::CommentChar { at: j }),
                    }
                    continue;
                }

                match c {
                    b' ' | b'\t' => j += 1,
                    b'\r' | b'\n' => {
                        if c == b'\r' {
                            if let Some(end) = fold_end(bytes, j) {
                                j = end;
                            } else {
                                return Err(EmailError::BadFold { at: j });
                            }
                        } else {
                            return Err(EmailError::BadFold { at: j });
                        }
                    }
                    b'(' => {
                        depth = 1;
                        outer_open = Some(j);
                        j += 1;
                    }
                    _ => break,
                }
            }

            if j == run_start {
                unreachable!();
            }
            runs.push(Run {
                start: run_start,
                end: j,
            });
            i = j;
            continue;
        }

        if b == b')' {
            return Err(EmailError::UnexpectedClose { at: i });
        }

        clean.push(b);
        positions.push(i);
        i += 1;
    }

    for run in runs {
        let retained_before = positions.iter().rposition(|&p| p < run.start);
        let retained_after = positions.iter().position(|&p| p >= run.end);
        let left_is_at = retained_before.is_some_and(|k| clean[k] == b'@');
        let right_is_at = retained_after.is_some_and(|k| clean[k] == b'@');
        let at_boundary = retained_before.is_none() || retained_after.is_none();
        if !(at_boundary || left_is_at || right_is_at) {
            return Err(EmailError::CfwsBoundary { at: run.start });
        }
    }

    if clean.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let n = clean.len();
    let mut quote_end = None;
    if clean[0] == b'"' {
        let mut j = 1usize;
        while j < n {
            if clean[j] == b'\\' {
                j += 2;
            } else if clean[j] == b'"' {
                quote_end = Some(j);
                break;
            } else {
                j += 1;
            }
        }
        if quote_end.is_none() {
            return Err(EmailError::UnclosedQuote {
                at: positions[0],
            });
        }
    }

    let mut separators = Vec::new();
    let protected_end = quote_end.unwrap_or(0);
    for (idx, &b) in clean.iter().enumerate() {
        if b == b'@' && (quote_end.is_none() || idx > protected_end) {
            separators.push(idx);
        }
    }
    if separators.is_empty() {
        return Err(EmailError::MissingAt);
    }
    if separators.len() >= 2 {
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
    let quoted = clean[0] == b'"';
    if !quoted {
        if sep > 64 {
            return Err(EmailError::LocalTooLong {
                at: positions[64],
            });
        }
        for idx in 0..sep {
            let b = clean[idx];
            if b == b'.' {
                if idx == 0
                    || idx + 1 == sep
                    || clean[idx - 1] == b'.'
                {
                    return Err(EmailError::LocalDot {
                        at: positions[idx],
                    });
                }
            } else if !(b.is_ascii_alphanumeric()
                || b"!#$%&'*+-/=?^_`{|}~".contains(&b))
            {
                return Err(EmailError::LocalChar {
                    at: positions[idx],
                });
            }
            local.push(b as char);
        }
    } else {
        let close = quote_end.unwrap();
        if close + 1 != sep {
            return Err(EmailError::TrailingLocal {
                at: positions[close + 1],
            });
        }

        let mut decoded = Vec::<u8>::new();
        let mut origins = Vec::<usize>::new();
        let mut idx = 1usize;
        while idx < close {
            let b = clean[idx];
            if b == b'\\' {
                if idx + 1 >= close {
                    return Err(EmailError::BadEscape {
                        at: positions[idx],
                    });
                }
                let escaped = clean[idx + 1];
                if !matches!(escaped, b'"' | b'\\' | b'@') {
                    return Err(EmailError::BadEscape {
                        at: positions[idx],
                    });
                }
                decoded.push(escaped);
                origins.push(positions[idx]);
                idx += 2;
            } else {
                if !printable(b) {
                    return Err(EmailError::QuotedChar {
                        at: positions[idx],
                    });
                }
                decoded.push(b);
                origins.push(positions[idx]);
                idx += 1;
            }
        }
        if decoded.is_empty() {
            return Err(EmailError::EmptyLocal {
                at: positions[close],
            });
        }
        if decoded.len() > 64 {
            return Err(EmailError::LocalTooLong { at: origins[64] });
        }
        local = String::from_utf8(decoded).unwrap();
    }

    let domain_start = sep + 1;
    let raw_domain = &clean[domain_start..];
    let mut domain_end = n;
    let root_dot = if raw_domain.last() == Some(&b'.') {
        domain_end -= 1;
        Some(positions[domain_end])
    } else {
        None
    };

    if domain_start == domain_end {
        return Err(EmailError::EmptyDomain {
            at: if domain_start < n {
                positions[domain_start]
            } else {
                input.len()
            },
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
    let mut idx = domain_start;
    while idx <= domain_end {
        if idx == domain_end || clean[idx] == b'.' {
            if label_start == idx {
                let at = if idx == domain_end {
                    root_dot.unwrap_or_else(|| {
                        if idx < n {
                            positions[idx]
                        } else {
                            input.len()
                        }
                    })
                } else {
                    positions[idx]
                };
                return Err(EmailError::EmptyLabel { at });
            }

            let label_len = idx - label_start;
            if label_len > 63 {
                return Err(EmailError::LabelTooLong {
                    at: positions[label_start + 63],
                });
            }
            if clean[label_start] == b'-' {
                return Err(EmailError::LabelHyphen {
                    at: positions[label_start],
                });
            }
            if clean[idx - 1] == b'-' {
                return Err(EmailError::LabelHyphen {
                    at: positions[idx - 1],
                });
            }
            for p in label_start..idx {
                let b = clean[p];
                if !(b.is_ascii_alphanumeric() || b == b'-') {
                    return Err(EmailError::DomainChar { at: positions[p] });
                }
            }
            labels.push((label_start, idx));
            label_start = idx + 1;
        }
        idx += 1;
    }

    if labels.len() < 2 {
        return Err(EmailError::DomainNeedsDot {
            at: positions[domain_start],
        });
    }

    let (last_start, last_end) = *labels.last().unwrap();
    if clean[last_start..last_end].iter().all(u8::is_ascii_digit) {
        return Err(EmailError::NumericTld {
            at: positions[last_start],
        });
    }

    let domain = clean[domain_start..domain_end]
        .iter()
        .map(|b| b.to_ascii_lowercase() as char)
        .collect();

    Ok(Address {
        local,
        domain,
        quoted,
    })
}