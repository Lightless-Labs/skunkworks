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
                b'\\' => i += 2,
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

    let local_bytes = &bytes[..separator];
    let (local, is_quoted) = if local_bytes[0] == b'"' {
        let close = quoted_close.expect("opening quote has a closing quote");
        if close >= separator {
            // A quote cannot protect the separator if it closes after it; the
            // separator scan above would have ignored it only through `close`.
        }
        if close + 1 != separator {
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
                match bytes[j + 1] {
                    b'"' | b'\\' | b'@' => {
                        decoded.push(bytes[j + 1]);
                        origins.push(j);
                        j += 2;
                    }
                    _ => return Err(EmailError::BadEscape { at: j }),
                }
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
        (String::from_utf8(decoded).unwrap(), true)
    } else {
        if local_bytes.len() > 64 {
            return Err(EmailError::LocalTooLong { at: 64 });
        }

        let specials = b"!#$%&'*+-/=?^_`{|}~";
        for (j, &b) in local_bytes.iter().enumerate() {
            if b == b'.' {
                if j == 0 || j + 1 == local_bytes.len() || local_bytes[j - 1] == b'.' {
                    return Err(EmailError::LocalDot { at: j });
                }
            } else if !b.is_ascii_alphanumeric() && !specials.contains(&b) {
                return Err(EmailError::LocalChar { at: j });
            }
        }
        (String::from_utf8(local_bytes.to_vec()).unwrap(), false)
    };

    let domain_start = separator + 1;
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

    let removed_dot = if has_root_dot {
        Some(domain_start + raw_domain.len() - 1)
    } else {
        None
    };
    let domain_end = domain_start + domain_len;
    let mut labels: Vec<(usize, usize)> = Vec::new();
    let mut label_start = domain_start;

    for pos in domain_start..domain_end {
        if bytes[pos] == b'.' {
            if pos == label_start {
                return Err(EmailError::EmptyLabel { at: pos });
            }
            labels.push((label_start, pos));
            label_start = pos + 1;
        }
    }

    if label_start == domain_end {
        return Err(EmailError::EmptyLabel {
            at: removed_dot.unwrap_or(domain_end),
        });
    }
    labels.push((label_start, domain_end));

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
            if !b.is_ascii_alphanumeric() && b != b'-' {
                return Err(EmailError::DomainChar { at: pos });
            }
        }
    }

    if labels.len() < 2 {
        return Err(EmailError::DomainNeedsDot { at: domain_start });
    }

    let (last_start, last_end) = *labels.last().unwrap();
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