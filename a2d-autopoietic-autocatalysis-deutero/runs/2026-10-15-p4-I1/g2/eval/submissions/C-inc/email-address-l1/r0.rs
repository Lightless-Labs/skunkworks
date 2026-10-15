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

    let separators: Vec<usize> = bytes
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| (b == b'@').then_some(i))
        .collect();

    if separators.is_empty() {
        return Err(EmailError::MissingAt);
    }
    if separators.len() > 1 {
        return Err(EmailError::MultipleAt { at: separators[1] });
    }

    let sep = separators[0];
    if sep == 0 {
        return Err(EmailError::EmptyLocal { at: sep });
    }

    let local_bytes = &bytes[..sep];
    if local_bytes.len() > 64 {
        return Err(EmailError::LocalTooLong { at: 64 });
    }

    for (i, &b) in local_bytes.iter().enumerate() {
        if b == b'.' {
            if i == 0 || i + 1 == local_bytes.len() || local_bytes[i - 1] == b'.' {
                return Err(EmailError::LocalDot { at: i });
            }
        } else if !b.is_ascii_alphanumeric()
            && !b"!#$%&'*+-/=?^_`{|}~".contains(&b)
        {
            return Err(EmailError::LocalChar { at: i });
        }
    }

    let domain_start = sep + 1;
    let raw_domain = &bytes[domain_start..];
    let has_root_dot = raw_domain.last() == Some(&b'.');
    let domain_bytes = if has_root_dot {
        &raw_domain[..raw_domain.len() - 1]
    } else {
        raw_domain
    };
    let domain_len = domain_bytes.len();
    let root_dot_at = has_root_dot.then_some(input.len() - 1);

    if domain_bytes.is_empty() {
        return Err(EmailError::EmptyDomain { at: domain_start });
    }

    if domain_len > 253 {
        return Err(EmailError::DomainTooLong {
            at: domain_start + 253,
        });
    }

    let mut labels: Vec<(usize, &[u8])> = Vec::new();
    let mut label_start = 0;

    for i in 0..=domain_len {
        if i == domain_len || domain_bytes[i] == b'.' {
            let label = &domain_bytes[label_start..i];
            if label.is_empty() {
                let at = if label_start == domain_len {
                    root_dot_at.unwrap_or(domain_start + label_start)
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
                    at: domain_start + i - 1,
                });
            }

            for (j, &b) in label.iter().enumerate() {
                if !b.is_ascii_alphanumeric() && b != b'-' {
                    return Err(EmailError::DomainChar {
                        at: domain_start + label_start + j,
                    });
                }
            }

            labels.push((domain_start + label_start, label));
            label_start = i + 1;
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
    let domain = domain_bytes
        .iter()
        .map(|b| b.to_ascii_lowercase() as char)
        .collect();

    Ok(Address {
        local,
        domain,
        quoted: false,
    })
}

fn main() {}