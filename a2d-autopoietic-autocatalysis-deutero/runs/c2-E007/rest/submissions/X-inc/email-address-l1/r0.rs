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
        .filter_map(|(at, &byte)| (byte == b'@').then_some(at));

    let Some(separator) = separators.next() else {
        return Err(EmailError::MissingAt);
    };

    if let Some(at) = separators.next() {
        return Err(EmailError::MultipleAt { at });
    }

    if separator == 0 {
        return Err(EmailError::EmptyLocal { at: separator });
    }

    let local = &bytes[..separator];
    if local.len() > 64 {
        return Err(EmailError::LocalTooLong { at: 64 });
    }

    for (index, &byte) in local.iter().enumerate() {
        let at = index;
        if byte == b'.' {
            if index == 0
                || index + 1 == local.len()
                || local[index - 1] == b'.'
            {
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
    let domain_len = raw_domain.len() - usize::from(has_root_dot);

    if domain_len == 0 {
        return Err(EmailError::EmptyDomain { at: domain_start });
    }

    if domain_len > 253 {
        return Err(EmailError::DomainTooLong {
            at: domain_start + 253,
        });
    }

    let domain = &raw_domain[..domain_len];
    let mut labels: Vec<(usize, &[u8])> = Vec::new();
    let mut label_start = 0;

    for index in 0..=domain.len() {
        if index == domain.len() || domain[index] == b'.' {
            let label = &domain[label_start..index];
            let absolute_start = domain_start + label_start;

            if label.is_empty() {
                let at = if label_start == domain.len() && has_root_dot {
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

            for (offset, &byte) in label.iter().enumerate() {
                if !byte.is_ascii_alphanumeric() && byte != b'-' {
                    return Err(EmailError::DomainChar {
                        at: absolute_start + offset,
                    });
                }
            }

            labels.push((absolute_start, label));
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

    let local_string = String::from_utf8(local.to_vec()).expect("ASCII input");
    let domain_string = domain
        .iter()
        .map(u8::to_ascii_lowercase)
        .map(char::from)
        .collect();

    Ok(Address {
        local: local_string,
        domain: domain_string,
        quoted: false,
    })
}