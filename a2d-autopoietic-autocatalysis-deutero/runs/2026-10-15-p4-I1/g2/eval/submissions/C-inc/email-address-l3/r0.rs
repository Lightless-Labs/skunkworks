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

fn parse_email(input: &str) -> Result<Address, EmailError> {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return Err(EmailError::EmptyInput);
    }
    if let Some(at) = bytes.iter().position(|&b| b >= 128) {
        return Err(EmailError::NonAscii { at });
    }

    let mut cleaned = Vec::<u8>::new();
    let mut positions = Vec::<usize>::new();
    let mut runs = Vec::<Run>::new();
    let mut i = 0usize;
    let mut quote_protected = false;

    while i < bytes.len() {
        if quote_protected {
            let b = bytes[i];
            cleaned.push(b);
            positions.push(i);
            i += 1;
            if b == b'\\' && i < bytes.len() {
                cleaned.push(bytes[i]);
                positions.push(i);
                i += 1;
            } else if b == b'"' {
                quote_protected = false;
            }
            continue;
        }

        if cleaned.is_empty() && bytes[i] == b'"' {
            quote_protected = true;
            cleaned.push(bytes[i]);
            positions.push(i);
            i += 1;
            continue;
        }

        let run_start = i;
        let mut is_cfws = false;

        if bytes[i] == b' ' || bytes[i] == b'\t' || bytes[i] == b'(' {
            is_cfws = true;
            loop {
                let before = i;
                while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
                    i += 1;
                }
                if i < bytes.len() && bytes[i] == b'\r' {
                    if i + 2 < bytes.len()
                        && bytes[i + 1] == b'\n'
                        && (bytes[i + 2] == b' ' || bytes[i + 2] == b'\t')
                    {
                        i += 2;
                        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
                            i += 1;
                        }
                    } else {
                        return Err(EmailError::BadFold { at: i });
                    }
                } else if i < bytes.len() && bytes[i] == b'\n' {
                    return Err(EmailError::BadFold { at: i });
                }

                if i < bytes.len() && bytes[i] == b'(' {
                    let outer = i;
                    let mut depth = 0usize;
                    while i < bytes.len() {
                        let b = bytes[i];
                        if b == b'\\' {
                            if i + 1 >= bytes.len()
                                || !(0x20..=0x7e).contains(&bytes[i + 1])
                            {
                                return Err(EmailError::CommentEscape { at: i });
                            }
                            i += 2;
                            continue;
                        }
                        if b == b'\r' {
                            if i + 2 < bytes.len()
                                && bytes[i + 1] == b'\n'
                                && (bytes[i + 2] == b' ' || bytes[i + 2] == b'\t')
                            {
                                i += 2;
                                while i < bytes.len()
                                    && (bytes[i] == b' ' || bytes[i] == b'\t')
                                {
                                    i += 1;
                                }
                                continue;
                            }
                            return Err(EmailError::BadFold { at: i });
                        }
                        if b == b'\n' {
                            return Err(EmailError::BadFold { at: i });
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
                        if !((0x20..=0x7e).contains(&b) || b == b'\t') {
                            return Err(EmailError::CommentChar { at: i });
                        }
                        i += 1;
                    }
                    if depth != 0 {
                        return Err(EmailError::UnclosedComment { at: outer });
                    }
                    continue;
                }

                if i == before {
                    break;
                }
            }
        } else if bytes[i] == b'\r' || bytes[i] == b'\n' {
            is_cfws = true;
            let at = i;
            if bytes[i] == b'\r'
                && i + 2 < bytes.len()
                && bytes[i + 1] == b'\n'
                && (bytes[i + 2] == b' ' || bytes[i + 2] == b'\t')
            {
                i += 2;
                while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
                    i += 1;
                }
            } else {
                return Err(EmailError::BadFold { at });
            }
        } else if bytes[i] == b'(' {
            is_cfws = true;
            let outer = i;
            let mut depth = 0usize;
            while i < bytes.len() {
                let b = bytes[i];
                if b == b'\\' {
                    if i + 1 >= bytes.len() || !(0x20..=0x7e).contains(&bytes[i + 1]) {
                        return Err(EmailError::CommentEscape { at: i });
                    }
                    i += 2;
                    continue;
                }
                if b == b'\r' {
                    if i + 2 < bytes.len()
                        && bytes[i + 1] == b'\n'
                        && (bytes[i + 2] == b' ' || bytes[i + 2] == b'\t')
                    {
                        i += 2;
                        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
                            i += 1;
                        }
                        continue;
                    }
                    return Err(EmailError::BadFold { at: i });
                }
                if b == b'\n' {
                    return Err(EmailError::BadFold { at: i });
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
                if !((0x20..=0x7e).contains(&b) || b == b'\t') {
                    return Err(EmailError::CommentChar { at: i });
                }
                i += 1;
            }
            if depth != 0 {
                return Err(EmailError::UnclosedComment { at: outer });
            }
        } else if bytes[i] == b')' {
            return Err(EmailError::UnexpectedClose { at: i });
        }

        if is_cfws {
            runs.push(Run {
                start: run_start,
                end: i,
            });
        } else {
            cleaned.push(bytes[i]);
            positions.push(i);
            i += 1;
        }
    }

    for run in &runs {
        let left_at = positions.iter().position(|&p| p >= run.end);
        let right_at = positions.iter().rposition(|&p| p < run.start);
        let left_ok = right_at.is_none()
            || cleaned.get(right_at.unwrap()) == Some(&b'@');
        let right_ok = left_at.is_none()
            || cleaned.get(left_at.unwrap()) == Some(&b'@');
        if !(left_ok || right_ok) {
            return Err(EmailError::CfwsBoundary { at: run.start });
        }
    }

    if cleaned.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let mut quote_end = None;
    if cleaned[0] == b'"' {
        let mut j = 1usize;
        while j < cleaned.len() {
            if cleaned[j] == b'\\' {
                j += 2;
            } else if cleaned[j] == b'"' {
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

    let protected_end = quote_end.unwrap_or(0);
    let ats: Vec<usize> = cleaned
        .iter()
        .enumerate()
        .filter_map(|(idx, &b)| {
            if b == b'@' && (quote_end.is_none() || idx > protected_end) {
                Some(idx)
            } else {
                None
            }
        })
        .collect();

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
        return Err(EmailError::EmptyLocal {
            at: positions[sep],
        });
    }

    let local_bytes = &cleaned[..sep];
    let local_positions = &positions[..sep];
    let quoted = local_bytes[0] == b'"';
    let local = if !quoted {
        if local_bytes.len() > 64 {
            return Err(EmailError::LocalTooLong {
                at: local_positions[64],
            });
        }
        let mut prev_dot = false;
        for (idx, &b) in local_bytes.iter().enumerate() {
            if b == b'.' {
                if idx == 0 || idx + 1 == local_bytes.len() || prev_dot {
                    return Err(EmailError::LocalDot {
                        at: local_positions[idx],
                    });
                }
                prev_dot = true;
            } else {
                prev_dot = false;
                let allowed = b.is_ascii_alphanumeric()
                    || b"!#$%&'*+-/=?^_`{|}~".contains(&b);
                if !allowed {
                    return Err(EmailError::LocalChar {
                        at: local_positions[idx],
                    });
                }
            }
        }
        String::from_utf8(local_bytes.to_vec()).unwrap()
    } else {
        let end = quote_end.unwrap();
        if end + 1 != sep {
            return Err(EmailError::TrailingLocal {
                at: positions[end + 1],
            });
        }
        let mut decoded = Vec::new();
        let mut origins = Vec::new();
        let mut j = 1usize;
        while j < end {
            let b = local_bytes[j];
            if b == b'\\' {
                if j + 1 >= end {
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
                if !(0x20..=0x7e).contains(&b) {
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
                at: local_positions[end],
            });
        }
        if decoded.len() > 64 {
            return Err(EmailError::LocalTooLong { at: origins[64] });
        }
        String::from_utf8(decoded).unwrap()
    };

    let domain_start = sep + 1;
    let mut domain_end = cleaned.len();
    let mut root_dot = None;
    if domain_end > domain_start && cleaned[domain_end - 1] == b'.' {
        domain_end -= 1;
        root_dot = Some(positions[domain_end]);
    }
    if domain_end == domain_start {
        return Err(EmailError::EmptyDomain {
            at: positions.get(domain_start).copied().unwrap_or(input.len()),
        });
    }

    if domain_end - domain_start > 253 {
        return Err(EmailError::DomainTooLong {
            at: positions[domain_start + 253],
        });
    }

    let mut labels = Vec::<(usize, usize)>::new();
    let mut start = domain_start;
    for idx in domain_start..=domain_end {
        if idx == domain_end || cleaned[idx] == b'.' {
            if start == idx {
                let at = positions.get(start).copied().or(root_dot).unwrap_or(input.len());
                return Err(EmailError::EmptyLabel { at });
            }
            labels.push((start, idx));
            start = idx + 1;
        }
    }

    for &(start, end) in &labels {
        let len = end - start;
        if len > 63 {
            return Err(EmailError::LabelTooLong {
                at: positions[start + 63],
            });
        }
        if cleaned[start] == b'-' {
            return Err(EmailError::LabelHyphen {
                at: positions[start],
            });
        }
        if cleaned[end - 1] == b'-' {
            return Err(EmailError::LabelHyphen {
                at: positions[end - 1],
            });
        }
        for idx in start..end {
            let b = cleaned[idx];
            if !b.is_ascii_alphanumeric() && b != b'-' {
                return Err(EmailError::DomainChar {
                    at: positions[idx],
                });
            }
        }
    }

    if labels.len() < 2 {
        return Err(EmailError::DomainNeedsDot {
            at: positions[domain_start],
        });
    }

    let (last_start, last_end) = *labels.last().unwrap();
    if cleaned[last_start..last_end]
        .iter()
        .all(|b| b.is_ascii_digit())
    {
        return Err(EmailError::NumericTld {
            at: positions[last_start],
        });
    }

    let domain: Vec<u8> = cleaned[domain_start..domain_end]
        .iter()
        .map(u8::to_ascii_lowercase)
        .collect();

    Ok(Address {
        local,
        domain: String::from_utf8(domain).unwrap(),
        quoted,
    })
}

fn main() {}