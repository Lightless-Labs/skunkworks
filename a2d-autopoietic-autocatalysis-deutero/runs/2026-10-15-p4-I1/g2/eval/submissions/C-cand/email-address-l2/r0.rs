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
}

pub fn parse_email(input: &str) -> Result<Address, EmailError> {
    let bytes = input.as_bytes();

    if bytes.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    if let Some(at) = bytes.iter().position(|&b| b >= 128) {
        return Err(EmailError::NonAscii { at });
    }

    let quoted_close = if bytes[0] == b'"' {
        let mut i = 1;
        let mut close = None;
        while i < bytes.len() {
            match bytes[i] {
                b'\\' => {
                    i += 2;
                }
                b'"' => {
                    close = Some(i);
                    break;
                }
                _ => i += 1,
            }
        }
        match close {
            Some(i) => Some(i),
            None => return Err(EmailError::UnclosedQuote { at: 0 }),
        }
    } else {
        None
    };

    let mut separators = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if let Some(close) = quoted_close {
            if i <= close {
                i = close + 1;
                continue;
            }
        }
        if bytes[i] == b'@' {
            separators.push(i);
        }
        i += 1;
    }

    if separators.is_empty() {
        return Err(EmailError::MissingAt);
    }
    if separators.len() >= 2 {
        return Err(EmailError::MultipleAt { at: separators[1] });
    }

    let sep = separators[0];
    if sep == 0 {
        return Err(EmailError::EmptyLocal { at: sep });
    }

    let local_bytes = &bytes[..sep];
    let (local, quoted) = if let Some(close) = quoted_close {
        if close + 1 != sep {
            return Err(EmailError::TrailingLocal { at: close + 1 });
        }

        let mut decoded = Vec::new();
        let mut origins = Vec::new();
        let mut p = 1;
        while p < close {
            if bytes[p] == b'\\' {
                if p + 1 >= close {
                    return Err(EmailError::BadEscape { at: p });
                }
                let escaped = bytes[p + 1];
                if !matches!(escaped, b'"' | b'\\' | b'@') {
                    return Err(EmailError::BadEscape { at: p });
                }
                decoded.push(escaped);
                origins.push(p);
                p += 2;
            } else {
                if !(0x20..=0x7e).contains(&bytes[p]) {
                    return Err(EmailError::QuotedChar { at: p });
                }
                decoded.push(bytes[p]);
                origins.push(p);
                p += 1;
            }
        }

        if decoded.is_empty() {
            return Err(EmailError::EmptyLocal { at: close });
        }
        if decoded.len() > 64 {
            return Err(EmailError::LocalTooLong { at: origins[64] });
        }
        (String::from_utf8(decoded).unwrap(), true)
    } else {
        if local_bytes.len() > 64 {
            return Err(EmailError::LocalTooLong { at: 64 });
        }

        let mut previous_dot = false;
        for (p, &b) in local_bytes.iter().enumerate() {
            if b == b'.' {
                if p == 0 || p + 1 == local_bytes.len() || previous_dot {
                    return Err(EmailError::LocalDot { at: p });
                }
                previous_dot = true;
            } else {
                previous_dot = false;
                if !b.is_ascii_alphanumeric()
                    && !b"!#$%&'*+-/=?^_`{|}~".contains(&b)
                {
                    return Err(EmailError::LocalChar { at: p });
                }
            }
        }
        (String::from_utf8(local_bytes.to_vec()).unwrap(), false)
    };

    let domain_start = sep + 1;
    let mut domain_end = bytes.len();
    let mut removed_root_dot = None;
    if domain_start < domain_end && bytes[domain_end - 1] == b'.' {
        domain_end -= 1;
        removed_root_dot = Some(domain_end);
    }

    if domain_start == domain_end {
        return Err(EmailError::EmptyDomain { at: domain_start });
    }

    let domain_bytes = &bytes[domain_start..domain_end];
    if domain_bytes.len() > 253 {
        return Err(EmailError::DomainTooLong {
            at: domain_start + 253,
        });
    }

    let mut labels: Vec<(usize, usize)> = Vec::new();
    let mut label_start = domain_start;
    for p in domain_start..domain_end {
        if bytes[p] == b'.' {
            labels.push((label_start, p));
            label_start = p + 1;
        }
    }
    labels.push((label_start, domain_end));

    for (start, end) in &labels {
        if start == end {
            let at = if *start == domain_end {
                removed_root_dot.unwrap_or(*start)
            } else {
                *start
            };
            return Err(EmailError::EmptyLabel { at });
        }

        let len = end - start;
        if len > 63 {
            return Err(EmailError::LabelTooLong { at: start + 63 });
        }
        if bytes[*start] == b'-' {
            return Err(EmailError::LabelHyphen { at: *start });
        }
        if bytes[*end - 1] == b'-' {
            return Err(EmailError::LabelHyphen { at: end - 1 });
        }
        for p in *start..*end {
            let b = bytes[p];
            if !b.is_ascii_alphanumeric() && b != b'-' {
                return Err(EmailError::DomainChar { at: p });
            }
        }
    }

    if labels.len() < 2 {
        return Err(EmailError::DomainNeedsDot { at: domain_start });
    }

    let (last_start, last_end) = labels[labels.len() - 1];
    if bytes[last_start..last_end]
        .iter()
        .all(|b| b.is_ascii_digit())
    {
        return Err(EmailError::NumericTld { at: last_start });
    }

    let domain = bytes[domain_start..domain_end]
        .iter()
        .map(|&b| b.to_ascii_lowercase() as char)
        .collect();

    Ok(Address {
        local,
        domain,
        quoted,
    })
}

fn main() {}