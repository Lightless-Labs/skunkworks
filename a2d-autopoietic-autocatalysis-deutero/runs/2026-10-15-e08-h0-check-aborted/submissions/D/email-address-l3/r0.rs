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

struct Cleaned {
    bytes: Vec<Byte>,
    runs: Vec<usize>,
}

fn is_hws(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

fn is_printable(b: u8) -> bool {
    (0x20..=0x7e).contains(&b)
}

fn parse_email(input: &str) -> Result<Address, EmailError> {
    let raw = input.as_bytes();
    if raw.is_empty() {
        return Err(EmailError::EmptyInput);
    }
    if let Some(at) = raw.iter().position(|&b| b >= 128) {
        return Err(EmailError::NonAscii { at });
    }

    let cleaned = cleanup(raw)?;
    for &run_at in &cleaned.runs {
        let run_index = cleaned
            .bytes
            .iter()
            .position(|b| b.pos > run_at)
            .unwrap_or(cleaned.bytes.len());
        let at_left = run_index > 0 && cleaned.bytes[run_index - 1].value == b'@';
        let at_right = run_index < cleaned.bytes.len()
            && cleaned.bytes[run_index].value == b'@';
        if run_index != 0
            && run_index != cleaned.bytes.len()
            && !at_left
            && !at_right
        {
            return Err(EmailError::CfwsBoundary { at: run_at });
        }
    }

    if cleaned.bytes.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let bytes = &cleaned.bytes;
    let mut quoted_end = None;
    if bytes[0].value == b'"' {
        let mut i = 1;
        while i < bytes.len() {
            match bytes[i].value {
                b'\\' => i += 2,
                b'"' => {
                    quoted_end = Some(i);
                    break;
                }
                _ => i += 1,
            }
        }
        if quoted_end.is_none() {
            return Err(EmailError::UnclosedQuote { at: bytes[0].pos });
        }
    }

    let mut separators = Vec::new();
    let start_after_quote = quoted_end.map_or(0, |i| i + 1);
    for i in start_after_quote..bytes.len() {
        if bytes[i].value == b'@' {
            separators.push(i);
        }
    }
    if separators.is_empty() {
        return Err(EmailError::MissingAt);
    }
    if separators.len() >= 2 {
        return Err(EmailError::MultipleAt {
            at: bytes[separators[1]].pos,
        });
    }
    let sep = separators[0];
    let sep_pos = bytes[sep].pos;
    if sep == 0 {
        return Err(EmailError::EmptyLocal { at: sep_pos });
    }

    let quoted = bytes[0].value == b'"';
    let local = if quoted {
        let close = quoted_end.unwrap();
        if close != sep - 1 {
            return Err(EmailError::TrailingLocal {
                at: bytes[close + 1].pos,
            });
        }
        let mut decoded = Vec::new();
        let mut origins = Vec::new();
        let mut i = 1;
        while i < close {
            let b = bytes[i].value;
            if b == b'\\' {
                if i + 1 >= close {
                    return Err(EmailError::BadEscape { at: bytes[i].pos });
                }
                let next = bytes[i + 1].value;
                if !matches!(next, b'"' | b'\\' | b'@') {
                    return Err(EmailError::BadEscape { at: bytes[i].pos });
                }
                decoded.push(next);
                origins.push(bytes[i].pos);
                i += 2;
            } else {
                if !is_printable(b) {
                    return Err(EmailError::QuotedChar { at: bytes[i].pos });
                }
                decoded.push(b);
                origins.push(bytes[i].pos);
                i += 1;
            }
        }
        if decoded.is_empty() {
            return Err(EmailError::EmptyLocal {
                at: bytes[close].pos,
            });
        }
        if decoded.len() > 64 {
            return Err(EmailError::LocalTooLong { at: origins[64] });
        }
        String::from_utf8(decoded).unwrap()
    } else {
        if sep > 64 {
            return Err(EmailError::LocalTooLong {
                at: bytes[64].pos,
            });
        }
        let mut prev_dot = false;
        for i in 0..sep {
            let b = bytes[i].value;
            if b == b'.' {
                if i == 0 || i + 1 == sep || prev_dot {
                    return Err(EmailError::LocalDot { at: bytes[i].pos });
                }
                prev_dot = true;
            } else {
                prev_dot = false;
                if !b.is_ascii_alphanumeric()
                    && !b"!#$%&'*+-/=?^_`{|}~".contains(&b)
                {
                    return Err(EmailError::LocalChar { at: bytes[i].pos });
                }
            }
        }
        String::from_utf8(bytes[..sep].iter().map(|b| b.value).collect()).unwrap()
    };

    let domain_start_index = sep + 1;
    let domain_start_pos = if domain_start_index < bytes.len() {
        bytes[domain_start_index].pos
    } else {
        input.len()
    };
    if domain_start_index == bytes.len() {
        return Err(EmailError::EmptyDomain {
            at: domain_start_pos,
        });
    }

    let mut domain_end = bytes.len();
    let mut root_dot_pos = None;
    if bytes[domain_end - 1].value == b'.' {
        domain_end -= 1;
        root_dot_pos = Some(bytes[domain_end].pos);
    }
    if domain_end == domain_start_index {
        return Err(EmailError::EmptyDomain {
            at: domain_start_pos,
        });
    }

    let domain_len = domain_end - domain_start_index;
    if domain_len > 253 {
        return Err(EmailError::DomainTooLong {
            at: bytes[domain_start_index + 253].pos,
        });
    }

    let mut labels: Vec<(usize, usize)> = Vec::new();
    let mut label_start = domain_start_index;
    for i in domain_start_index..=domain_end {
        if i == domain_end || bytes[i].value == b'.' {
            if i == label_start {
                let pos = if i == domain_end {
                    root_dot_pos.unwrap_or(input.len())
                } else {
                    bytes[i].pos
                };
                return Err(EmailError::EmptyLabel { at: pos });
            }
            let len = i - label_start;
            if len > 63 {
                return Err(EmailError::LabelTooLong {
                    at: bytes[label_start + 63].pos,
                });
            }
            if bytes[label_start].value == b'-' {
                return Err(EmailError::LabelHyphen {
                    at: bytes[label_start].pos,
                });
            }
            if bytes[i - 1].value == b'-' {
                return Err(EmailError::LabelHyphen {
                    at: bytes[i - 1].pos,
                });
            }
            for j in label_start..i {
                let b = bytes[j].value;
                if !b.is_ascii_alphanumeric() && b != b'-' {
                    return Err(EmailError::DomainChar { at: bytes[j].pos });
                }
            }
            labels.push((label_start, i));
            label_start = i + 1;
        }
    }

    if labels.len() < 2 {
        return Err(EmailError::DomainNeedsDot {
            at: domain_start_pos,
        });
    }
    let (last_start, last_end) = *labels.last().unwrap();
    if bytes[last_start..last_end]
        .iter()
        .all(|b| b.value.is_ascii_digit())
    {
        return Err(EmailError::NumericTld {
            at: bytes[last_start].pos,
        });
    }

    let domain = bytes[domain_start_index..domain_end]
        .iter()
        .map(|b| b.value.to_ascii_lowercase())
        .collect::<Vec<_>>();
    Ok(Address {
        local,
        domain: String::from_utf8(domain).unwrap(),
        quoted,
    })
}

fn cleanup(raw: &[u8]) -> Result<Cleaned, EmailError> {
    let mut bytes = Vec::new();
    let mut runs = Vec::new();
    let mut i = 0;
    let mut protected_quote = false;

    while i < raw.len() {
        if protected_quote {
            let b = raw[i];
            bytes.push(Byte { value: b, pos: i });
            i += 1;
            if b == b'\\' {
                if i < raw.len() {
                    bytes.push(Byte {
                        value: raw[i],
                        pos: i,
                    });
                    i += 1;
                }
            } else if b == b'"' {
                protected_quote = false;
            }
            continue;
        }

        let first_retained = bytes.is_empty();
        let b = raw[i];
        if first_retained && b == b'"' {
            protected_quote = true;
            bytes.push(Byte { value: b, pos: i });
            i += 1;
            continue;
        }

        let is_cfws_start = is_hws(b)
            || b == b'('
            || b == b'\r'
            || b == b'\n';
        if !is_cfws_start {
            if b == b')' {
                return Err(EmailError::UnexpectedClose { at: i });
            }
            bytes.push(Byte { value: b, pos: i });
            i += 1;
            continue;
        }

        let run_at = i;
        let mut comment_depth = 0usize;
        let mut outer_open = None;
        let mut in_run = true;

        while in_run && i < raw.len() {
            if comment_depth > 0 {
                let c = raw[i];
                if c == b'\\' {
                    if i + 1 >= raw.len() || !is_printable(raw[i + 1]) {
                        return Err(EmailError::CommentEscape { at: i });
                    }
                    i += 2;
                    continue;
                }
                if c == b'(' {
                    if comment_depth == 3 {
                        return Err(EmailError::CommentDepth { at: i });
                    }
                    comment_depth += 1;
                    i += 1;
                    continue;
                }
                if c == b')' {
                    comment_depth -= 1;
                    i += 1;
                    continue;
                }
                if c == b'\r' || c == b'\n' {
                    if c == b'\r'
                        && i + 2 < raw.len()
                        && raw[i + 1] == b'\n'
                        && is_hws(raw[i + 2])
                    {
                        i += 2;
                        while i < raw.len() && is_hws(raw[i]) {
                            i += 1;
                        }
                        continue;
                    }
                    return Err(EmailError::BadFold { at: i });
                }
                if is_printable(c) || c == b'\t' {
                    i += 1;
                    continue;
                }
                return Err(EmailError::CommentChar { at: i });
            }

            if is_hws(raw[i]) {
                i += 1;
                continue;
            }
            if raw[i] == b'\r' || raw[i] == b'\n' {
                if raw[i] == b'\r'
                    && i + 2 < raw.len()
                    && raw[i + 1] == b'\n'
                    && is_hws(raw[i + 2])
                {
                    i += 2;
                    while i < raw.len() && is_hws(raw[i]) {
                        i += 1;
                    }
                    continue;
                }
                return Err(EmailError::BadFold { at: i });
            }
            if raw[i] == b'(' {
                if comment_depth == 0 {
                    outer_open = Some(i);
                }
                comment_depth = 1;
                i += 1;
                continue;
            }
            in_run = false;
        }

        if comment_depth != 0 {
            return Err(EmailError::UnclosedComment {
                at: outer_open.unwrap_or(run_at),
            });
        }
        runs.push(run_at);
    }

    Ok(Cleaned { bytes, runs })
}