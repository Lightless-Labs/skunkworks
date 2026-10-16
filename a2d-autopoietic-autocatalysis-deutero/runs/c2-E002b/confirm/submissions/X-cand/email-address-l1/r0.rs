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

    for (index, &byte) in local_bytes.iter().enumerate() {
        let original_at = index;
        if byte == b'.' {
            if index == 0
                || index + 1 == local_bytes.len()
                || local_bytes[index - 1] == b'.'
            {
                return Err(EmailError::LocalDot { at: original_at });
            }
        } else if !byte.is_ascii_alphanumeric()
            && !b"!#$%&'*+-/=?^_`{|}~".contains(&byte)
        {
            return Err(EmailError::LocalChar { at: original_at });
        }
    }

    let domain_start = separator + 1;
    let raw_domain = &bytes[domain_start..];
    let removed_root_dot = raw_domain.last() == Some(&b'.');
    let domain_len = raw_domain.len() - usize::from(removed_root_dot);
    if domain_len == 0 {
        return Err(EmailError::EmptyDomain { at: domain_start });
    }

    if domain_len > 253 {
        return Err(EmailError::DomainTooLong {
            at: domain_start + 253,
        });
    }

    let domain_bytes = &raw_domain[..domain_len];
    let mut label_count = 0usize;
    let mut label_start = 0usize;
    let mut last_label = &domain_bytes[0..0];

    for index in 0..=domain_len {
        if index == domain_len || domain_bytes[index] == b'.' {
            let label = &domain_bytes[label_start..index];
            if label.is_empty() {
                let at = if index == domain_len && removed_root_dot {
                    input.len() - 1
                } else {
                    domain_start + label_start
                };
                return Err(EmailError::EmptyLabel { at });
            }

            if label.len() > 63 {
                return Err(EmailError::LabelTooLong {
                    at: domain_start + label_start + 63,
                });
            }

            if label[0] == b'-' {
                return Err(EmailError::LabelHyphen {
                    at: domain_start + label_start,
                });
            }
            if label[label.len() - 1] == b'-' {
                return Err(EmailError::LabelHyphen {
                    at: domain_start + index - 1,
                });
            }

            for (offset, &byte) in label.iter().enumerate() {
                if !byte.is_ascii_alphanumeric() && byte != b'-' {
                    return Err(EmailError::DomainChar {
                        at: domain_start + label_start + offset,
                    });
                }
            }

            label_count += 1;
            last_label = label;
            label_start = index + 1;
        }
    }

    if label_count < 2 {
        return Err(EmailError::DomainNeedsDot { at: domain_start });
    }

    if last_label.iter().all(u8::is_ascii_digit) {
        return Err(EmailError::NumericTld {
            at: domain_start + domain_len - last_label.len(),
        });
    }

    let local = input[..separator].to_owned();
    let domain = domain_bytes
        .iter()
        .map(u8::to_ascii_lowercase)
        .map(char::from)
        .collect();

    Ok(Address {
        local,
        domain,
        quoted: false,
    })
}