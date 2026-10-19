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

    if let Some(at) = bytes.iter().position(|&byte| byte >= 128) {
        return Err(EmailError::NonAscii { at });
    }

    let mut separators = bytes
        .iter()
        .enumerate()
        .filter_map(|(index, &byte)| (byte == b'@').then_some(index));

    let separator = match separators.next() {
        Some(index) => index,
        None => return Err(EmailError::MissingAt),
    };

    if let Some(second) = separators.next() {
        return Err(EmailError::MultipleAt { at: second });
    }

    if separator == 0 {
        return Err(EmailError::EmptyLocal { at: separator });
    }

    let local_bytes = &bytes[..separator];
    if local_bytes.len() > 64 {
        return Err(EmailError::LocalTooLong { at: 64 });
    }

    let allowed_special = b"!#$%&'*+-/=?^_`{|}~";
    for (index, &byte) in local_bytes.iter().enumerate() {
        let original_at = index;
        if byte == b'.' {
            if index == 0 || index + 1 == local_bytes.len() || local_bytes[index - 1] == b'.' {
                return Err(EmailError::LocalDot { at: original_at });
            }
        } else if !byte.is_ascii_alphanumeric() && !allowed_special.contains(&byte) {
            return Err(EmailError::LocalChar { at: original_at });
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

    if domain_start == domain_end {
        return Err(EmailError::EmptyDomain { at: domain_start });
    }

    let domain_bytes = &bytes[domain_start..domain_end];
    if domain_bytes.len() > 253 {
        return Err(EmailError::DomainTooLong {
            at: domain_start + 253,
        });
    }

    let mut labels: Vec<(usize, &[u8])> = Vec::new();
    let mut label_start = domain_start;

    for index in domain_start..=domain_end {
        if index == domain_end || bytes[index] == b'.' {
            let label = &bytes[label_start..index];
            if label.is_empty() {
                let at = if index == domain_end && has_root_dot {
                    domain_end
                } else {
                    label_start
                };
                return Err(EmailError::EmptyLabel { at });
            }

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
                if !byte.is_ascii_alphanumeric() && byte != b'-' {
                    return Err(EmailError::DomainChar {
                        at: label_start + offset,
                    });
                }
            }

            labels.push((label_start, label));
            label_start = index + 1;
        }
    }

    if labels.len() < 2 {
        return Err(EmailError::DomainNeedsDot { at: domain_start });
    }

    let (last_start, last_label) = labels[labels.len() - 1];
    if last_label.iter().all(u8::is_ascii_digit) {
        return Err(EmailError::NumericTld { at: last_start });
    }

    let local = String::from_utf8(local_bytes.to_vec()).expect("ASCII input");
    let domain = String::from_utf8(domain_bytes.to_vec())
        .expect("ASCII input")
        .to_ascii_lowercase();

    Ok(Address {
        local,
        domain,
        quoted: false,
    })
}