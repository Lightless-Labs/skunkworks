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

    let mut previous_was_dot = false;
    for (index, &byte) in local_bytes.iter().enumerate() {
        if byte == b'.' {
            if index == 0 || index + 1 == local_bytes.len() || previous_was_dot {
                return Err(EmailError::LocalDot { at: index });
            }
            previous_was_dot = true;
        } else {
            previous_was_dot = false;
            let permitted = byte.is_ascii_alphanumeric()
                || b"!#$%&'*+-/=?^_`{|}~".contains(&byte);
            if !permitted {
                return Err(EmailError::LocalChar { at: index });
            }
        }
    }

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

    let domain_bytes = &raw_domain[..domain_len];
    let root_dot_position = input.len() - 1;
    let mut labels: Vec<(usize, &[u8])> = Vec::new();
    let mut label_start = 0;

    for index in 0..=domain_len {
        if index == domain_len || domain_bytes[index] == b'.' {
            let label = &domain_bytes[label_start..index];
            let label_position = domain_start + label_start;

            if label.is_empty() {
                let at = if has_root_dot && label_start == domain_len {
                    root_dot_position
                } else {
                    label_position
                };
                return Err(EmailError::EmptyLabel { at });
            }

            if label.len() > 63 {
                return Err(EmailError::LabelTooLong {
                    at: label_position + 63,
                });
            }
            if label[0] == b'-' {
                return Err(EmailError::LabelHyphen { at: label_position });
            }
            if label[label.len() - 1] == b'-' {
                return Err(EmailError::LabelHyphen {
                    at: label_position + label.len() - 1,
                });
            }
            for (offset, &byte) in label.iter().enumerate() {
                if !(byte.is_ascii_alphanumeric() || byte == b'-') {
                    return Err(EmailError::DomainChar {
                        at: label_position + offset,
                    });
                }
            }

            labels.push((label_position, label));
            label_start = index + 1;
        }
    }

    if labels.len() < 2 {
        return Err(EmailError::DomainNeedsDot { at: domain_start });
    }

    let (last_position, last_label) = labels[labels.len() - 1];
    if last_label.iter().all(u8::is_ascii_digit) {
        return Err(EmailError::NumericTld { at: last_position });
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