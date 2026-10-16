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

    let quoted_close = if bytes.first() == Some(&b'"') {
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
    if separators.len() > 1 {
        return Err(EmailError::MultipleAt { at: separators[1] });
    }

    let sep = separators[0];
    if sep == 0 {
        return Err(EmailError::EmptyLocal { at: sep });
    }

    let (local, is_quoted) = if let Some(close) = quoted_close {
        if close + 1 != sep {
            return Err(EmailError::TrailingLocal { at: close + 1 });
        }

        let mut decoded = Vec::new();
        let mut origins = Vec::new();
        let mut j = 1;
        while j < close {
            if bytes[j] == b'\\' {
                if j + 1 >= close {
                    return Err(EmailError::BadEscape { at: j });
                }
                let escaped = bytes[j + 1];
                match escaped {
                    b'"' | b'\\' | b'@' => {
                        decoded.push(escaped);
                        origins.push(j);
                    }
                    _ => return Err(EmailError::BadEscape { at: j }),
                }
                j += 2;
            } else {
                if !(0x20..=0x7e).contains(&bytes[j]) {
                    return Err(EmailError::QuotedChar { at: j });
                }
                decoded.push(bytes[j]);
                origins.push(j);
                j += 1;
            }
        }

        if decoded.is_empty() {
            return Err(EmailError::EmptyLocal { at: close });
        }
        if decoded.len() > 64 {
            return Err(EmailError::LocalTooLong { at: origins[64] });
        }
        (String::from_utf8(decoded).expect("ASCII bytes are valid UTF-8"), true)
    } else {
        let local_bytes = &bytes[..sep];
        if local_bytes.len() > 64 {
            return Err(EmailError::LocalTooLong { at: 64 });
        }

        for (idx, &b) in local_bytes.iter().enumerate() {
            if b == b'.' {
                if idx == 0 || idx + 1 == local_bytes.len() || local_bytes[idx - 1] == b'.' {
                    return Err(EmailError::LocalDot { at: idx });
                }
            } else if !(b.is_ascii_alphanumeric() || b"!#$%&'*+-/=?^_`{|}~".contains(&b)) {
                return Err(EmailError::LocalChar { at: idx });
            }
        }

        (
            String::from_utf8(local_bytes.to_vec()).expect("ASCII bytes are valid UTF-8"),
            false,
        )
    };

    let domain_start = sep + 1;
    let raw_domain = &bytes[domain_start..];
    let root_dot = raw_domain.last() == Some(&b'.');
    let domain_end = if root_dot {
        bytes.len() - 1
    } else {
        bytes.len()
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

    let mut labels: Vec<(usize, usize)> = Vec::new();
    let mut label_start = domain_start;
    let mut cursor = domain_start;

    while cursor <= domain_end {
        if cursor == domain_end || bytes[cursor] == b'.' {
            if label_start == cursor {
                let at = if cursor == domain_end && root_dot {
                    domain_end
                } else {
                    label_start
                };
                return Err(EmailError::EmptyLabel { at });
            }
            labels.push((label_start, cursor));
            label_start = cursor + 1;
        }
        cursor += 1;
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

        for pos in start..end {
            let b = bytes[pos];
            if !(b.is_ascii_alphanumeric() || b == b'-') {
                return Err(EmailError::DomainChar { at: pos });
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