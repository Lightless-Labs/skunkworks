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

    if let Some(at) = bytes.iter().position(|&byte| byte >= 128) {
        return Err(EmailError::NonAscii { at });
    }

    let quoted_prefix_end = if bytes[0] == b'"' {
        let mut i = 1;
        let mut closing = None;

        while i < bytes.len() {
            match bytes[i] {
                b'\\' => i += 2,
                b'"' => {
                    closing = Some(i);
                    break;
                }
                _ => i += 1,
            }
        }

        match closing {
            Some(at) => Some(at),
            None => return Err(EmailError::UnclosedQuote { at: 0 }),
        }
    } else {
        None
    };

    let mut separators = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if let Some(end) = quoted_prefix_end {
            if i <= end {
                i = end + 1;
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
    if separators.len() > 1 {
        return Err(EmailError::MultipleAt { at: separators[1] });
    }

    let separator = separators[0];
    if separator == 0 {
        return Err(EmailError::EmptyLocal { at: separator });
    }

    let local_bytes = &bytes[..separator];
    let (local, quoted) = if local_bytes[0] == b'"' {
        let closing = quoted_prefix_end.expect("quoted prefix has a closing quote");

        if closing + 1 != separator {
            return Err(EmailError::TrailingLocal { at: closing + 1 });
        }

        let mut decoded = Vec::new();
        let mut origins = Vec::new();
        let mut pos = 1;

        while pos < closing {
            let byte = bytes[pos];
            if byte == b'\\' {
                if pos + 1 >= closing {
                    return Err(EmailError::BadEscape { at: pos });
                }
                let escaped = bytes[pos + 1];
                match escaped {
                    b'"' | b'\\' | b'@' => {
                        decoded.push(escaped);
                        origins.push(pos);
                    }
                    _ => return Err(EmailError::BadEscape { at: pos }),
                }
                pos += 2;
            } else {
                if !(0x20..=0x7e).contains(&byte) {
                    return Err(EmailError::QuotedChar { at: pos });
                }
                decoded.push(byte);
                origins.push(pos);
                pos += 1;
            }
        }

        if decoded.is_empty() {
            return Err(EmailError::EmptyLocal { at: closing });
        }
        if decoded.len() > 64 {
            return Err(EmailError::LocalTooLong { at: origins[64] });
        }

        (
            String::from_utf8(decoded).expect("decoded quoted local is ASCII"),
            true,
        )
    } else {
        if local_bytes.len() > 64 {
            return Err(EmailError::LocalTooLong { at: 64 });
        }

        for (pos, &byte) in local_bytes.iter().enumerate() {
            if byte == b'.' {
                if pos == 0
                    || pos + 1 == local_bytes.len()
                    || local_bytes[pos - 1] == b'.'
                {
                    return Err(EmailError::LocalDot { at: pos });
                }
            } else if !byte.is_ascii_alphanumeric()
                && !b"!#$%&'*+-/=?^_`{|}~".contains(&byte)
            {
                return Err(EmailError::LocalChar { at: pos });
            }
        }

        (
            String::from_utf8(local_bytes.to_vec()).expect("local bytes are ASCII"),
            false,
        )
    };

    let domain_start = separator + 1;
    let mut domain_end = bytes.len();
    let root_dot = if domain_start < domain_end && bytes[domain_end - 1] == b'.' {
        domain_end -= 1;
        Some(domain_end)
    } else {
        None
    };

    if domain_start == domain_end {
        return Err(EmailError::EmptyDomain { at: domain_start });
    }

    let domain_len = domain_end - domain_start;
    if domain_len > 253 {
        return Err(EmailError::DomainTooLong {
            at: domain_start + 253,
        });
    }

    let domain_bytes = &bytes[domain_start..domain_end];
    let mut label_starts = Vec::new();
    let mut label_ends = Vec::new();
    let mut label_start = 0;

    for offset in 0..=domain_bytes.len() {
        if offset == domain_bytes.len() || domain_bytes[offset] == b'.' {
            if offset == label_start {
                let error_at = if offset == domain_bytes.len() {
                    root_dot.unwrap_or(domain_start + offset)
                } else {
                    domain_start + offset
                };
                return Err(EmailError::EmptyLabel { at: error_at });
            }

            label_starts.push(label_start);
            label_ends.push(offset);

            let label_len = offset - label_start;
            if label_len > 63 {
                return Err(EmailError::LabelTooLong {
                    at: domain_start + label_start + 63,
                });
            }

            let first = domain_bytes[label_start];
            let last = domain_bytes[offset - 1];
            if first == b'-' {
                return Err(EmailError::LabelHyphen {
                    at: domain_start + label_start,
                });
            }
            if last == b'-' {
                return Err(EmailError::LabelHyphen {
                    at: domain_start + offset - 1,
                });
            }

            for (relative, &byte) in domain_bytes[label_start..offset].iter().enumerate() {
                if !byte.is_ascii_alphanumeric() && byte != b'-' {
                    return Err(EmailError::DomainChar {
                        at: domain_start + label_start + relative,
                    });
                }
            }

            label_start = offset + 1;
        }
    }

    if label_starts.len() < 2 {
        return Err(EmailError::DomainNeedsDot { at: domain_start });
    }

    let last_start = *label_starts.last().expect("at least one label");
    let last_end = *label_ends.last().expect("at least one label");
    if domain_bytes[last_start..last_end]
        .iter()
        .all(u8::is_ascii_digit)
    {
        return Err(EmailError::NumericTld {
            at: domain_start + last_start,
        });
    }

    let domain = domain_bytes
        .iter()
        .map(|byte| byte.to_ascii_lowercase() as char)
        .collect();

    Ok(Address {
        local,
        domain,
        quoted,
    })
}