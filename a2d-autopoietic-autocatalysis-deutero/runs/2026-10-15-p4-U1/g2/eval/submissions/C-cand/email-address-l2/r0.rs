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

    let mut quoted_close = None;
    if bytes[0] == b'"' {
        let mut i = 1;
        while i < bytes.len() {
            match bytes[i] {
                b'\\' => i += 2,
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

    let separator = separators[0];
    if separator == 0 {
        return Err(EmailError::EmptyLocal { at: separator });
    }

    let (local, quoted) = if let Some(close) = quoted_close {
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
                let escaped = bytes[j + 1];
                match escaped {
                    b'"' | b'\\' | b'@' => {
                        decoded.push(escaped);
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
        let local_bytes = &bytes[..separator];

        if local_bytes.len() > 64 {
            return Err(EmailError::LocalTooLong { at: 64 });
        }

        for (idx, &byte) in local_bytes.iter().enumerate() {
            if byte == b'.' {
                if idx == 0
                    || idx + 1 == local_bytes.len()
                    || local_bytes[idx - 1] == b'.'
                {
                    return Err(EmailError::LocalDot { at: idx });
                }
            } else if !byte.is_ascii_alphanumeric()
                && !b"!#$%&'*+-/=?^_`{|}~".contains(&byte)
            {
                return Err(EmailError::LocalChar { at: idx });
            }
        }

        (String::from_utf8(local_bytes.to_vec()).unwrap(), false)
    };

    let domain_start = separator + 1;
    let raw_domain = &bytes[domain_start..];
    let mut domain_end = bytes.len();
    if domain_end > domain_start && bytes[domain_end - 1] == b'.' {
        domain_end -= 1;
    }

    if domain_end == domain_start {
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
    for pos in domain_start..domain_end {
        if bytes[pos] == b'.' {
            labels.push((label_start, pos));
            label_start = pos + 1;
        }
    }
    labels.push((label_start, domain_end));

    for &(start, end) in &labels {
        if start == end {
            let at = if start == domain_end && domain_end < bytes.len() {
                domain_end
            } else {
                start
            };
            return Err(EmailError::EmptyLabel { at });
        }

        if end - start > 63 {
            return Err(EmailError::LabelTooLong { at: start + 63 });
        }

        if bytes[start] == b'-' {
            return Err(EmailError::LabelHyphen { at: start });
        }
        if bytes[end - 1] == b'-' {
            return Err(EmailError::LabelHyphen { at: end - 1 });
        }

        for pos in start..end {
            let byte = bytes[pos];
            if !byte.is_ascii_alphanumeric() && byte != b'-' {
                return Err(EmailError::DomainChar { at: pos });
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

    let domain = String::from_utf8(
        domain_bytes.iter().map(u8::to_ascii_lowercase).collect(),
    )
    .unwrap();

    let _ = raw_domain;

    Ok(Address {
        local,
        domain,
        quoted,
    })
}

fn main() {}