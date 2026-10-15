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
                b'\\' => {
                    i += 2;
                }
                b'"' => {
                    closing = Some(i);
                    break;
                }
                _ => i += 1,
            }
        }

        match closing {
            Some(end) => Some(end),
            None => return Err(EmailError::UnclosedQuote { at: 0 }),
        }
    } else {
        None
    };

    let mut separators = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if quoted_prefix_end.is_some_and(|end| i <= end) {
            i = quoted_prefix_end.unwrap() + 1;
            continue;
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

    let separator = separators[0];
    if separator == 0 {
        return Err(EmailError::EmptyLocal { at: separator });
    }

    let (local, quoted) = if let Some(end) = quoted_prefix_end {
        if end + 1 != separator {
            return Err(EmailError::TrailingLocal { at: end + 1 });
        }

        let mut decoded = Vec::new();
        let mut origins = Vec::new();
        let mut pos = 1;

        while pos < end {
            let byte = bytes[pos];
            if byte == b'\\' {
                if pos + 1 >= end {
                    return Err(EmailError::BadEscape { at: pos });
                }
                let escaped = bytes[pos + 1];
                match escaped {
                    b'"' | b'\\' | b'@' => {
                        decoded.push(escaped);
                        origins.push(pos);
                        pos += 2;
                    }
                    _ => return Err(EmailError::BadEscape { at: pos }),
                }
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
            return Err(EmailError::EmptyLocal { at: end });
        }
        if decoded.len() > 64 {
            return Err(EmailError::LocalTooLong { at: origins[64] });
        }

        (
            String::from_utf8(decoded).expect("validated ASCII bytes"),
            true,
        )
    } else {
        if separator > 64 {
            return Err(EmailError::LocalTooLong { at: 64 });
        }

        let mut previous_dot = false;
        for pos in 0..separator {
            let byte = bytes[pos];
            if byte == b'.' {
                if pos == 0 || pos + 1 == separator || previous_dot {
                    return Err(EmailError::LocalDot { at: pos });
                }
                previous_dot = true;
            } else {
                previous_dot = false;
                let allowed = byte.is_ascii_alphanumeric()
                    || b"!#$%&'*+-/=?^_`{|}~".contains(&byte);
                if !allowed {
                    return Err(EmailError::LocalChar { at: pos });
                }
            }
        }

        (
            String::from_utf8(bytes[..separator].to_vec()).expect("ASCII input"),
            false,
        )
    };

    let domain_start = separator + 1;
    let mut domain_end = bytes.len();
    let removed_root_dot = if domain_end > domain_start && bytes[domain_end - 1] == b'.' {
        domain_end -= 1;
        Some(domain_end)
    } else {
        None
    };

    if domain_start == domain_end {
        return Err(EmailError::EmptyDomain { at: domain_start });
    }

    let domain_bytes = &bytes[domain_start..domain_end];
    if domain_bytes.len() > 253 {
        return Err(EmailError::DomainTooLong {
            at: domain_start + 253,
        });
    }

    let mut label_starts = Vec::new();
    let mut label_ranges = Vec::new();
    let mut start = 0;

    for offset in 0..=domain_bytes.len() {
        if offset == domain_bytes.len() || domain_bytes[offset] == b'.' {
            let label_start = domain_start + start;
            if start == offset {
                let error_at = if offset == domain_bytes.len() {
                    removed_root_dot.unwrap_or(label_start)
                } else {
                    domain_start + offset
                };
                return Err(EmailError::EmptyLabel { at: error_at });
            }

            let label = &domain_bytes[start..offset];
            if label.len() > 63 {
                return Err(EmailError::LabelTooLong {
                    at: label_start + 63,
                });
            }
            if label[0] == b'-' {
                return Err(EmailError::LabelHyphen { at: label_start });
            }
            if label[label.len() - 1] == b'-' {
                return Err(EmailError::LabelHyphen {
                    at: label_start + label.len() - 1,
                });
            }
            if let Some(bad) = label
                .iter()
                .position(|&byte| !byte.is_ascii_alphanumeric() && byte != b'-')
            {
                return Err(EmailError::DomainChar {
                    at: label_start + bad,
                });
            }

            label_starts.push(label_start);
            label_ranges.push((start, offset));
            start = offset + 1;
        }
    }

    if label_ranges.len() < 2 {
        return Err(EmailError::DomainNeedsDot { at: domain_start });
    }

    let (last_start, last_end) = *label_ranges.last().unwrap();
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
        .map(u8::to_ascii_lowercase)
        .collect::<Vec<_>>();
    let domain = String::from_utf8(domain).expect("validated ASCII bytes");

    Ok(Address {
        local,
        domain,
        quoted,
    })
}

fn main() {}