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
struct Byte {
    value: u8,
    pos: usize,
}

#[derive(Clone)]
struct Run {
    start: usize,
    end: usize,
    original: usize,
}

pub fn parse_email(input: &str) -> Result<Address, EmailError> {
    let bytes = input.as_bytes();

    if bytes.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    if let Some(at) = bytes.iter().position(|&b| b >= 128) {
        return Err(EmailError::NonAscii { at });
    }

    let mut cleaned: Vec<Byte> = Vec::with_capacity(bytes.len());
    let mut runs: Vec<Run> = Vec::new();
    let mut i = 0;
    let mut quote_protected = false;
    let mut quote_closed = false;

    while i < bytes.len() {
        let b = bytes[i];

        if quote_protected && !quote_closed {
            cleaned.push(Byte { value: b, pos: i });
            if b == b'\\' {
                if i + 1 < bytes.len() {
                    i += 1;
                    cleaned.push(Byte {
                        value: bytes[i],
                        pos: i,
                    });
                }
            } else if b == b'"' {
                quote_closed = true;
            }
            i += 1;
            continue;
        }

        let is_cfws = b == b' ' || b == b'\t' || b == b'(' || b == b'\r' || b == b'\n';
        if is_cfws {
            let run_start = i;
            let cleaned_start = cleaned.len();

            loop {
                if i >= bytes.len() {
                    break;
                }

                match bytes[i] {
                    b' ' | b'\t' => i += 1,
                    b'\r' | b'\n' => {
                        let fold_ok = bytes[i] == b'\r'
                            && i + 2 < bytes.len()
                            && bytes[i + 1] == b'\n'
                            && (bytes[i + 2] == b' ' || bytes[i + 2] == b'\t');
                        if !fold_ok {
                            return Err(EmailError::BadFold { at: i });
                        }
                        i += 2;
                        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
                            i += 1;
                        }
                    }
                    b'(' => {
                        let outer = i;
                        let mut depth = 0usize;
                        while i < bytes.len() {
                            match bytes[i] {
                                b'(' => {
                                    depth += 1;
                                    if depth > 3 {
                                        return Err(EmailError::CommentDepth { at: i });
                                    }
                                    i += 1;
                                }
                                b')' => {
                                    depth -= 1;
                                    i += 1;
                                    if depth == 0 {
                                        break;
                                    }
                                }
                                b'\\' => {
                                    let slash = i;
                                    if i + 1 >= bytes.len()
                                        || !(0x20..=0x7e).contains(&bytes[i + 1])
                                    {
                                        return Err(EmailError::CommentEscape { at: slash });
                                    }
                                    i += 2;
                                }
                                b'\r' | b'\n' => {
                                    let fold_ok = bytes[i] == b'\r'
                                        && i + 2 < bytes.len()
                                        && bytes[i + 1] == b'\n'
                                        && (bytes[i + 2] == b' ' || bytes[i + 2] == b'\t');
                                    if !fold_ok {
                                        return Err(EmailError::BadFold { at: i });
                                    }
                                    i += 2;
                                    while i < bytes.len()
                                        && (bytes[i] == b' ' || bytes[i] == b'\t')
                                    {
                                        i += 1;
                                    }
                                }
                                0x20..=0x7e | b'\t' => i += 1,
                                _ => return Err(EmailError::CommentChar { at: i }),
                            }
                        }
                        if depth != 0 {
                            return Err(EmailError::UnclosedComment { at: outer });
                        }
                    }
                    _ => break,
                }
            }

            runs.push(Run {
                start: cleaned_start,
                end: cleaned.len(),
                original: run_start,
            });
            continue;
        }

        if b == b')' {
            return Err(EmailError::UnexpectedClose { at: i });
        }

        if cleaned.is_empty() && b == b'"' {
            quote_protected = true;
            quote_closed = false;
        }

        cleaned.push(Byte { value: b, pos: i });
        i += 1;
    }

    for run in &runs {
        let left_at = run.start > 0 && cleaned[run.start - 1].value == b'@';
        let right_at = run.end < cleaned.len() && cleaned[run.end].value == b'@';
        if run.start != 0 && run.end != cleaned.len() && !left_at && !right_at {
            return Err(EmailError::CfwsBoundary { at: run.original });
        }
    }

    if cleaned.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let mut quote_end = None;
    if cleaned[0].value == b'"' {
        let mut j = 1;
        while j < cleaned.len() {
            if cleaned[j].value == b'\\' {
                j += 2;
            } else if cleaned[j].value == b'"' {
                quote_end = Some(j);
                break;
            } else {
                j += 1;
            }
        }
        if quote_end.is_none() {
            return Err(EmailError::UnclosedQuote { at: cleaned[0].pos });
        }
    }

    let mut ats = Vec::new();
    for (idx, byte) in cleaned.iter().enumerate() {
        if byte.value == b'@' && !quote_end.is_some_and(|end| idx <= end) {
            ats.push(idx);
        }
    }
    if ats.is_empty() {
        return Err(EmailError::MissingAt);
    }
    if ats.len() > 1 {
        return Err(EmailError::MultipleAt {
            at: cleaned[ats[1]].pos,
        });
    }

    let at_idx = ats[0];
    if at_idx == 0 {
        return Err(EmailError::EmptyLocal {
            at: cleaned[at_idx].pos,
        });
    }

    let local_bytes = &cleaned[..at_idx];
    let (local, quoted) = if local_bytes[0].value == b'"' {
        let end = quote_end.unwrap();
        if end + 1 != at_idx {
            return Err(EmailError::TrailingLocal {
                at: local_bytes[end + 1].pos,
            });
        }

        let mut decoded: Vec<(u8, usize)> = Vec::new();
        let mut j = 1;
        while j < end {
            let b = local_bytes[j].value;
            if b == b'\\' {
                if j + 1 >= end || !matches!(local_bytes[j + 1].value, b'"' | b'\\' | b'@') {
                    return Err(EmailError::BadEscape {
                        at: local_bytes[j].pos,
                    });
                }
                decoded.push((local_bytes[j + 1].value, local_bytes[j].pos));
                j += 2;
            } else {
                if !(0x20..=0x7e).contains(&b) {
                    return Err(EmailError::QuotedChar {
                        at: local_bytes[j].pos,
                    });
                }
                decoded.push((b, local_bytes[j].pos));
                j += 1;
            }
        }
        if decoded.is_empty() {
            return Err(EmailError::EmptyLocal {
                at: local_bytes[end].pos,
            });
        }
        if decoded.len() > 64 {
            return Err(EmailError::LocalTooLong { at: decoded[64].1 });
        }
        let value = decoded.into_iter().map(|(b, _)| b as char).collect();
        (value, true)
    } else {
        if local_bytes.len() > 64 {
            return Err(EmailError::LocalTooLong {
                at: local_bytes[64].pos,
            });
        }
        for (idx, byte) in local_bytes.iter().enumerate() {
            let b = byte.value;
            if b == b'.' {
                if idx == 0
                    || idx + 1 == local_bytes.len()
                    || local_bytes[idx - 1].value == b'.'
                {
                    return Err(EmailError::LocalDot { at: byte.pos });
                }
            } else if !b.is_ascii_alphanumeric() && !b"!#$%&'*+-/=?^_`{|}~".contains(&b) {
                return Err(EmailError::LocalChar { at: byte.pos });
            }
        }
        let value = local_bytes.iter().map(|b| b.value as char).collect();
        (value, false)
    };

    let domain_start_idx = at_idx + 1;
    let domain_start_pos = cleaned
        .get(domain_start_idx)
        .map(|b| b.pos)
        .unwrap_or(input.len());
    let raw_domain = &cleaned[domain_start_idx..];
    let root_dot = raw_domain.last().is_some_and(|b| b.value == b'.');
    let domain_len = raw_domain.len() - usize::from(root_dot);

    if domain_len == 0 {
        return Err(EmailError::EmptyDomain {
            at: domain_start_pos,
        });
    }
    if domain_len > 253 {
        return Err(EmailError::DomainTooLong {
            at: raw_domain[253].pos,
        });
    }

    let domain_bytes = &raw_domain[..domain_len];
    let mut labels: Vec<(usize, usize)> = Vec::new();
    let mut start = 0;
    for idx in 0..=domain_len {
        if idx == domain_len || domain_bytes[idx].value == b'.' {
            labels.push((start, idx));
            start = idx + 1;
        }
    }

    for &(start, end) in &labels {
        if start == end {
            let pos = if start < domain_len {
                domain_bytes[start].pos
            } else if root_dot {
                raw_domain[raw_domain.len() - 1].pos
            } else {
                input.len()
            };
            return Err(EmailError::EmptyLabel { at: pos });
        }
        if end - start > 63 {
            return Err(EmailError::LabelTooLong {
                at: domain_bytes[start + 63].pos,
            });
        }
        if domain_bytes[start].value == b'-' {
            return Err(EmailError::LabelHyphen {
                at: domain_bytes[start].pos,
            });
        }
        if domain_bytes[end - 1].value == b'-' {
            return Err(EmailError::LabelHyphen {
                at: domain_bytes[end - 1].pos,
            });
        }
        for byte in &domain_bytes[start..end] {
            if !byte.value.is_ascii_alphanumeric() && byte.value != b'-' {
                return Err(EmailError::DomainChar { at: byte.pos });
            }
        }
    }

    if labels.len() < 2 {
        return Err(EmailError::DomainNeedsDot {
            at: domain_start_pos,
        });
    }

    let (last_start, last_end) = *labels.last().unwrap();
    if domain_bytes[last_start..last_end]
        .iter()
        .all(|b| b.value.is_ascii_digit())
    {
        return Err(EmailError::NumericTld {
            at: domain_bytes[last_start].pos,
        });
    }

    let domain = domain_bytes
        .iter()
        .map(|b| (b.value as char).to_ascii_lowercase())
        .collect();

    Ok(Address {
        local,
        domain,
        quoted,
    })
}