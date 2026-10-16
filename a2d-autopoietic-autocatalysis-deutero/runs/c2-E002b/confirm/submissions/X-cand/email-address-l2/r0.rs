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
        let mut end = None;
        while i < bytes.len() {
            match bytes[i] {
                b'\\' => {
                    i += 2;
                }
                b'"' => {
                    end = Some(i);
                    break;
                }
                _ => i += 1,
            }
        }
        match end {
            Some(i) => Some(i),
            None => return Err(EmailError::UnclosedQuote { at: 0 }),
        }
    } else {
        None
    };

    let mut separators = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if let Some(end) = quoted_end {
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
    if separators.len() >= 2 {
        return Err(EmailError::MultipleAt { at: separators[1] });
    }

    let sep = separators[0];
    if sep == 0 {
        return Err(EmailError::EmptyLocal { at: sep });
    }

    let (local, is_quoted) = if let Some(end) = quoted_end {
        if end + 1 != sep {
            return Err(EmailError::TrailingLocal { at: end + 1 });
        }

        let mut decoded = Vec::new();
        let mut origins = Vec::new();
        let mut j = 1;
        while j < end {
            if bytes[j] == b'\\' {
                if j + 1 >= end {
                    return Err(EmailError::BadEscape { at: j });
                }
                let escaped = bytes[j + 1];
                if !matches!(escaped, b'"' | b'\\' | b'@') {
                    return Err(EmailError::BadEscape { at: j });
                }
                decoded.push(escaped);
                origins.push(j);
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
            return Err(EmailError::EmptyLocal { at: end });
        }
        if decoded.len() > 64 {
            return Err(EmailError::LocalTooLong { at: origins[64] });
        }
        (String::from_utf8(decoded).unwrap(), true)
    } else {
        if sep > 64 {
            return Err(EmailError::LocalTooLong { at: 64 });
        }

        let mut previous_dot = false;
        for j in 0..sep {
            let b = bytes[j];
            if b == b'.' {
                if j == 0 || j + 1 == sep || previous_dot {
                    return Err(EmailError::LocalDot { at: j });
                }
                previous_dot = true;
            } else {
                previous_dot = false;
                let valid = b.is_ascii_alphanumeric()
                    || b"!#$%&'*+-/=?^_`{|}~".contains(&b);
                if !valid {
                    return Err(EmailError::LocalChar { at: j });
                }
            }
        }
        (input[..sep].to_owned(), false)
    };

    let domain_start = sep + 1;
    let raw_domain = &bytes[domain_start..];
    let has_root_dot = raw_domain.last() == Some(&b'.');
    let domain_len = raw_domain.len() - usize::from(has_root_dot);
    if domain_len == 0 {
        return Err(EmailError::EmptyDomain { at: domain_start });
    }
    if domain_len > 253 {
        return Err(EmailError::DomainTooLong {
            at: domain_start + 253,
        });
    }

    let domain_end = domain_start + domain_len;
    let mut label_starts = Vec::new();
    let mut label_ends = Vec::new();
    let mut start = domain_start;
    for pos in domain_start..=domain_end {
        if pos == domain_end || bytes[pos] == b'.' {
            if start == pos {
                let at = if pos == domain_end && has_root_dot {
                    input.len() - 1
                } else {
                    start
                };
                return Err(EmailError::EmptyLabel { at });
            }

            let len = pos - start;
            if len > 63 {
                return Err(EmailError::LabelTooLong { at: start + 63 });
            }

            if bytes[start] == b'-' {
                return Err(EmailError::LabelHyphen { at: start });
            }
            if bytes[pos - 1] == b'-' {
                return Err(EmailError::LabelHyphen { at: pos - 1 });
            }

            for j in start..pos {
                let b = bytes[j];
                if !(b.is_ascii_alphanumeric() || b == b'-') {
                    return Err(EmailError::DomainChar { at: j });
                }
            }

            label_starts.push(start);
            label_ends.push(pos);
            start = pos + 1;
        }
    }

    if label_starts.len() < 2 {
        return Err(EmailError::DomainNeedsDot { at: domain_start });
    }

    let last_start = *label_starts.last().unwrap();
    let last_end = *label_ends.last().unwrap();
    if bytes[last_start..last_end].iter().all(u8::is_ascii_digit) {
        return Err(EmailError::NumericTld { at: last_start });
    }

    let normalized_domain = bytes[domain_start..domain_end]
        .iter()
        .map(|&b| b.to_ascii_lowercase() as char)
        .collect::<String>();

    Ok(Address {
        local,
        domain: normalized_domain,
        quoted: is_quoted,
    })
}