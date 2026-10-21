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

    let mut separators = bytes
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| (b == b'@').then_some(i));

    let separator = match separators.next() {
        None => return Err(EmailError::MissingAt),
        Some(at) => at,
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

    for (i, &b) in local_bytes.iter().enumerate() {
        if b == b'.' {
            if i == 0 || i + 1 == local_bytes.len() || local_bytes[i - 1] == b'.' {
                return Err(EmailError::LocalDot { at: i });
            }
        } else if !b.is_ascii_alphanumeric() && !b"!#$%&'*+-/=?^_`{|}~".contains(&b) {
            return Err(EmailError::LocalChar { at: i });
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

    let root_dot_at = input.len() - 1;
    let mut label_ranges = Vec::new();
    let mut label_start = 0;

    for i in 0..=domain_bytes.len() {
        if i == domain_bytes.len() || domain_bytes[i] == b'.' {
            label_ranges.push((label_start, i));
            label_start = i + 1;
        }
    }

    for &(start, end) in &label_ranges {
        if start == end {
            let at = if has_root_dot && start == domain_bytes.len() {
                root_dot_at
            } else if start < domain_bytes.len() {
                domain_start + start
            } else {
                input.len()
            };
            return Err(EmailError::EmptyLabel { at });
        }

        let len = end - start;
        if len > 63 {
            return Err(EmailError::LabelTooLong {
                at: domain_start + start + 63,
            });
        }

        if domain_bytes[start] == b'-' {
            return Err(EmailError::LabelHyphen {
                at: domain_start + start,
            });
        }
        if domain_bytes[end - 1] == b'-' {
            return Err(EmailError::LabelHyphen {
                at: domain_start + end - 1,
            });
        }

        for (offset, &b) in domain_bytes[start..end].iter().enumerate() {
            if !b.is_ascii_alphanumeric() && b != b'-' {
                return Err(EmailError::DomainChar {
                    at: domain_start + start + offset,
                });
            }
        }
    }

    if label_ranges.len() < 2 {
        return Err(EmailError::DomainNeedsDot { at: domain_start });
    }

    let &(last_start, last_end) = label_ranges.last().unwrap();
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