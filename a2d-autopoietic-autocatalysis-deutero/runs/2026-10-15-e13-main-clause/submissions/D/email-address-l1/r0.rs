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

    let separator = separators[0];
    if separator == 0 {
        return Err(EmailError::EmptyLocal { at: separator });
    }

    let local_bytes = &bytes[..separator];
    if local_bytes.len() > 64 {
        return Err(EmailError::LocalTooLong { at: 64 });
    }

    let allowed_specials = b"!#$%&'*+-/=?^_`{|}~";
    for (i, &b) in local_bytes.iter().enumerate() {
        if b == b'.' {
            if i == 0 || i + 1 == local_bytes.len() || local_bytes[i - 1] == b'.' {
                return Err(EmailError::LocalDot { at: i });
            }
        } else if !b.is_ascii_alphanumeric() && !allowed_specials.contains(&b) {
            return Err(EmailError::LocalChar { at: i });
        }
    }

    let domain_start = separator + 1;
    let raw_domain = &bytes[domain_start..];
    let has_root_dot = raw_domain.last() == Some(&b'.');
    let domain_end = if has_root_dot {
        input.len() - 1
    } else {
        input.len()
    };
    let domain_bytes = &bytes[domain_start..domain_end];

    if domain_bytes.is_empty() {
        return Err(EmailError::EmptyDomain { at: domain_start });
    }

    if domain_bytes.len() > 253 {
        return Err(EmailError::DomainTooLong {
            at: domain_start + 253,
        });
    }

    let mut label_starts = Vec::new();
    let mut label_ends = Vec::new();
    let mut start = 0;

    for i in 0..=domain_bytes.len() {
        if i == domain_bytes.len() || domain_bytes[i] == b'.' {
            label_starts.push(start);
            label_ends.push(i);
            start = i + 1;
        }
    }

    for (label_index, (&start, &end)) in label_starts.iter().zip(&label_ends).enumerate() {
        if start == end {
            let at = if label_index + 1 == label_starts.len() && has_root_dot {
                input.len() - 1
            } else {
                domain_start + start
            };
            return Err(EmailError::EmptyLabel { at });
        }

        let label = &domain_bytes[start..end];
        if label.len() > 63 {
            return Err(EmailError::LabelTooLong {
                at: domain_start + start + 63,
            });
        }

        if label[0] == b'-' {
            return Err(EmailError::LabelHyphen {
                at: domain_start + start,
            });
        }
        if label[label.len() - 1] == b'-' {
            return Err(EmailError::LabelHyphen {
                at: domain_start + end - 1,
            });
        }

        for (offset, &b) in label.iter().enumerate() {
            if !b.is_ascii_alphanumeric() && b != b'-' {
                return Err(EmailError::DomainChar {
                    at: domain_start + start + offset,
                });
            }
        }
    }

    if label_starts.len() < 2 {
        return Err(EmailError::DomainNeedsDot { at: domain_start });
    }

    let last_start = label_starts[label_starts.len() - 1];
    let last_end = label_ends[label_ends.len() - 1];
    if domain_bytes[last_start..last_end]
        .iter()
        .all(u8::is_ascii_digit)
    {
        return Err(EmailError::NumericTld {
            at: domain_start + last_start,
        });
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