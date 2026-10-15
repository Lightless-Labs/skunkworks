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
    let (local, quoted) = if local_bytes[0] == b'"' {
        let close = quoted_prefix_end.expect("quoted prefix was scanned");
        if close + 1 < sep {
            return Err(EmailError::TrailingLocal { at: close + 1 });
        }

        let mut decoded = Vec::new();
        let mut origins = Vec::new();
        let mut j = 1;
        while j < close {
            if local_bytes[j] == b'\\' {
                if j + 1 >= close {
                    return Err(EmailError::BadEscape { at: j });
                }
                let escaped = local_bytes[j + 1];
                match escaped {
                    b'"' | b'\\' | b'@' => {
                        decoded.push(escaped);
                        origins.push(j);
                        j += 2;
                    }
                    _ => return Err(EmailError::BadEscape { at: j }),
                }
            } else {
                let b = local_bytes[j];
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

        (
            String::from_utf8(decoded).expect("validated ASCII"),
            true,
        )
    } else {
        if local_bytes.len() > 64 {
            return Err(EmailError::LocalTooLong { at: 64 });
        }

        let allowed_special = b"!#$%&'*+-/=?^_`{|}~";
        for j in 0..local_bytes.len() {
            let b = local_bytes[j];
            if b == b'.' {
                if j == 0
                    || j + 1 == local_bytes.len()
                    || local_bytes[j - 1] == b'.'
                {
                    return Err(EmailError::LocalDot { at: j });
                }
            } else if !b.is_ascii_alphanumeric() && !allowed_special.contains(&b) {
                return Err(EmailError::LocalChar { at: j });
            }
        }

        (
            String::from_utf8(local_bytes.to_vec()).expect("validated ASCII"),
            false,
        )
    };

    let domain_start = sep + 1;
    let raw_domain = &bytes[domain_start..];
    let root_dot = if raw_domain.last() == Some(&b'.') {
        Some(input.len() - 1)
    } else {
        None
    };
    let domain_end = if root_dot.is_some() {
        input.len() - 1
    } else {
        input.len()
    };

    if domain_end == domain_start {
        return Err(EmailError::EmptyDomain { at: domain_start });
    }

    let domain_len = domain_end - domain_start;
    if domain_len > 253 {
        return Err(EmailError::DomainTooLong {
            at: domain_start + 253,
        });
    }

    let domain_bytes = &bytes[domain_start..domain_end];
    let mut labels: Vec<(usize, usize)> = Vec::new();
    let mut label_start = 0;
    for offset in 0..=domain_bytes.len() {
        if offset == domain_bytes.len() || domain_bytes[offset] == b'.' {
            labels.push((label_start, offset));
            label_start = offset + 1;
        }
    }

    for (start, end) in &labels {
        if start == end {
            let at = if *start == domain_bytes.len() {
                root_dot.unwrap_or(domain_end)
            } else {
                domain_start + *start
            };
            return Err(EmailError::EmptyLabel { at });
        }

        let len = end - start;
        if len > 63 {
            return Err(EmailError::LabelTooLong {
                at: domain_start + start + 63,
            });
        }

        let label = &domain_bytes[*start..*end];
        if label[0] == b'-' {
            return Err(EmailError::LabelHyphen {
                at: domain_start + start,
            });
        }
        if label[label.len() - 1] == b'-' {
            return Err(EmailError::LabelHyphen {
                at: domain_start + end - 1,
            });
        }
        for (idx, &b) in label.iter().enumerate() {
            if !b.is_ascii_alphanumeric() && b != b'-' {
                return Err(EmailError::DomainChar {
                    at: domain_start + start + idx,
                });
            }
        }
    }

    if labels.len() < 2 {
        return Err(EmailError::DomainNeedsDot { at: domain_start });
    }

    let (last_start, last_end) = labels[labels.len() - 1];
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
        .map(|b| b.to_ascii_lowercase() as char)
        .collect::<String>();

    Ok(Address {
        local,
        domain,
        quoted,
    })
}

fn main() {}