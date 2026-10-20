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
struct Retained {
    byte: u8,
    pos: usize,
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

fn valid_fold(bytes: &[u8], i: usize) -> Option<usize> {
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

fn push_retained(out: &mut Vec<Retained>, b: u8, pos: usize) {
    out.push(Retained { byte: b, pos });
}

pub fn parse_email(input: &str) -> Result<Address, EmailError> {
    let bytes = input.as_bytes();

    if bytes.is_empty() {
        return Err(EmailError::EmptyInput);
    }
    if let Some(at) = bytes.iter().position(|&b| b >= 128) {
        return Err(EmailError::NonAscii { at });
    }

    let mut retained: Vec<Retained> = Vec::with_capacity(bytes.len());
    let mut runs: Vec<CfwsRun> = Vec::new();
    let mut i = 0usize;
    let mut protected_quote = false;
    let mut quote_closed = false;

    while i < bytes.len() {
        let b = bytes[i];

        if protected_quote {
            push_retained(&mut retained, b, i);
            if b == b'\\' {
                if i + 1 < bytes.len() {
                    i += 1;
                    push_retained(&mut retained, bytes[i], i);
                }
            } else if b == b'"' {
                quote_closed = true;
                protected_quote = false;
            }
            i += 1;
            continue;
        }

        if retained.is_empty() && b == b'"' {
            protected_quote = true;
            quote_closed = false;
            push_retained(&mut retained, b, i);
            i += 1;
            continue;
        }

        let is_cfws_start = is_hws(b)
            || b == b'('
            || b == b'\r'
            || b == b'\n';

        if is_cfws_start {
            let run_start = i;
            let retained_start = retained.len();

            loop {
                if i >= bytes.len() {
                    break;
                }
                let c = bytes[i];

                if is_hws(c) {
                    i += 1;
                    continue;
                }
                if c == b'\r' || c == b'\n' {
                    if let Some(next) = valid_fold(bytes, i) {
                        i = next;
                        continue;
                    }
                    return Err(EmailError::BadFold { at: i });
                }
                if c == b'(' {
                    let outer = i;
                    let mut depth = 0usize;
                    loop {
                        if i >= bytes.len() {
                            return Err(EmailError::UnclosedComment { at: outer });
                        }
                        let d = bytes[i];
                        if d == b'\\' {
                            if i + 1 >= bytes.len() || !is_printable(bytes[i + 1]) {
                                return Err(EmailError::CommentEscape { at: i });
                            }
                            i += 2;
                            continue;
                        }
                        if d == b'(' {
                            depth += 1;
                            if depth > 3 {
                                return Err(EmailError::CommentDepth { at: i });
                            }
                            i += 1;
                            continue;
                        }
                        if d == b')' {
                            depth -= 1;
                            i += 1;
                            if depth == 0 {
                                break;
                            }
                            continue;
                        }
                        if d == b'\r' || d == b'\n' {
                            if let Some(next) = valid_fold(bytes, i) {
                                i = next;
                                continue;
                            }
                            return Err(EmailError::BadFold { at: i });
                        }
                        if !(is_printable(d) || d == b'\t') {
                            return Err(EmailError::CommentChar { at: i });
                        }
                        i += 1;
                    }
                    continue;
                }
                break;
            }

            if i > run_start {
                debug_assert_eq!(retained_start, retained.len());
                runs.push(CfwsRun {
                    start: run_start,
                    end: i,
                });
            }
            continue;
        }

        if b == b')' {
            return Err(EmailError::UnexpectedClose { at: i });
        }

        push_retained(&mut retained, b, i);
        i += 1;
    }

    let _ = quote_closed;

    for run in &runs {
        let left_is_at = retained
            .iter()
            .find(|r| r.pos >= run.end)
            .is_some_and(|r| r.byte == b'@');
        let right_is_at = retained
            .iter()
            .rev()
            .find(|r| r.pos < run.start)
            .is_some_and(|r| r.byte == b'@');
        let at_beginning = retained.iter().all(|r| r.pos >= run.end);
        let at_end = retained.iter().all(|r| r.pos < run.start);

        if !(at_beginning || at_end || left_is_at || right_is_at) {
            return Err(EmailError::CfwsBoundary { at: run.start });
        }
    }

    if retained.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let cleaned: Vec<u8> = retained.iter().map(|r| r.byte).collect();
    let positions: Vec<usize> = retained.iter().map(|r| r.pos).collect();

    let mut quote_end = None;
    if cleaned.first() == Some(&b'"') {
        let mut j = 1usize;
        while j < cleaned.len() {
            if cleaned[j] == b'\\' {
                j = j.saturating_add(2);
            } else if cleaned[j] == b'"' {
                quote_end = Some(j);
                break;
            } else {
                j += 1;
            }
        }
        if quote_end.is_none() {
            return Err(EmailError::UnclosedQuote { at: positions[0] });
        }
    }

    let mut separators = Vec::new();
    let mut j = 0usize;
    while j < cleaned.len() {
        if let Some(end) = quote_end {
            if j <= end {
                j = end + 1;
                continue;
            }
        }
        if cleaned[j] == b'@' {
            separators.push(j);
        }
        j += 1;
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

    let local_bytes = &cleaned[..sep];
    let local_positions = &positions[..sep];
    let (local, quoted) = if local_bytes[0] == b'"' {
        let end = quote_end.unwrap();
        if end + 1 != sep {
            return Err(EmailError::TrailingLocal {
                at: positions[end + 1],
            });
        }

        let mut decoded = Vec::new();
        let mut origins = Vec::new();
        let mut k = 1usize;
        while k < end {
            let b = local_bytes[k];
            if b == b'\\' {
                if k + 1 >= end
                    || !matches!(local_bytes[k + 1], b'"' | b'\\' | b'@')
                {
                    return Err(EmailError::BadEscape {
                        at: local_positions[k],
                    });
                }
                decoded.push(local_bytes[k + 1]);
                origins.push(local_positions[k]);
                k += 2;
            } else {
                if !is_printable(b) {
                    return Err(EmailError::QuotedChar {
                        at: local_positions[k],
                    });
                }
                decoded.push(b);
                origins.push(local_positions[k]);
                k += 1;
            }
        }
        if decoded.is_empty() {
            return Err(EmailError::EmptyLocal {
                at: local_positions[end],
            });
        }
        if decoded.len() > 64 {
            return Err(EmailError::LocalTooLong { at: origins[64] });
        }
        (String::from_utf8(decoded).unwrap(), true)
    } else {
        if local_bytes.len() > 64 {
            return Err(EmailError::LocalTooLong {
                at: local_positions[64],
            });
        }
        for (k, &b) in local_bytes.iter().enumerate() {
            if b == b'.' {
                if k == 0
                    || k + 1 == local_bytes.len()
                    || local_bytes[k - 1] == b'.'
                {
                    return Err(EmailError::LocalDot {
                        at: local_positions[k],
                    });
                }
            } else if !b.is_ascii_alphanumeric()
                && !b"!#$%&'*+-/=?^_`{|}~".contains(&b)
            {
                return Err(EmailError::LocalChar {
                    at: local_positions[k],
                });
            }
        }
        (String::from_utf8(local_bytes.to_vec()).unwrap(), false)
    };

    let domain_start_idx = sep + 1;
    let domain_start = positions
        .get(domain_start_idx)
        .copied()
        .unwrap_or(input.len());
    let mut domain_end_idx = cleaned.len();
    let mut removed_dot_pos = None;
    if domain_end_idx > domain_start_idx && cleaned[domain_end_idx - 1] == b'.' {
        removed_dot_pos = Some(positions[domain_end_idx - 1]);
        domain_end_idx -= 1;
    }

    if domain_end_idx == domain_start_idx {
        return Err(EmailError::EmptyDomain { at: domain_start });
    }

    let domain_len = domain_end_idx - domain_start_idx;
    if domain_len > 253 {
        return Err(EmailError::DomainTooLong {
            at: positions[domain_start_idx + 253],
        });
    }

    let mut label_ranges = Vec::new();
    let mut label_start = domain_start_idx;
    for k in domain_start_idx..=domain_end_idx {
        if k == domain_end_idx || cleaned[k] == b'.' {
            let label_end = k;
            if label_start == label_end {
                let at = if label_start == domain_end_idx {
                    removed_dot_pos.unwrap_or(domain_start)
                } else {
                    positions[label_start]
                };
                return Err(EmailError::EmptyLabel { at });
            }

            let len = label_end - label_start;
            if len > 63 {
                return Err(EmailError::LabelTooLong {
                    at: positions[label_start + 63],
                });
            }
            if cleaned[label_start] == b'-' {
                return Err(EmailError::LabelHyphen {
                    at: positions[label_start],
                });
            }
            if cleaned[label_end - 1] == b'-' {
                return Err(EmailError::LabelHyphen {
                    at: positions[label_end - 1],
                });
            }
            for p in label_start..label_end {
                let b = cleaned[p];
                if !b.is_ascii_alphanumeric() && b != b'-' {
                    return Err(EmailError::DomainChar { at: positions[p] });
                }
            }
            label_ranges.push((label_start, label_end));
            label_start = k + 1;
        }
    }

    if label_ranges.len() < 2 {
        return Err(EmailError::DomainNeedsDot { at: domain_start });
    }

    let (last_start, last_end) = *label_ranges.last().unwrap();
    if cleaned[last_start..last_end]
        .iter()
        .all(u8::is_ascii_digit)
    {
        return Err(EmailError::NumericTld {
            at: positions[last_start],
        });
    }

    let domain = cleaned[domain_start_idx..domain_end_idx]
        .iter()
        .map(|b| b.to_ascii_lowercase() as char)
        .collect::<String>();

    Ok(Address {
        local,
        domain,
        quoted,
    })
}