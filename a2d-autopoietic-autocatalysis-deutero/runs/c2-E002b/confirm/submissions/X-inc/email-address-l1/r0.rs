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

    let raw_domain_start = separator + 1;
    let raw_domain = &bytes[raw_domain_start..];
    let has_root_dot = raw_domain.last() == Some(&b'.');
    let domain_bytes = if has_root_dot {
        &raw_domain[..raw_domain.len() - 1]
    } else {
        raw_domain
    };

    if domain_bytes.is_empty() {
        return Err(EmailError::EmptyDomain {
            at: raw_domain_start,
        });
    }

    if domain_bytes.len() > 253 {
        return Err(EmailError::DomainTooLong {
            at: raw_domain_start + 253,
        });
    }

    let mut labels: Vec<(usize, &[u8])> = Vec::new();
    let mut label_start = 0;

    for index in 0..=domain_bytes.len() {
        if index == domain_bytes.len() || domain_bytes[index] == b'.' {
            let label = &domain_bytes[label_start..index];
            let original_start = raw_domain_start + label_start;

            if label.is_empty() {
                let at = if has_root_dot && index == domain_bytes.len() {
                    input.len() - 1
                } else {
                    original_start
                };
                return Err(EmailError::EmptyLabel { at });
            }

            if label.len() > 63 {
                return Err(EmailError::LabelTooLong {
                    at: original_start + 63,
                });
            }

            if label[0] == b'-' {
                return Err(EmailError::LabelHyphen { at: original_start });
            }

            if label[label.len() - 1] == b'-' {
                return Err(EmailError::LabelHyphen {
                    at: original_start + label.len() - 1,
                });
            }

            for (offset, &byte) in label.iter().enumerate() {
                if !(byte.is_ascii_alphanumeric() || byte == b'-') {
                    return Err(EmailError::DomainChar {
                        at: original_start + offset,
                    });
                }
            }

            labels.push((original_start, label));
            label_start = index + 1;
        }
    }

    if labels.len() < 2 {
        return Err(EmailError::DomainNeedsDot {
            at: raw_domain_start,
        });
    }

    let (last_start, last_label) = labels[labels.len() - 1];
    if last_label.iter().all(u8::is_ascii_digit) {
        return Err(EmailError::NumericTld { at: last_start });
    }

    let domain = String::from_utf8(domain_bytes.to_ascii_lowercase())
        .expect("ASCII domain is valid UTF-8");

    Ok(Address {
        local: input[..separator].to_owned(),
        domain,
        quoted: false,
    })
}