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

    for (index, &byte) in local_bytes.iter().enumerate() {
        if byte == b'.' {
            if index == 0
                || index == local_bytes.len() - 1
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
    let domain_bytes = if has_root_dot {
        &raw_domain[..raw_domain.len() - 1]
    } else {
        raw_domain
    };
    let domain_end = domain_start + domain_bytes.len();

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

    for index in 0..=domain_bytes.len() {
        if index == domain_bytes.len() || domain_bytes[index] == b'.' {
            let label = &domain_bytes[start..index];
            let label_start = domain_start + start;
            label_starts.push(label_start);
            label_ranges.push((start, index));

            if label.is_empty() {
                let at = if start == domain_bytes.len() && has_root_dot {
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

            start = index + 1;
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

    let local = String::from_utf8(local_bytes.to_vec()).expect("ASCII local part");
    let domain = String::from_utf8(
        domain_bytes
            .iter()
            .map(u8::to_ascii_lowercase)
            .collect(),
    )
    .expect("ASCII domain");

    Ok(Address {
        local,
        domain,
        quoted: false,
    })
}