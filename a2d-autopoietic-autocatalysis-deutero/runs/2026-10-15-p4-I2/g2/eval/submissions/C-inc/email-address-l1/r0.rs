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
        let at = index;
        if byte == b'.' {
            if index == 0 || index + 1 == local_bytes.len() || previous_was_dot {
                return Err(EmailError::LocalDot { at });
            }
            previous_was_dot = true;
        } else {
            previous_was_dot = false;
            let allowed = byte.is_ascii_alphanumeric()
                || b"!#$%&'*+-/=?^_`{|}~".contains(&byte);
            if !allowed {
                return Err(EmailError::LocalChar { at });
            }
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
    let domain_len = domain_bytes.len();

    if domain_len == 0 {
        return Err(EmailError::EmptyDomain { at: domain_start });
    }

    if domain_len > 253 {
        return Err(EmailError::DomainTooLong {
            at: domain_start + 253,
        });
    }

    let mut labels: Vec<(usize, usize)> = Vec::new();
    let mut label_start = 0;
    for index in 0..=domain_len {
        if index == domain_len || domain_bytes[index] == b'.' {
            let label_len = index - label_start;
            if label_len == 0 {
                let at = if label_start == domain_len && has_root_dot {
                    input.len() - 1
                } else {
                    domain_start + label_start
                };
                return Err(EmailError::EmptyLabel { at });
            }

            if label_len > 63 {
                return Err(EmailError::LabelTooLong {
                    at: domain_start + label_start + 63,
                });
            }

            let first = domain_bytes[label_start];
            let last = domain_bytes[index - 1];
            if first == b'-' {
                return Err(EmailError::LabelHyphen {
                    at: domain_start + label_start,
                });
            }
            if last == b'-' {
                return Err(EmailError::LabelHyphen {
                    at: domain_start + index - 1,
                });
            }

            for (offset, &byte) in domain_bytes[label_start..index].iter().enumerate() {
                if !(byte.is_ascii_alphanumeric() || byte == b'-') {
                    return Err(EmailError::DomainChar {
                        at: domain_start + label_start + offset,
                    });
                }
            }

            labels.push((label_start, index));
            label_start = index + 1;
        }
    }

    if labels.len() < 2 {
        return Err(EmailError::DomainNeedsDot { at: domain_start });
    }

    let (last_start, last_end) = *labels.last().unwrap();
    if domain_bytes[last_start..last_end]
        .iter()
        .all(u8::is_ascii_digit)
    {
        return Err(EmailError::NumericTld {
            at: domain_start + last_start,
        });
    }

    let local = String::from_utf8(local_bytes.to_vec()).unwrap();
    let domain = String::from_utf8(
        domain_bytes
            .iter()
            .map(u8::to_ascii_lowercase)
            .collect(),
    )
    .unwrap();

    Ok(Address {
        local,
        domain,
        quoted: false,
    })
}

fn main() {}