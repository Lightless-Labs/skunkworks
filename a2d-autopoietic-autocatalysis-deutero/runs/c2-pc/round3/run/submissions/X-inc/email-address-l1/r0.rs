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
        Some(at) => at,
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
        let at = index;
        if byte == b'.' {
            if index == 0 || index + 1 == local_bytes.len() || local_bytes[index - 1] == b'.' {
                return Err(EmailError::LocalDot { at });
            }
        } else if !byte.is_ascii_alphanumeric()
            && !b"!#$%&'*+-/=?^_`{|}~".contains(&byte)
        {
            return Err(EmailError::LocalChar { at });
        }
    }

    let domain_start = separator + 1;
    let raw_domain = &bytes[domain_start..];
    let has_root_dot = raw_domain.last() == Some(&b'.');
    let domain_bytes = if has_root_dot {
        &raw_domain[..raw_domain.len() - 1]
    } else {
        raw_domain
    };

    if domain_bytes.is_empty() {
        return Err(EmailError::EmptyDomain { at: domain_start });
    }

    if domain_bytes.len() > 253 {
        return Err(EmailError::DomainTooLong {
            at: domain_start + 253,
        });
    }

    let mut label_count = 0usize;
    let mut label_start = 0usize;
    let mut last_label: &[u8] = &[];

    for end in 0..=domain_bytes.len() {
        if end == domain_bytes.len() || domain_bytes[end] == b'.' {
            let label = &domain_bytes[label_start..end];
            let original_start = domain_start + label_start;

            if label.is_empty() {
                let at = if has_root_dot && label_start == domain_bytes.len() {
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
                if !byte.is_ascii_alphanumeric() && byte != b'-' {
                    return Err(EmailError::DomainChar {
                        at: original_start + offset,
                    });
                }
            }

            label_count += 1;
            last_label = label;
            label_start = end + 1;
        }
    }

    if label_count < 2 {
        return Err(EmailError::DomainNeedsDot { at: domain_start });
    }

    if last_label.iter().all(u8::is_ascii_digit) {
        let last_label_start = domain_start + domain_bytes.len() - last_label.len();
        return Err(EmailError::NumericTld {
            at: last_label_start,
        });
    }

    let local = String::from_utf8(local_bytes.to_vec()).expect("ASCII local part");
    let domain = String::from_utf8(
        domain_bytes
            .iter()
            .map(u8::to_ascii_lowercase)
            .collect::<Vec<_>>(),
    )
    .expect("ASCII domain");

    Ok(Address {
        local,
        domain,
        quoted: false,
    })
}