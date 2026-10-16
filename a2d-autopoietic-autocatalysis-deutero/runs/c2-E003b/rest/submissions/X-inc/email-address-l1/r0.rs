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

    let separators: Vec<usize> = bytes
        .iter()
        .enumerate()
        .filter_map(|(index, &byte)| (byte == b'@').then_some(index))
        .collect();

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

    let local_bytes = &bytes[..separator];
    if local_bytes.len() > 64 {
        return Err(EmailError::LocalTooLong { at: 64 });
    }

    for (index, &byte) in local_bytes.iter().enumerate() {
        if byte == b'.' {
            if index == 0
                || index + 1 == local_bytes.len()
                || local_bytes[index - 1] == b'.'
            {
                return Err(EmailError::LocalDot { at: index });
            }
        } else if !byte.is_ascii_alphanumeric()
            && !b"!#$%&'*+-/=?^_`{|}~".contains(&byte)
        {
            return Err(EmailError::LocalChar { at: index });
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
    let mut label_start = 0;
    let mut label_count = 0;
    let mut last_label = &[][..];
    let mut last_label_start = domain_start;

    for boundary in (0..=domain_len).filter(|&index| {
        index == domain_len || domain_bytes[index] == b'.'
    }) {
        let label = &domain_bytes[label_start..boundary];
        let absolute_start = domain_start + label_start;

        if label.is_empty() {
            let at = if has_root_dot && label_start == domain_len {
                input.len() - 1
            } else {
                absolute_start
            };
            return Err(EmailError::EmptyLabel { at });
        }

        if label.len() > 63 {
            return Err(EmailError::LabelTooLong {
                at: absolute_start + 63,
            });
        }

        if label[0] == b'-' {
            return Err(EmailError::LabelHyphen { at: absolute_start });
        }
        if label[label.len() - 1] == b'-' {
            return Err(EmailError::LabelHyphen {
                at: absolute_start + label.len() - 1,
            });
        }

        for (index, &byte) in label.iter().enumerate() {
            if !byte.is_ascii_alphanumeric() && byte != b'-' {
                return Err(EmailError::DomainChar {
                    at: absolute_start + index,
                });
            }
        }

        label_count += 1;
        last_label = label;
        last_label_start = absolute_start;
        label_start = boundary + 1;
    }

    if label_count < 2 {
        return Err(EmailError::DomainNeedsDot { at: domain_start });
    }

    if last_label.iter().all(u8::is_ascii_digit) {
        return Err(EmailError::NumericTld {
            at: last_label_start,
        });
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