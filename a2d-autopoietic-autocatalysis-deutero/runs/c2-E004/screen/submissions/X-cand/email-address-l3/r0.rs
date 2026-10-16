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

struct Cleaned {
    bytes: Vec<u8>,
    positions: Vec<usize>,
    runs: Vec<(usize, usize)>,
}

fn printable(b: u8) -> bool {
    (0x20..=0x7e).contains(&b)
}

fn consume_fold(raw: &[u8], i: usize) -> Option<usize> {
    if raw.get(i) != Some(&b'\r')
        || raw.get(i + 1) != Some(&b'\n')
        || !matches!(raw.get(i + 2), Some(b' ') | Some(b'\t'))
    {
        return None;
    }

    let mut end = i + 2;
    while matches!(raw.get(end), Some(b' ') | Some(b'\t')) {
        end += 1;
    }
    Some(end)
}

fn cleanup(input: &str) -> Result<Cleaned, EmailError> {
    let raw = input.as_bytes();
    let mut bytes = Vec::with_capacity(raw.len());
    let mut positions = Vec::with_capacity(raw.len());
    let mut runs = Vec::new();

    let mut i = 0;
    let mut quote_protected = false;
    let mut in_quote = false;

    while i < raw.len() {
        if in_quote {
            let b = raw[i];
            bytes.push(b);
            positions.push(i);
            i += 1;

            if b == b'\\' {
                if i < raw.len() {
                    bytes.push(raw[i]);
                    positions.push(i);
                    i += 1;
                }
            } else if b == b'"' {
                in_quote = false;
            }
            continue;
        }

        if matches!(raw[i], b' ' | b'\t' | b'\r' | b'\n' | b'(') {
            let run_start = i;

            loop {
                if i >= raw.len() {
                    break;
                }

                match raw[i] {
                    b' ' | b'\t' => i += 1,
                    b'\r' | b'\n' => {
                        if let Some(end) = consume_fold(raw, i) {
                            i = end;
                        } else {
                            return Err(EmailError::BadFold { at: i });
                        }
                    }
                    b'(' => {
                        let outer = i;
                        let mut stack = Vec::with_capacity(3);

                        while i < raw.len() {
                            match raw[i] {
                                b'(' => {
                                    if stack.len() == 3 {
                                        return Err(EmailError::CommentDepth { at: i });
                                    }
                                    stack.push(i);
                                    i += 1;
                                }
                                b')' => {
                                    stack.pop();
                                    i += 1;
                                    if stack.is_empty() {
                                        break;
                                    }
                                }
                                b'\\' => {
                                    let slash = i;
                                    if i + 1 >= raw.len() || !printable(raw[i + 1]) {
                                        return Err(EmailError::CommentEscape { at: slash });
                                    }
                                    i += 2;
                                }
                                b'\r' | b'\n' => {
                                    if let Some(end) = consume_fold(raw, i) {
                                        i = end;
                                    } else {
                                        return Err(EmailError::BadFold { at: i });
                                    }
                                }
                                b if printable(b) || b == b'\t' => i += 1,
                                _ => return Err(EmailError::CommentChar { at: i }),
                            }
                        }

                        if !stack.is_empty() {
                            return Err(EmailError::UnclosedComment { at: outer });
                        }
                    }
                    _ => break,
                }
            }

            if i > run_start {
                runs.push((run_start, i));
                continue;
            }
        }

        if raw[i] == b')' {
            return Err(EmailError::UnexpectedClose { at: i });
        }

        if !quote_protected && bytes.is_empty() && raw[i] == b'"' {
            quote_protected = true;
            in_quote = true;
        }

        bytes.push(raw[i]);
        positions.push(i);
        i += 1;
    }

    Ok(Cleaned {
        bytes,
        positions,
        runs,
    })
}

fn parse_email_impl(input: &str) -> Result<Address, EmailError> {
    let raw = input.as_bytes();

    if raw.is_empty() {
        return Err(EmailError::EmptyInput);
    }
    if let Some(at) = raw.iter().position(|&b| b >= 128) {
        return Err(EmailError::NonAscii { at });
    }

    let cleaned = cleanup(input)?;
    let bytes = &cleaned.bytes;
    let positions = &cleaned.positions;

    // A run is at an edge if it has no retained bytes on that side. Otherwise
    // it is legal only when its immediately adjacent retained byte is '@'.
    for &(start, end) in &cleaned.runs {
        let left = positions.iter().rposition(|&p| p < start);
        let right = positions.iter().position(|&p| p >= end);

        let at_edge = left.is_none() || right.is_none();
        let adjacent_to_at = left.is_some_and(|idx| bytes[idx] == b'@')
            || right.is_some_and(|idx| bytes[idx] == b'@');

        if !(at_edge || adjacent_to_at) {
            return Err(EmailError::CfwsBoundary { at: start });
        }
    }

    if bytes.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let pos = |idx: usize| positions.get(idx).copied().unwrap_or(input.len());

    let mut quote_end = None;
    if bytes[0] == b'"' {
        let mut j = 1;
        while j < bytes.len() {
            if bytes[j] == b'\\' {
                j += 2;
            } else if bytes[j] == b'"' {
                quote_end = Some(j);
                break;
            } else {
                j += 1;
            }
        }
        if quote_end.is_none() {
            return Err(EmailError::UnclosedQuote { at: pos(0) });
        }
    }

    let protected_end = quote_end.map_or(0, |end| end + 1);
    let mut separators = Vec::new();
    for (idx, &b) in bytes.iter().enumerate() {
        if b == b'@' && (quote_end.is_none() || idx >= protected_end) {
            separators.push(idx);
        }
    }

    if separators.is_empty() {
        return Err(EmailError::MissingAt);
    }
    if separators.len() >= 2 {
        return Err(EmailError::MultipleAt {
            at: pos(separators[1]),
        });
    }

    let at_idx = separators[0];
    if at_idx == 0 {
        return Err(EmailError::EmptyLocal { at: pos(at_idx) });
    }

    let mut local = Vec::new();
    let quoted;

    if let Some(qend) = quote_end {
        if qend + 1 != at_idx {
            return Err(EmailError::TrailingLocal { at: pos(qend + 1) });
        }

        quoted = true;
        let mut j = 1;
        while j < qend {
            if bytes[j] == b'\\' {
                let slash = j;
                if j + 1 >= qend || !matches!(bytes[j + 1], b'"' | b'\\' | b'@') {
                    return Err(EmailError::BadEscape { at: pos(slash) });
                }
                local.push((bytes[j + 1], pos(slash)));
                j += 2;
            } else {
                if !printable(bytes[j]) {
                    return Err(EmailError::QuotedChar { at: pos(j) });
                }
                local.push((bytes[j], pos(j)));
                j += 1;
            }
        }

        if local.is_empty() {
            return Err(EmailError::EmptyLocal { at: pos(qend) });
        }
        if local.len() > 64 {
            return Err(EmailError::LocalTooLong { at: local[64].1 });
        }
    } else {
        quoted = false;

        if at_idx > 64 {
            return Err(EmailError::LocalTooLong { at: pos(64) });
        }

        for j in 0..at_idx {
            let b = bytes[j];
            if b == b'.' {
                if j == 0 || j + 1 == at_idx || bytes[j - 1] == b'.' {
                    return Err(EmailError::LocalDot { at: pos(j) });
                }
            } else if !b.is_ascii_alphanumeric() && !b"!#$%&'*+-/=?^_`{|}~".contains(&b) {
                return Err(EmailError::LocalChar { at: pos(j) });
            }
        }

        local.extend((0..at_idx).map(|j| (bytes[j], pos(j))));
    }

    let domain_start = at_idx + 1;
    let domain_start_pos = pos(domain_start);
    if domain_start >= bytes.len() {
        return Err(EmailError::EmptyDomain {
            at: domain_start_pos,
        });
    }

    let mut domain_end = bytes.len();
    let root_pos = if bytes.last() == Some(&b'.') {
        domain_end -= 1;
        Some(pos(bytes.len() - 1))
    } else {
        None
    };

    if domain_end <= domain_start {
        return Err(EmailError::EmptyDomain {
            at: domain_start_pos,
        });
    }

    if domain_end - domain_start > 253 {
        return Err(EmailError::DomainTooLong {
            at: pos(domain_start + 253),
        });
    }

    let mut labels = Vec::new();
    let mut label_start = domain_start;
    for cursor in domain_start..=domain_end {
        if cursor == domain_end || bytes[cursor] == b'.' {
            labels.push((label_start, cursor));
            label_start = cursor + 1;
        }
    }

    for (index, &(start, end)) in labels.iter().enumerate() {
        if start == end {
            let at = if index + 1 == labels.len() {
                root_pos.unwrap_or_else(|| pos(start))
            } else {
                pos(start)
            };
            return Err(EmailError::EmptyLabel { at });
        }

        if end - start > 63 {
            return Err(EmailError::LabelTooLong { at: pos(start + 63) });
        }
        if bytes[start] == b'-' {
            return Err(EmailError::LabelHyphen { at: pos(start) });
        }
        if bytes[end - 1] == b'-' {
            return Err(EmailError::LabelHyphen { at: pos(end - 1) });
        }
        for j in start..end {
            let b = bytes[j];
            if !b.is_ascii_alphanumeric() && b != b'-' {
                return Err(EmailError::DomainChar { at: pos(j) });
            }
        }
    }

    if labels.len() < 2 {
        return Err(EmailError::DomainNeedsDot {
            at: domain_start_pos,
        });
    }

    let (last_start, last_end) = *labels.last().unwrap();
    if bytes[last_start..last_end].iter().all(u8::is_ascii_digit) {
        return Err(EmailError::NumericTld { at: pos(last_start) });
    }

    let local_string: String = local.into_iter().map(|(b, _)| b as char).collect();
    let domain_string: String = bytes[domain_start..domain_end]
        .iter()
        .map(|b| b.to_ascii_lowercase() as char)
        .collect();

    Ok(Address {
        local: local_string,
        domain: domain_string,
        quoted,
    })
}

pub fn parse_email(input: &str) -> Result<Address, EmailError> {
    parse_email_impl(input)
}