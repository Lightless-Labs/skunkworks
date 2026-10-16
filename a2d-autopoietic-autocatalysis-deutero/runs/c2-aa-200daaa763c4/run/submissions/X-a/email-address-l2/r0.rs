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

    let quoted_prefix = if bytes[0] == b'"' {
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
        if let Some(end) = quoted_prefix {
            if i <= end {
                i = end + 1;
                continue;
            }
        }
        if bytes[i] == b'@' {
            separators.push(i);
            if separators.len() == 2 {
                return Err(EmailError::MultipleAt { at: i });
            }
        }
        i += 1;
    }

    let separator = match separators.first() {
        Some(&at) => at,
        None => return Err(EmailError::MissingAt),
    };

    if separator == 0 {
        return Err(EmailError::EmptyLocal { at: separator });
    }

    let (local, quoted) = if let Some(end_quote) = quoted_prefix.filter(|_| bytes[0] == b'"') {
        if end_quote + 1 != separator {
            return Err(EmailError::TrailingLocal { at: end_quote + 1 });
        }

        let mut decoded = Vec::new();
        let mut origins = Vec::new();
        let mut j = 1;
        while j < end_quote {
            if bytes[j] == b'\\' {
                let next = j + 1;
                if next >= end_quote
                    || !matches!(bytes[next], b'"' | b'\\' | b'@')
                {
                    return Err(EmailError::BadEscape { at: j });
                }
                decoded.push(bytes[next]);
                origins.push(j);
                j += 2;
            } else {
                let b = bytes[j];
                if !(0x20..=0x7e).contains(&b) {
                    return Err(EmailError::QuotedChar { at: j });
                }
                decoded.push(b);
                origins.push(j);
                j += 1;
            }
        }

        if decoded.is_empty() {
            return Err(EmailError::EmptyLocal { at: end_quote });
        }
        if decoded.len() > 64 {
            return Err(EmailError::LocalTooLong { at: origins[64] });
        }

        (String::from_utf8(decoded).expect("ASCII local"), true)
    } else {
        let local_bytes = &bytes[..separator];
        if local_bytes.len() > 64 {
            return Err(EmailError::LocalTooLong { at: 64 });
        }

        for (j, &b) in local_bytes.iter().enumerate() {
            if b == b'.' {
                if j == 0 || j + 1 == local_bytes.len() || local_bytes[j - 1] == b'.' {
                    return Err(EmailError::LocalDot { at: j });
                }
            } else if !b.is_ascii_alphanumeric()
                && !b"!#$%&'*+-/=?^_`{|}~".contains(&b)
            {
                return Err(EmailError::LocalChar { at: j });
            }
        }

        (
            String::from_utf8(local_bytes.to_vec()).expect("ASCII local"),
            false,
        )
    };

    let domain_start = separator + 1;
    let raw_domain = &bytes[domain_start..];
    let removed_root_dot = raw_domain.last() == Some(&b'.');
    let domain_len = raw_domain.len() - usize::from(removed_root_dot);

    if domain_len == 0 {
        return Err(EmailError::EmptyDomain { at: domain_start });
    }
    if domain_len > 253 {
        return Err(EmailError::DomainTooLong {
            at: domain_start + 253,
        });
    }

    let domain_bytes = &raw_domain[..domain_len];
    let mut labels: Vec<(usize, usize)> = Vec::new();
    let mut label_start = 0;

    for i in 0..=domain_len {
        if i == domain_len || domain_bytes[i] == b'.' {
            let label_end = i;
            if label_start == label_end {
                let at = if label_start == domain_len && removed_root_dot {
                    input.len() - 1
                } else {
                    domain_start + label_start
                };
                return Err(EmailError::EmptyLabel { at });
            }

            let label = &domain_bytes[label_start..label_end];
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
                    at: domain_start + label_end - 1,
                });
            }
            for (offset, &b) in label.iter().enumerate() {
                if !b.is_ascii_alphanumeric() && b != b'-' {
                    return Err(EmailError::DomainChar {
                        at: domain_start + label_start + offset,
                    });
                }
            }

            labels.push((label_start, label_end));
            label_start = i + 1;
        }
    }

    if labels.len() < 2 {
        return Err(EmailError::DomainNeedsDot { at: domain_start });
    }

    let (last_start, last_end) = labels[labels.len() - 1];
    let last_label = &domain_bytes[last_start..last_end];
    if last_label.iter().all(u8::is_ascii_digit) {
        return Err(EmailError::NumericTld {
            at: domain_start + last_start,
        });
    }

    let domain = String::from_utf8(domain_bytes.to_vec())
        .expect("ASCII domain")
        .to_ascii_lowercase();

    Ok(Address {
        local,
        domain,
        quoted,
    })
}