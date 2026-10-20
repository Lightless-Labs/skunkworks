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

    let mut quoted_close = None;
    if bytes[0] == b'"' {
        let mut i = 1;
        while i < bytes.len() {
            match bytes[i] {
                b'\\' => {
                    i += 2;
                }
                b'"' => {
                    quoted_close = Some(i);
                    break;
                }
                _ => i += 1,
            }
        }
        if quoted_close.is_none() {
            return Err(EmailError::UnclosedQuote { at: 0 });
        }
    }

    let mut separators = Vec::new();
    let protected_end = quoted_close.map(|i| i + 1).unwrap_or(0);
    for i in 0..bytes.len() {
        if i >= protected_end && bytes[i] == b'@' {
            separators.push(i);
        }
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

    let (local, quoted) = if bytes[0] == b'"' {
        let close = quoted_close.unwrap();
        if close + 1 != sep {
            return Err(EmailError::TrailingLocal { at: close + 1 });
        }

        let mut decoded = Vec::new();
        let mut origins = Vec::new();
        let mut i = 1;
        while i < close {
            if bytes[i] == b'\\' {
                if i + 1 >= close {
                    return Err(EmailError::BadEscape { at: i });
                }
                let escaped = bytes[i + 1];
                match escaped {
                    b'"' | b'\\' | b'@' => {
                        decoded.push(escaped);
                        origins.push(i);
                        i += 2;
                    }
                    _ => return Err(EmailError::BadEscape { at: i }),
                }
            } else {
                let b = bytes[i];
                if !(0x20..=0x7e).contains(&b) {
                    return Err(EmailError::QuotedChar { at: i });
                }
                decoded.push(b);
                origins.push(i);
                i += 1;
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
        let local_bytes = &bytes[..sep];
        if local_bytes.len() > 64 {
            return Err(EmailError::LocalTooLong { at: 64 });
        }

        for i in 0..local_bytes.len() {
            let b = local_bytes[i];
            if b == b'.' {
                if i == 0
                    || i + 1 == local_bytes.len()
                    || local_bytes[i - 1] == b'.'
                {
                    return Err(EmailError::LocalDot { at: i });
                }
            } else if !(b.is_ascii_alphanumeric()
                || b"!#$%&'*+-/=?^_`{|}~".contains(&b))
            {
                return Err(EmailError::LocalChar { at: i });
            }
        }

        (input[..sep].to_owned(), false)
    };

    let domain_start = sep + 1;
    let raw_domain = &bytes[domain_start..];
    let root_dot = if raw_domain.last() == Some(&b'.') {
        Some(bytes.len() - 1)
    } else {
        None
    };
    let domain_end = root_dot.unwrap_or(bytes.len());

    if domain_start == domain_end {
        return Err(EmailError::EmptyDomain { at: domain_start });
    }

    if domain_end - domain_start > 253 {
        return Err(EmailError::DomainTooLong {
            at: domain_start + 253,
        });
    }

    let domain_bytes = &bytes[domain_start..domain_end];
    let mut labels: Vec<(usize, usize)> = Vec::new();
    let mut label_start = domain_start;

    for (offset, &b) in domain_bytes.iter().enumerate() {
        if b == b'.' {
            labels.push((label_start, domain_start + offset));
            label_start = domain_start + offset + 1;
        }
    }
    labels.push((label_start, domain_end));

    for (index, &(start, end)) in labels.iter().enumerate() {
        if start == end {
            let at = if index + 1 == labels.len() {
                root_dot.unwrap_or(start)
            } else {
                start
            };
            return Err(EmailError::EmptyLabel { at });
        }

        let len = end - start;
        if len > 63 {
            return Err(EmailError::LabelTooLong { at: start + 63 });
        }
        if bytes[start] == b'-' {
            return Err(EmailError::LabelHyphen { at: start });
        }
        if bytes[end - 1] == b'-' {
            return Err(EmailError::LabelHyphen { at: end - 1 });
        }
        for i in start..end {
            let b = bytes[i];
            if !(b.is_ascii_alphanumeric() || b == b'-') {
                return Err(EmailError::DomainChar { at: i });
            }
        }
    }

    if labels.len() < 2 {
        return Err(EmailError::DomainNeedsDot { at: domain_start });
    }

    let (last_start, last_end) = labels[labels.len() - 1];
    if bytes[last_start..last_end].iter().all(u8::is_ascii_digit) {
        return Err(EmailError::NumericTld { at: last_start });
    }

    let domain = input[domain_start..domain_end].to_ascii_lowercase();
    Ok(Address {
        local,
        domain,
        quoted,
    })
}