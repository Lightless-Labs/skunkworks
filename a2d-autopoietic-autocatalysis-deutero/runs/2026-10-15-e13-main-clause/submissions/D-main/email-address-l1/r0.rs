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

    let mut separators = bytes.iter().enumerate().filter(|item| *item.1 == b'@');
    let separator = match separators.next() {
        Some((at, _)) => at,
        None => return Err(EmailError::MissingAt),
    };
    if let Some((at, _)) = separators.next() {
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
        if byte == b'.' {
            if index == 0 || index + 1 == local.len() || local[index - 1] == b'.' {
                return Err(EmailError::LocalDot { at: index });
            }
        } else if !byte.is_ascii_alphanumeric() && !b"!#$%&'*+-/=?^_`{|}~".contains(&byte) {
            return Err(EmailError::LocalChar { at: index });
        }
    }

    let domain_start = separator + 1;
    let raw_domain = &bytes[domain_start..];
    let has_root_dot = raw_domain.last() == Some(&b'.');
    let domain = if has_root_dot {
        &raw_domain[..raw_domain.len() - 1]
    } else {
        raw_domain
    };
    let root_dot_at = input.len() - 1;

    if domain.is_empty() {
        return Err(EmailError::EmptyDomain { at: domain_start });
    }

    if domain.len() > 253 {
        return Err(EmailError::DomainTooLong {
            at: domain_start + 253,
        });
    }

    let mut label_ranges = Vec::new();
    let mut start = 0;
    for (index, &byte) in domain.iter().enumerate() {
        if byte == b'.' {
            label_ranges.push((start, index));
            start = index + 1;
        }
    }
    label_ranges.push((start, domain.len()));

    for (label_index, &(begin, end)) in label_ranges.iter().enumerate() {
        if begin == end {
            let at = if label_index + 1 == label_ranges.len() && has_root_dot {
                root_dot_at
            } else {
                domain_start + begin
            };
            return Err(EmailError::EmptyLabel { at });
        }

        let label = &domain[begin..end];
        if label.len() > 63 {
            return Err(EmailError::LabelTooLong {
                at: domain_start + begin + 63,
            });
        }
        if label[0] == b'-' {
            return Err(EmailError::LabelHyphen {
                at: domain_start + begin,
            });
        }
        if label[label.len() - 1] == b'-' {
            return Err(EmailError::LabelHyphen {
                at: domain_start + end - 1,
            });
        }
        for (offset, &byte) in label.iter().enumerate() {
            if !byte.is_ascii_alphanumeric() && byte != b'-' {
                return Err(EmailError::DomainChar {
                    at: domain_start + begin + offset,
                });
            }
        }
    }

    if label_ranges.len() < 2 {
        return Err(EmailError::DomainNeedsDot { at: domain_start });
    }

    let (last_begin, last_end) = *label_ranges.last().unwrap();
    let last_label = &domain[last_begin..last_end];
    if last_label.iter().all(u8::is_ascii_digit) {
        return Err(EmailError::NumericTld {
            at: domain_start + last_begin,
        });
    }

    Ok(Address {
        local: String::from_utf8(local.to_vec()).expect("validated ASCII local part"),
        domain: String::from_utf8(domain.to_vec())
            .expect("validated ASCII domain")
            .to_ascii_lowercase(),
        quoted: false,
    })
}

fn main() {}