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
    if separators.len() >= 2 {
        return Err(EmailError::MultipleAt { at: separators[1] });
    }

    let separator = separators[0];
    if separator == 0 {
        return Err(EmailError::EmptyLocal { at: separator });
    }

    let (local, is_quoted) = if let Some(end_quote) = quoted_prefix_end {
        if end_quote + 1 != separator {
            return Err(EmailError::TrailingLocal { at: end_quote + 1 });
        }

        let mut decoded = Vec::new();
        let mut origins = Vec::new();
        let mut i = 1;
        while i < end_quote {
            let b = bytes[i];
            if b == b'\\' {
                if i + 1 >= end_quote {
                    return Err(EmailError::BadEscape { at: i });
                }
                let escaped = bytes[i + 1];
                if !matches!(escaped, b'"' | b'\\' | b'@') {
                    return Err(EmailError::BadEscape { at: i });
                }
                decoded.push(escaped);
                origins.push(i);
                i += 2;
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
            return Err(EmailError::EmptyLocal { at: end_quote });
        }
        if decoded.len() > 64 {
            return Err(EmailError::LocalTooLong { at: origins[64] });
        }
        (String::from_utf8(decoded).unwrap(), true)
    } else {
        if separator > 64 {
            return Err(EmailError::LocalTooLong { at: 64 });
        }

        let mut previous_dot = false;
        for (i, &b) in bytes[..separator].iter().enumerate() {
            if b == b'.' {
                if i == 0 || i + 1 == separator || previous_dot {
                    return Err(EmailError::LocalDot { at: i });
                }
                previous_dot = true;
            } else {
                previous_dot = false;
                if !b.is_ascii_alphanumeric()
                    && !b"!#$%&'*+-/=?^_`{|}~".contains(&b)
                {
                    return Err(EmailError::LocalChar { at: i });
                }
            }
        }
        (input[..separator].to_string(), false)
    };

    let domain_start = separator + 1;
    let mut domain_end = bytes.len();
    let had_root_dot = domain_end > domain_start && bytes[domain_end - 1] == b'.';
    if had_root_dot {
        domain_end -= 1;
    }

    if domain_start == domain_end {
        return Err(EmailError::EmptyDomain { at: domain_start });
    }
    if domain_end - domain_start > 253 {
        return Err(EmailError::DomainTooLong {
            at: domain_start + 253,
        });
    }

    let mut labels: Vec<(usize, usize)> = Vec::new();
    let mut label_start = domain_start;
    for i in domain_start..domain_end {
        if bytes[i] == b'.' {
            if i == label_start {
                return Err(EmailError::EmptyLabel { at: i });
            }
            labels.push((label_start, i));
            label_start = i + 1;
        }
    }
    if label_start == domain_end {
        let at = if had_root_dot { domain_end } else { domain_end };
        return Err(EmailError::EmptyLabel { at });
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

    let (last_start, last_end) = *labels.last().unwrap();
    if bytes[last_start..last_end].iter().all(u8::is_ascii_digit) {
        return Err(EmailError::NumericTld { at: last_start });
    }

    let normalized_domain = input[domain_start..domain_end].to_ascii_lowercase();
    Ok(Address {
        local,
        domain: normalized_domain,
        quoted: is_quoted,
    })
}

fn main() {}