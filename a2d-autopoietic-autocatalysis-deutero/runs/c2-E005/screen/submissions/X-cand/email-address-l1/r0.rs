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
}

pub fn parse_email(input: &str) -> Result<Address, EmailError> {
    let bytes = input.as_bytes();

    if bytes.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    if let Some(at) = bytes.iter().position(|&b| b >= 128) {
        return Err(EmailError::NonAscii { at });
    }

    let mut separators = bytes
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| (b == b'@').then_some(i));

    let Some(separator) = separators.next() else {
        return Err(EmailError::MissingAt);
    };
    if let Some(at) = separators.next() {
        return Err(EmailError::MultipleAt { at });
    }

    if separator == 0 {
        return Err(EmailError::EmptyLocal { at: separator });
    }

    let local_bytes = &bytes[..separator];
    if local_bytes.len() > 64 {
        return Err(EmailError::LocalTooLong { at: 64 });
    }

    for (i, &byte) in local_bytes.iter().enumerate() {
        if byte == b'.' {
            if i == 0 || i + 1 == local_bytes.len() || local_bytes[i - 1] == b'.' {
                return Err(EmailError::LocalDot { at: i });
            }
        } else if !is_ascii_alphanumeric(byte)
            && !b"!#$%&'*+-/=?^_`{|}~".contains(&byte)
        {
            return Err(EmailError::LocalChar { at: i });
        }
    }

    let domain_start = separator + 1;
    let raw_domain = &bytes[domain_start..];
    let has_root_dot = raw_domain.last() == Some(&b'.');
    let domain_end = if has_root_dot {
        input.len() - 1
    } else {
        input.len()
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

    let mut label_starts = Vec::new();
    let mut label_ranges = Vec::new();
    let mut start = 0;

    for i in 0..=domain_bytes.len() {
        if i == domain_bytes.len() || domain_bytes[i] == b'.' {
            let label_start = domain_start + start;
            if start == i {
                let at = if i == domain_bytes.len() && has_root_dot {
                    input.len() - 1
                } else {
                    label_start
                };
                return Err(EmailError::EmptyLabel { at });
            }

            let label = &domain_bytes[start..i];
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
            for (offset, &byte) in label.iter().enumerate() {
                if !is_ascii_alphanumeric(byte) && byte != b'-' {
                    return Err(EmailError::DomainChar {
                        at: label_start + offset,
                    });
                }
            }

            label_starts.push(label_start);
            label_ranges.push((start, i));
            start = i + 1;
        }
    }

    if label_ranges.len() < 2 {
        return Err(EmailError::DomainNeedsDot { at: domain_start });
    }

    let (last_start, last_end) = label_ranges[label_ranges.len() - 1];
    let last_label = &domain_bytes[last_start..last_end];
    if last_label.iter().all(u8::is_ascii_digit) {
        return Err(EmailError::NumericTld {
            at: domain_start + last_start,
        });
    }

    let local = String::from_utf8(local_bytes.to_vec()).expect("ASCII input");
    let domain = domain_bytes
        .iter()
        .map(|&byte| byte.to_ascii_lowercase() as char)
        .collect();

    Ok(Address {
        local,
        domain,
        quoted: false,
    })
}

fn is_ascii_alphanumeric(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
}