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
            Some(pos) => Some(pos),
            None => return Err(EmailError::UnclosedQuote { at: 0 }),
        }
    } else {
        None
    };

    let mut first_at = None;
    let mut second_at = None;
    let mut i = quoted_close.map_or(0, |close| close + 1);

    while i < bytes.len() {
        if bytes[i] == b'@' {
            if first_at.is_none() {
                first_at = Some(i);
            } else {
                second_at = Some(i);
                break;
            }
        }
        i += 1;
    }

    let sep = match first_at {
        Some(pos) => pos,
        None => return Err(EmailError::MissingAt),
    };

    if let Some(pos) = second_at {
        return Err(EmailError::MultipleAt { at: pos });
    }

    if sep == 0 {
        return Err(EmailError::EmptyLocal { at: sep });
    }

    let quoted = quoted_close.is_some();
    let local = if let Some(close) = quoted_close {
        if close + 1 != sep {
            return Err(EmailError::TrailingLocal { at: close + 1 });
        }

        let mut decoded = Vec::with_capacity(close.saturating_sub(1));
        let mut origins = Vec::with_capacity(close.saturating_sub(1));
        let mut j = 1;

        while j < close {
            let b = bytes[j];
            if b == b'\\' {
                if j + 1 >= close {
                    return Err(EmailError::BadEscape { at: j });
                }

                let escaped = bytes[j + 1];
                if matches!(escaped, b'"' | b'\\' | b'@') {
                    decoded.push(escaped);
                    origins.push(j);
                    j += 2;
                } else {
                    return Err(EmailError::BadEscape { at: j });
                }
            } else {
                if !(0x20..=0x7e).contains(&b) {
                    return Err(EmailError::QuotedChar { at: j });
                }
                decoded.push(b);
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

        String::from_utf8(decoded).expect("decoded bytes are printable ASCII")
    } else {
        let local_bytes = &bytes[..sep];

        if local_bytes.len() > 64 {
            return Err(EmailError::LocalTooLong { at: 64 });
        }

        let mut previous_dot = false;
        for (idx, &b) in local_bytes.iter().enumerate() {
            if b == b'.' {
                if idx == 0 || idx + 1 == local_bytes.len() || previous_dot {
                    return Err(EmailError::LocalDot { at: idx });
                }
                previous_dot = true;
            } else {
                previous_dot = false;
                if !(b.is_ascii_alphanumeric()
                    || b"!#$%&'*+-/=?^_`{|}~".contains(&b))
                {
                    return Err(EmailError::LocalChar { at: idx });
                }
            }
        }

        String::from_utf8(local_bytes.to_vec()).expect("local bytes are ASCII")
    };

    let domain_start = sep + 1;
    let has_root_dot = bytes.last() == Some(&b'.');
    let domain_end = if has_root_dot {
        bytes.len() - 1
    } else {
        bytes.len()
    };

    if domain_end == domain_start {
        return Err(EmailError::EmptyDomain { at: domain_start });
    }

    let domain_bytes = &bytes[domain_start..domain_end];
    if domain_bytes.len() > 253 {
        return Err(EmailError::DomainTooLong {
            at: domain_start + 253,
        });
    }

    let mut labels = Vec::new();
    let mut start = domain_start;

    for pos in domain_start..=domain_end {
        if pos == domain_end || bytes[pos] == b'.' {
            if start == pos {
                let at = if pos == domain_end && has_root_dot {
                    domain_end
                } else {
                    start
                };
                return Err(EmailError::EmptyLabel { at });
            }

            let label = &bytes[start..pos];

            if label.len() > 63 {
                return Err(EmailError::LabelTooLong { at: start + 63 });
            }
            if label[0] == b'-' {
                return Err(EmailError::LabelHyphen { at: start });
            }
            if label[label.len() - 1] == b'-' {
                return Err(EmailError::LabelHyphen { at: pos - 1 });
            }

            for (offset, &b) in label.iter().enumerate() {
                if !(b.is_ascii_alphanumeric() || b == b'-') {
                    return Err(EmailError::DomainChar { at: start + offset });
                }
            }

            labels.push((start, pos));
            start = pos + 1;
        }
    }

    if labels.len() < 2 {
        return Err(EmailError::DomainNeedsDot { at: domain_start });
    }

    let (last_start, last_end) = labels[labels.len() - 1];
    if bytes[last_start..last_end].iter().all(u8::is_ascii_digit) {
        return Err(EmailError::NumericTld { at: last_start });
    }

    let domain = domain_bytes
        .iter()
        .map(|&b| (b as char).to_ascii_lowercase())
        .collect();

    Ok(Address {
        local,
        domain,
        quoted,
    })
}