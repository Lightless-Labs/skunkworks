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
            Some(i) => Some(i),
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
    let sep = separators[0];
    if sep == 0 {
        return Err(EmailError::EmptyLocal { at: sep });
    }

    let local_bytes = &bytes[..sep];
    let local = if local_bytes[0] == b'"' {
        let close = quoted_prefix_end.expect("initial quote has a closing quote");
        if close + 1 != sep {
            return Err(EmailError::TrailingLocal { at: close + 1 });
        }
        let mut decoded = Vec::new();
        let mut origins = Vec::new();
        let mut p = 1;
        while p < close {
            if local_bytes[p] == b'\\' {
                if p + 1 >= close {
                    return Err(EmailError::BadEscape { at: p });
                }
                let escaped = local_bytes[p + 1];
                if !matches!(escaped, b'"' | b'\\' | b'@') {
                    return Err(EmailError::BadEscape { at: p });
                }
                decoded.push(escaped);
                origins.push(p);
                p += 2;
            } else {
                let b = local_bytes[p];
                if !(0x20..=0x7e).contains(&b) {
                    return Err(EmailError::QuotedChar { at: p });
                }
                decoded.push(b);
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
        String::from_utf8(decoded).expect("validated ASCII")
    } else {
        if local_bytes.len() > 64 {
            return Err(EmailError::LocalTooLong { at: 64 });
        }
        for (p, &b) in local_bytes.iter().enumerate() {
            if b == b'.' {
                if p == 0
                    || p + 1 == local_bytes.len()
                    || local_bytes[p - 1] == b'.'
                {
                    return Err(EmailError::LocalDot { at: p });
                }
            } else if !b.is_ascii_alphanumeric()
                && !b"!#$%&'*+-/=?^_`{|}~".contains(&b)
            {
                return Err(EmailError::LocalChar { at: p });
            }
        }
        String::from_utf8(local_bytes.to_vec()).expect("validated ASCII")
    };

    let domain_start = sep + 1;
    let raw_domain = &bytes[domain_start..];
    let removed_dot = raw_domain.last() == Some(&b'.');
    let domain_end = if removed_dot {
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

    let mut label_count = 0usize;
    let mut label_start = 0usize;
    let mut last_label = &domain_bytes[0..0];
    for boundary in 0..=domain_bytes.len() {
        if boundary != domain_bytes.len() && domain_bytes[boundary] != b'.' {
            continue;
        }

        let label = &domain_bytes[label_start..boundary];
        if label.is_empty() {
            let at = if label_start == domain_bytes.len() && removed_dot {
                bytes.len() - 1
            } else {
                domain_start + label_start
            };
            return Err(EmailError::EmptyLabel { at });
        }
        if label.len() > 63 {
            return Err(EmailError::LabelTooLong {
                at: domain_start + label_start + 63,
            });
        }
        if label[0] == b'-' {
            return Err(EmailError::LabelHyphen {
                at: domain_start + label_start,
            });
        }
        if label[label.len() - 1] == b'-' {
            return Err(EmailError::LabelHyphen {
                at: domain_start + boundary - 1,
            });
        }
        for (offset, &b) in label.iter().enumerate() {
            if !b.is_ascii_alphanumeric() && b != b'-' {
                return Err(EmailError::DomainChar {
                    at: domain_start + label_start + offset,
                });
            }
        }

        label_count += 1;
        last_label = label;
        label_start = boundary + 1;
    }

    if label_count < 2 {
        return Err(EmailError::DomainNeedsDot { at: domain_start });
    }
    if last_label.iter().all(u8::is_ascii_digit) {
        return Err(EmailError::NumericTld {
            at: domain_start + domain_bytes.len() - last_label.len(),
        });
    }

    let domain = domain_bytes
        .iter()
        .map(|b| b.to_ascii_lowercase() as char)
        .collect();

    Ok(Address {
        local,
        domain,
        quoted: local_bytes[0] == b'"',
    })
}
