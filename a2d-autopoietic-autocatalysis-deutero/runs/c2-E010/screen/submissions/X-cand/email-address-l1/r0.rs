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
    let b = input.as_bytes();
    if b.is_empty() {
        return Err(EmailError::EmptyInput);
    }
    for (i, &byte) in b.iter().enumerate() {
        if byte >= 128 {
            return Err(EmailError::NonAscii { at: i });
        }
    }

    let mut at_pos = None;
    for (i, &byte) in b.iter().enumerate() {
        if byte == b'@' {
            if at_pos.is_some() {
                return Err(EmailError::MultipleAt { at: i });
            }
            at_pos = Some(i);
        }
    }
    let at_pos = match at_pos {
        Some(pos) => pos,
        None => return Err(EmailError::MissingAt),
    };
    if at_pos == 0 {
        return Err(EmailError::EmptyLocal { at: at_pos });
    }

    let local = &b[..at_pos];
    if local.len() > 64 {
        return Err(EmailError::LocalTooLong { at: 64 });
    }
    let mut previous_dot = false;
    for (i, &byte) in local.iter().enumerate() {
        if byte == b'.' {
            if i == 0 || i + 1 == local.len() || previous_dot {
                return Err(EmailError::LocalDot { at: i });
            }
            previous_dot = true;
        } else {
            if !byte.is_ascii_alphanumeric() && !b"!#$%&'*+-/=?^_`{|}~".contains(&byte) {
                return Err(EmailError::LocalChar { at: i });
            }
            previous_dot = false;
        }
    }

    let domain_start = at_pos + 1;
    let raw = &b[domain_start..];
    let removed_root_dot = raw.last() == Some(&b'.');
    let domain_end = b.len() - usize::from(removed_root_dot);
    let domain = &b[domain_start..domain_end];

    if domain.is_empty() {
        return Err(EmailError::EmptyDomain { at: domain_start });
    }
    if domain.len() > 253 {
        return Err(EmailError::DomainTooLong {
            at: domain_start + 253,
        });
    }

    let mut count = 0usize;
    let mut start = 0usize;
    let mut last_label = (0usize, 0usize);
    for end in 0..=domain.len() {
        if end != domain.len() && domain[end] != b'.' {
            continue;
        }

        let label = &domain[start..end];
        let abs_start = domain_start + start;
        if label.is_empty() {
            let empty_at = if start == domain.len() && removed_root_dot {
                domain_end
            } else {
                abs_start
            };
            return Err(EmailError::EmptyLabel { at: empty_at });
        }
        if label.len() > 63 {
            return Err(EmailError::LabelTooLong {
                at: abs_start + 63,
            });
        }
        if label[0] == b'-' {
            return Err(EmailError::LabelHyphen { at: abs_start });
        }
        if label[label.len() - 1] == b'-' {
            return Err(EmailError::LabelHyphen {
                at: abs_start + label.len() - 1,
            });
        }
        for (offset, &byte) in label.iter().enumerate() {
            if !byte.is_ascii_alphanumeric() && byte != b'-' {
                return Err(EmailError::DomainChar {
                    at: abs_start + offset,
                });
            }
        }

        count += 1;
        last_label = (start, end);
        start = end + 1;
    }

    if count < 2 {
        return Err(EmailError::DomainNeedsDot { at: domain_start });
    }
    let tld = &domain[last_label.0..last_label.1];
    if tld.iter().all(|byte| byte.is_ascii_digit()) {
        return Err(EmailError::NumericTld {
            at: domain_start + last_label.0,
        });
    }

    let local_string = String::from_utf8(local.to_vec()).expect("ASCII");
    let domain_string: String = domain
        .iter()
        .map(|byte| byte.to_ascii_lowercase() as char)
        .collect();
    Ok(Address {
        local: local_string,
        domain: domain_string,
        quoted: false,
    })
}
