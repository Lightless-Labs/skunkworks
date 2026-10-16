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
struct Byte {
    value: u8,
    pos: usize,
}

struct CfwsRun {
    start: usize,
    first: usize,
    last: usize,
}

pub fn parse_email(input: &str) -> Result<Address, EmailError> {
    let raw = input.as_bytes();
    if raw.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    if let Some(at) = raw.iter().position(|&b| b >= 128) {
        return Err(EmailError::NonAscii { at });
    }

    let mut kept: Vec<Byte> = Vec::with_capacity(raw.len());
    let mut runs: Vec<CfwsRun> = Vec::new();
    let mut i = 0;
    let mut protected_quote = false;

    while i < raw.len() {
        let b = raw[i];

        if protected_quote {
            kept.push(Byte { value: b, pos: i });
            if b == b'\\' {
                if i + 1 < raw.len() {
                    kept.push(Byte { value: raw[i + 1], pos: i + 1 });
                    i += 2;
                    continue;
                }
                i += 1;
                continue;
            }
            if b == b'"' {
                protected_quote = false;
            }
            i += 1;
            continue;
        }

        if b == b'"' && kept.is_empty() {
            protected_quote = true;
            kept.push(Byte { value: b, pos: i });
            i += 1;
            continue;
        }

        let is_cfws_start = b == b' ' || b == b'\t' || b == b'\r' || b == b'\n' || b == b'(';
        if !is_cfws_start && b != b')' {
            kept.push(Byte { value: b, pos: i });
            i += 1;
            continue;
        }

        let run_start = i;
        let mut run_first = i;
        let mut run_last = i;
        let mut in_run = true;

        while in_run && i < raw.len() {
            match raw[i] {
                b' ' | b'\t' => {
                    run_last = i;
                    i += 1;
                }
                b'\r' | b'\n' => {
                    if raw[i] != b'\r'
                        || i + 2 >= raw.len()
                        || raw[i + 1] != b'\n'
                        || (raw[i + 2] != b' ' && raw[i + 2] != b'\t')
                    {
                        return Err(EmailError::BadFold { at: i });
                    }
                    let fold_at = i;
                    i += 2;
                    while i < raw.len() && (raw[i] == b' ' || raw[i] == b'\t') {
                        i += 1;
                    }
                    run_last = i - 1;
                    if run_first == run_start && fold_at < run_first {
                        run_first = fold_at;
                    }
                }
                b'(' => {
                    if run_first == run_start {
                        run_first = i;
                    }
                    let outer = i;
                    let mut stack = Vec::with_capacity(3);
                    stack.push(i);
                    i += 1;
                    while !stack.is_empty() {
                        if i >= raw.len() {
                            return Err(EmailError::UnclosedComment { at: outer });
                        }
                        match raw[i] {
                            b'\\' => {
                                if i + 1 >= raw.len()
                                    || !(0x20..=0x7e).contains(&raw[i + 1])
                                {
                                    return Err(EmailError::CommentEscape { at: i });
                                }
                                run_last = i + 1;
                                i += 2;
                            }
                            b'\r' | b'\n' => {
                                if raw[i] != b'\r'
                                    || i + 2 >= raw.len()
                                    || raw[i + 1] != b'\n'
                                    || (raw[i + 2] != b' ' && raw[i + 2] != b'\t')
                                {
                                    return Err(EmailError::BadFold { at: i });
                                }
                                i += 2;
                                while i < raw.len()
                                    && (raw[i] == b' ' || raw[i] == b'\t')
                                {
                                    i += 1;
                                }
                                run_last = i - 1;
                            }
                            b'(' => {
                                if stack.len() == 3 {
                                    return Err(EmailError::CommentDepth { at: i });
                                }
                                stack.push(i);
                                run_last = i;
                                i += 1;
                            }
                            b')' => {
                                stack.pop();
                                run_last = i;
                                i += 1;
                            }
                            c if (0x20..=0x7e).contains(&c) || c == b'\t' => {
                                run_last = i;
                                i += 1;
                            }
                            _ => return Err(EmailError::CommentChar { at: i }),
                        }
                    }
                }
                b')' => return Err(EmailError::UnexpectedClose { at: i }),
                _ => in_run = false,
            }

            if i >= raw.len() {
                break;
            }
            match raw[i] {
                b' ' | b'\t' | b'\r' | b'\n' | b'(' => {}
                _ => in_run = false,
            }
        }

        if i > run_start {
            if run_last < run_first {
                run_last = i - 1;
            }
            runs.push(CfwsRun {
                start: run_start,
                first: run_first,
                last: run_last,
            });
        }
    }

    for run in &runs {
        let left = kept.iter().rev().find(|b| b.pos < run.start);
        let right = kept.iter().find(|b| b.pos > run.last);
        let at_boundary = left.is_none()
            || right.is_none()
            || left.map(|b| b.value) == Some(b'@')
            || right.map(|b| b.value) == Some(b'@');
        if !at_boundary {
            return Err(EmailError::CfwsBoundary { at: run.first });
        }
    }

    if kept.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let values: Vec<u8> = kept.iter().map(|b| b.value).collect();
    let n = values.len();

    let quote_end = if values.first() == Some(&b'"') {
        let mut j = 1;
        let mut found = None;
        while j < n {
            if values[j] == b'\\' {
                j += 2;
            } else if values[j] == b'"' {
                found = Some(j);
                break;
            } else {
                j += 1;
            }
        }
        match found {
            Some(end) => Some(end),
            None => return Err(EmailError::UnclosedQuote { at: kept[0].pos }),
        }
    } else {
        None
    };

    let mut separators = Vec::new();
    for j in 0..n {
        if values[j] == b'@' && !matches!(quote_end, Some(end) if j <= end) {
            separators.push(j);
        }
    }
    if separators.is_empty() {
        return Err(EmailError::MissingAt);
    }
    if separators.len() > 1 {
        return Err(EmailError::MultipleAt {
            at: kept[separators[1]].pos,
        });
    }

    let sep = separators[0];
    if sep == 0 {
        return Err(EmailError::EmptyLocal { at: kept[sep].pos });
    }

    let (local, quoted) = if let Some(end) = quote_end {
        if end + 1 != sep {
            let at = if end + 1 < n {
                kept[end + 1].pos
            } else {
                input.len()
            };
            return Err(EmailError::TrailingLocal { at });
        }

        let mut decoded = Vec::new();
        let mut origins = Vec::new();
        let mut j = 1;
        while j < end {
            if values[j] == b'\\' {
                if j + 1 >= end
                    || !matches!(values[j + 1], b'"' | b'\\' | b'@')
                {
                    return Err(EmailError::BadEscape { at: kept[j].pos });
                }
                decoded.push(values[j + 1]);
                origins.push(kept[j].pos);
                j += 2;
            } else {
                if !(0x20..=0x7e).contains(&values[j]) {
                    return Err(EmailError::QuotedChar { at: kept[j].pos });
                }
                decoded.push(values[j]);
                origins.push(kept[j].pos);
                j += 1;
            }
        }

        if decoded.is_empty() {
            return Err(EmailError::EmptyLocal { at: kept[end].pos });
        }
        if decoded.len() > 64 {
            return Err(EmailError::LocalTooLong { at: origins[64] });
        }
        (String::from_utf8(decoded).unwrap(), true)
    } else {
        if sep > 64 {
            return Err(EmailError::LocalTooLong { at: kept[64].pos });
        }
        let mut prev_dot = false;
        for j in 0..sep {
            let b = values[j];
            if b == b'.' {
                if j == 0 || j + 1 == sep || prev_dot {
                    return Err(EmailError::LocalDot { at: kept[j].pos });
                }
                prev_dot = true;
            } else {
                prev_dot = false;
                if !b.is_ascii_alphanumeric()
                    && !b"!#$%&'*+-/=?^_`{|}~".contains(&b)
                {
                    return Err(EmailError::LocalChar { at: kept[j].pos });
                }
            }
        }
        (String::from_utf8(values[..sep].to_vec()).unwrap(), false)
    };

    let domain_start = sep + 1;
    let raw_domain = &values[domain_start..];
    let root_dot = raw_domain.last() == Some(&b'.');
    let domain_len = raw_domain.len() - usize::from(root_dot);
    let domain_start_pos = if domain_start < n {
        kept[domain_start].pos
    } else {
        input.len()
    };
    if domain_len == 0 {
        return Err(EmailError::EmptyDomain {
            at: if root_dot {
                kept[n - 1].pos
            } else {
                domain_start_pos
            },
        });
    }
    if domain_len > 253 {
        return Err(EmailError::DomainTooLong {
            at: kept[domain_start + 253].pos,
        });
    }

    let mut labels: Vec<(usize, usize)> = Vec::new();
    let mut label_start = 0;
    for j in 0..=domain_len {
        if j == domain_len || raw_domain[j] == b'.' {
            let absolute_start = domain_start + label_start;
            if label_start == j {
                let at = if j == domain_len && root_dot {
                    kept[n - 1].pos
                } else if absolute_start < n {
                    kept[absolute_start].pos
                } else {
                    input.len()
                };
                return Err(EmailError::EmptyLabel { at });
            }
            let len = j - label_start;
            if len > 63 {
                return Err(EmailError::LabelTooLong {
                    at: kept[absolute_start + 63].pos,
                });
            }
            let first = raw_domain[label_start];
            let last = raw_domain[j - 1];
            if first == b'-' {
                return Err(EmailError::LabelHyphen {
                    at: kept[absolute_start].pos,
                });
            }
            if last == b'-' {
                return Err(EmailError::LabelHyphen {
                    at: kept[absolute_start + len - 1].pos,
                });
            }
            for k in label_start..j {
                let b = raw_domain[k];
                if !b.is_ascii_alphanumeric() && b != b'-' {
                    return Err(EmailError::DomainChar {
                        at: kept[domain_start + k].pos,
                    });
                }
            }
            labels.push((label_start, len));
            label_start = j + 1;
        }
    }

    if labels.len() < 2 {
        return Err(EmailError::DomainNeedsDot {
            at: domain_start_pos,
        });
    }
    let (last_start, last_len) = *labels.last().unwrap();
    if raw_domain[last_start..last_start + last_len]
        .iter()
        .all(u8::is_ascii_digit)
    {
        return Err(EmailError::NumericTld {
            at: kept[domain_start + last_start].pos,
        });
    }

    let domain = raw_domain[..domain_len]
        .iter()
        .map(|b| b.to_ascii_lowercase() as char)
        .collect();

    Ok(Address {
        local,
        domain,
        quoted,
    })
}