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

    let quoted_end = if bytes[0] == b'"' {
        let mut i = 1;
        let mut found = None;
        while i < bytes.len() {
            match bytes[i] {
                b'\\' => {
                    i += 2;
                }
                b'"' => {
                    found = Some(i);
                    break;
                }
                _ => i += 1,
            }
        }
        match found {
            Some(end) => Some(end),
            None => return Err(EmailError::UnclosedQuote { at: 0 }),
        }
    } else {
        None
    };

    let mut separators = Vec::new();
    for i in 0..bytes.len() {
        if bytes[i] == b'@' && !matches!(quoted_end, Some(end) if i <= end) {
            separators.push(i);
        }
    }

    if separators.is_empty() {
        return Err(EmailError::MissingAt);
    }
    if separators.len() >= 2 {
        return Err(EmailError::MultipleAt {
            at: separators[1],
        });
    }

    let separator = separators[0];
    if separator == 0 {
        return Err(EmailError::EmptyLocal { at: separator });
    }

    let (local, is_quoted) = if let Some(end) = quoted_end {
        if end + 1 != separator {
            return Err(EmailError::TrailingLocal { at: end + 1 });
        }

        let mut decoded = Vec::new();
        let mut origins = Vec::new();
        let mut i = 1;
        while i < end {
            let b = bytes[i];
            if b == b'\\' {
                if i + 1 >= end {
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
                if !(0x20..=0x7e).contains(&b) {
                    return Err(EmailError::QuotedChar { at: i });
                }
                decoded.push(b);
                origins.push(i);
                i += 1;
            }
        }

        if decoded.is_empty() {
            return Err(EmailError::EmptyLocal { at: end });
        }
        if decoded.len() > 64 {
            return Err(EmailError::LocalTooLong {
                at: origins[64],
            });
        }

        (
            String::from_utf8(decoded).expect("validated ASCII"),
            true,
        )
    } else {
        let local_bytes = &bytes[..separator];
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
            } else if !b.is_ascii_alphanumeric()
                && !b"!#$%&'*+-/=?^_`{|}~".contains(&b)
            {
                return Err(EmailError::LocalChar { at: i });
            }
        }

        (
            String::from_utf8(local_bytes.to_vec()).expect("validated ASCII"),
            false,
        )
    };

    let domain_start = separator + 1;
    let raw_domain = &bytes[domain_start..];
    let remove_root = raw_domain.last() == Some(&b'.');
    let domain_end = if remove_root {
        bytes.len() - 1
    } else {
        bytes.len()
    };
    let domain_bytes = &bytes[domain_start..domain_end];

    if domain_bytes.is_empty() {
        return Err(EmailError::EmptyDomain { at: domain_start });
    }

    if domain_bytes.len() > 253 {
        return Err(EmailError::DomainTooLong {
            at: domain_start + 253,
        });
    }

    let mut labels: Vec<(usize, usize)> = Vec::new();
    let mut label_start = domain_start;
    for i in domain_start..=domain_end {
        if i == domain_end || bytes[i] == b'.' {
            if label_start == i {
                let at = if i == domain_end && remove_root {
                    domain_end
                } else {
                    i
                };
                return Err(EmailError::EmptyLabel { at });
            }
            labels.push((label_start, i));
            label_start = i + 1;
        }
    }

    for &(start, end) in &labels {
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
            if !b.is_ascii_alphanumeric() && b != b'-' {
                return Err(EmailError::DomainChar { at: i });
            }
        }
    }

    if labels.len() < 2 {
        return Err(EmailError::DomainNeedsDot { at: domain_start });
    }

    let (last_start, last_end) = labels[labels.len() - 1];
    if bytes[last_start..last_end]
        .iter()
        .all(u8::is_ascii_digit)
    {
        return Err(EmailError::NumericTld { at: last_start });
    }

    let domain = bytes[domain_start..domain_end]
        .iter()
        .map(|b| b.to_ascii_lowercase() as char)
        .collect();

    Ok(Address {
        local,
        domain,
        quoted: is_quoted,
    })
}