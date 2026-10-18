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
    UnclosedQuote { at: usize },
    TrailingLocal { at: usize },
    BadEscape { at: usize },
    QuotedChar { at: usize },
    BadFold { at: usize },
    UnclosedComment { at: usize },
    CommentDepth { at: usize },
    CommentChar { at: usize },
    CommentEscape { at: usize },
    UnexpectedClose { at: usize },
    CfwsBoundary { at: usize },
    EmptyNormalizedLocal { at: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalCase {
    Preserve,
    Lower,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    pub local_case: LocalCase,
    pub strip_subaddress: bool,
    pub unquote_safe: bool,
    pub allow_single_label: bool,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            local_case: LocalCase::Preserve,
            strip_subaddress: false,
            unquote_safe: false,
            allow_single_label: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DedupResult {
    pub addresses: Vec<Address>,
    pub source_indices: Vec<usize>,
    pub duplicate_of: Vec<Option<usize>>,
    pub errors: Vec<(usize, EmailError)>,
}

#[derive(Clone)]
struct Cleaned {
    bytes: Vec<u8>,
    positions: Vec<usize>,
    runs: Vec<(usize, usize, usize)>,
}

fn clean_cfws(input: &[u8]) -> Result<Cleaned, EmailError> {
    let mut bytes = Vec::with_capacity(input.len());
    let mut positions = Vec::with_capacity(input.len());
    let mut runs = Vec::new();
    let mut i = 0;
    let mut quote_prefix = false;
    let mut in_quote = false;
    let mut quote_closed = false;

    while i < input.len() {
        if quote_prefix && in_quote {
            let b = input[i];
            bytes.push(b);
            positions.push(i);
            if b == b'\\' {
                i += 1;
                if i < input.len() {
                    bytes.push(input[i]);
                    positions.push(i);
                    i += 1;
                }
            } else {
                if b == b'"' {
                    in_quote = false;
                    quote_closed = true;
                }
                i += 1;
            }
            continue;
        }

        if !quote_prefix && bytes.is_empty() && input[i] == b'"' {
            quote_prefix = true;
            in_quote = true;
            bytes.push(input[i]);
            positions.push(i);
            i += 1;
            continue;
        }

        let start = i;
        let mut had_cfws = false;
        while i < input.len() {
            if input[i] == b' ' || input[i] == b'\t' {
                had_cfws = true;
                i += 1;
                continue;
            }
            if input[i] == b'\r' || input[i] == b'\n' {
                if input[i] == b'\r'
                    && i + 2 < input.len()
                    && input[i + 1] == b'\n'
                    && (input[i + 2] == b' ' || input[i + 2] == b'\t')
                {
                    had_cfws = true;
                    i += 2;
                    while i < input.len() && (input[i] == b' ' || input[i] == b'\t') {
                        i += 1;
                    }
                    continue;
                }
                return Err(EmailError::BadFold { at: i });
            }
            if input[i] == b'(' {
                had_cfws = true;
                let outer = i;
                let mut openings = [0usize; 3];
                openings[0] = i;
                let mut depth = 1usize;
                i += 1;
                while i < input.len() {
                    let b = input[i];
                    if b == b'\\' {
                        let slash = i;
                        if i + 1 >= input.len()
                            || !(0x20..=0x7e).contains(&input[i + 1])
                        {
                            return Err(EmailError::CommentEscape { at: slash });
                        }
                        i += 2;
                        continue;
                    }
                    if b == b'\r' || b == b'\n' {
                        if b == b'\r'
                            && i + 2 < input.len()
                            && input[i + 1] == b'\n'
                            && (input[i + 2] == b' ' || input[i + 2] == b'\t')
                        {
                            i += 2;
                            while i < input.len()
                                && (input[i] == b' ' || input[i] == b'\t')
                            {
                                i += 1;
                            }
                            continue;
                        }
                        return Err(EmailError::BadFold { at: i });
                    }
                    if b == b'(' {
                        if depth == 3 {
                            return Err(EmailError::CommentDepth { at: i });
                        }
                        openings[depth] = i;
                        depth += 1;
                        i += 1;
                        continue;
                    }
                    if b == b')' {
                        depth -= 1;
                        i += 1;
                        if depth == 0 {
                            break;
                        }
                        continue;
                    }
                    if !(b == b'\t' || (0x20..=0x7e).contains(&b)) {
                        return Err(EmailError::CommentChar { at: i });
                    }
                    i += 1;
                }
                if depth != 0 {
                    return Err(EmailError::UnclosedComment { at: outer });
                }
                continue;
            }
            break;
        }
        if had_cfws {
            runs.push((start, i, bytes.len()));
        }
        if i < input.len() {
            if input[i] == b')' {
                return Err(EmailError::UnexpectedClose { at: i });
            }
            if !quote_prefix && bytes.is_empty() && input[i] == b'"' && !quote_closed {
                quote_prefix = true;
                in_quote = true;
            }
            bytes.push(input[i]);
            positions.push(i);
            i += 1;
        }
    }

    Ok(Cleaned {
        bytes,
        positions,
        runs,
    })
}

fn mapped(cleaned: &Cleaned, index: usize, input_len: usize) -> usize {
    cleaned.positions.get(index).copied().unwrap_or(input_len)
}

fn local_dot_atom(local: &[u8]) -> bool {
    if local.is_empty() || local.len() > 64 {
        return false;
    }
    let mut previous_dot = true;
    for (i, &b) in local.iter().enumerate() {
        if b == b'.' {
            if previous_dot || i + 1 == local.len() {
                return false;
            }
            previous_dot = true;
        } else if b.is_ascii_alphanumeric()
            || b"!#$%&'*+-/=?^_`{|}~".contains(&b)
        {
            previous_dot = false;
        } else {
            return false;
        }
    }
    true
}

fn parse_internal(
    input: &str,
    allow_single_label: bool,
) -> Result<(Address, Vec<usize>), EmailError> {
    let raw = input.as_bytes();
    if raw.is_empty() {
        return Err(EmailError::EmptyInput);
    }
    if let Some(at) = raw.iter().position(|&b| b >= 128) {
        return Err(EmailError::NonAscii { at });
    }

    let cleaned = clean_cfws(raw)?;
    for &(start, end, boundary) in &cleaned.runs {
        let before_at = boundary > 0 && cleaned.bytes[boundary - 1] == b'@';
        let after_at = boundary < cleaned.bytes.len() && cleaned.bytes[boundary] == b'@';
        if boundary == 0
            || boundary == cleaned.bytes.len()
            || before_at
            || after_at
        {
            continue;
        }
        return Err(EmailError::CfwsBoundary {
            at: start.min(end),
        });
    }
    let bytes = &cleaned.bytes;
    if bytes.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let mut quoted_close = None;
    if bytes[0] == b'"' {
        let mut i = 1;
        while i < bytes.len() {
            if bytes[i] == b'\\' {
                i += 2;
            } else if bytes[i] == b'"' {
                quoted_close = Some(i);
                break;
            } else {
                i += 1;
            }
        }
        if quoted_close.is_none() {
            return Err(EmailError::UnclosedQuote {
                at: mapped(&cleaned, 0, raw.len()),
            });
        }
    }

    let mut separators = Vec::new();
    for i in 0..bytes.len() {
        if bytes[i] == b'@' {
            if let Some(close) = quoted_close {
                if i <= close {
                    continue;
                }
            }
            separators.push(i);
        }
    }
    if separators.is_empty() {
        return Err(EmailError::MissingAt);
    }
    if separators.len() > 1 {
        return Err(EmailError::MultipleAt {
            at: mapped(&cleaned, separators[1], raw.len()),
        });
    }
    let sep = separators[0];
    if sep == 0 {
        return Err(EmailError::EmptyLocal {
            at: mapped(&cleaned, sep, raw.len()),
        });
    }
    let local_raw = &bytes[..sep];
    let mut local = Vec::new();
    let mut local_origins = Vec::new();
    let quoted;

    if let Some(close) = quoted_close {
        if close + 1 != sep {
            return Err(EmailError::TrailingLocal {
                at: mapped(&cleaned, close + 1, raw.len()),
            });
        }
        let mut i = 1;
        while i < close {
            let b = bytes[i];
            if b == b'\\' {
                if i + 1 >= close {
                    return Err(EmailError::BadEscape {
                        at: mapped(&cleaned, i, raw.len()),
                    });
                }
                let next = bytes[i + 1];
                if next != b'"' && next != b'\\' && next != b'@' {
                    return Err(EmailError::BadEscape {
                        at: mapped(&cleaned, i, raw.len()),
                    });
                }
                local.push(next);
                local_origins.push(mapped(&cleaned, i, raw.len()));
                i += 2;
            } else {
                if !(0x20..=0x7e).contains(&b) {
                    return Err(EmailError::QuotedChar {
                        at: mapped(&cleaned, i, raw.len()),
                    });
                }
                local.push(b);
                local_origins.push(mapped(&cleaned, i, raw.len()));
                i += 1;
            }
        }
        if local.is_empty() {
            return Err(EmailError::EmptyLocal {
                at: mapped(&cleaned, close, raw.len()),
            });
        }
        if local.len() > 64 {
            return Err(EmailError::LocalTooLong {
                at: local_origins[64],
            });
        }
        quoted = true;
    } else {
        if local_raw.len() > 64 {
            return Err(EmailError::LocalTooLong {
                at: mapped(&cleaned, 64, raw.len()),
            });
        }
        let mut previous_dot = false;
        for (i, &b) in local_raw.iter().enumerate() {
            if b == b'.' {
                if i == 0 || i + 1 == local_raw.len() || previous_dot {
                    return Err(EmailError::LocalDot {
                        at: mapped(&cleaned, i, raw.len()),
                    });
                }
                previous_dot = true;
            } else {
                if !b.is_ascii_alphanumeric()
                    && !b"!#$%&'*+-/=?^_`{|}~".contains(&b)
                {
                    return Err(EmailError::LocalChar {
                        at: mapped(&cleaned, i, raw.len()),
                    });
                }
                previous_dot = false;
            }
            local.push(b);
            local_origins.push(mapped(&cleaned, i, raw.len()));
        }
        quoted = false;
    }

    let domain_start = sep + 1;
    let mut domain_end = bytes.len();
    let mut root_dot = None;
    if domain_end > domain_start && bytes[domain_end - 1] == b'.' {
        root_dot = Some(domain_end - 1);
        domain_end -= 1;
    }
    if domain_end == domain_start {
        return Err(EmailError::EmptyDomain {
            at: mapped(&cleaned, domain_start, raw.len()),
        });
    }
    if domain_end - domain_start > 253 {
        return Err(EmailError::DomainTooLong {
            at: mapped(&cleaned, domain_start + 253, raw.len()),
        });
    }

    let mut labels = Vec::new();
    let mut label_start = domain_start;
    for i in domain_start..=domain_end {
        if i == domain_end || bytes[i] == b'.' {
            if label_start == i {
                let at_index = if i == domain_end {
                    root_dot.unwrap_or(i)
                } else {
                    i
                };
                return Err(EmailError::EmptyLabel {
                    at: mapped(&cleaned, at_index, raw.len()),
                });
            }
            let label = &bytes[label_start..i];
            if label.len() > 63 {
                return Err(EmailError::LabelTooLong {
                    at: mapped(&cleaned, label_start + 63, raw.len()),
                });
            }
            if label[0] == b'-' {
                return Err(EmailError::LabelHyphen {
                    at: mapped(&cleaned, label_start, raw.len()),
                });
            }
            if label[label.len() - 1] == b'-' {
                return Err(EmailError::LabelHyphen {
                    at: mapped(&cleaned, i - 1, raw.len()),
                });
            }
            for (offset, &b) in label.iter().enumerate() {
                if !b.is_ascii_alphanumeric() && b != b'-' {
                    return Err(EmailError::DomainChar {
                        at: mapped(&cleaned, label_start + offset, raw.len()),
                    });
                }
            }
            labels.push((label_start, i));
            label_start = i + 1;
        }
    }

    if labels.len() < 2 && !allow_single_label {
        return Err(EmailError::DomainNeedsDot {
            at: mapped(&cleaned, domain_start, raw.len()),
        });
    }
    let (last_start, last_end) = *labels.last().unwrap();
    if bytes[last_start..last_end].iter().all(u8::is_ascii_digit) {
        return Err(EmailError::NumericTld {
            at: mapped(&cleaned, last_start, raw.len()),
        });
    }

    let domain = bytes[domain_start..domain_end]
        .iter()
        .map(|&b| b.to_ascii_lowercase() as char)
        .collect::<String>();
    let local_string = String::from_utf8(local).unwrap();
    Ok((
        Address {
            local: local_string,
            domain,
            quoted,
        },
        local_origins,
    ))
}

pub fn parse_email(input: &str) -> Result<Address, EmailError> {
    parse_internal(input, false).map(|(address, _)| address)
}

pub fn normalize_email(input: &str, policy: &Policy) -> Result<Address, EmailError> {
    let (mut address, origins) = parse_internal(input, policy.allow_single_label)?;
    let mut local = address.local.into_bytes();

    if policy.local_case == LocalCase::Lower {
        for b in &mut local {
            *b = b.to_ascii_lowercase();
        }
    }

    let mut transformed_origins = origins;
    if policy.strip_subaddress {
        if let Some(plus) = local.iter().position(|&b| b == b'+') {
            if plus == 0 {
                return Err(EmailError::EmptyNormalizedLocal {
                    at: transformed_origins.get(plus).copied().unwrap_or(input.len()),
                });
            }
            local.truncate(plus);
            transformed_origins.truncate(plus);
        }
    }

    address.local = String::from_utf8(local).unwrap();
    if !local_dot_atom(address.local.as_bytes()) {
        address.quoted = true;
    } else if policy.unquote_safe {
        address.quoted = false;
    }
    Ok(address)
}

pub fn format_email(address: &Address) -> String {
    let mut result = String::new();
    if address.quoted {
        result.push('"');
        for ch in address.local.chars() {
            if ch == '"' || ch == '\\' {
                result.push('\\');
            }
            result.push(ch);
        }
        result.push('"');
    } else {
        result.push_str(&address.local);
    }
    result.push('@');
    result.push_str(&address.domain);
    result
}

pub fn deduplicate(inputs: &[&str], policy: &Policy) -> DedupResult {
    let mut result = DedupResult {
        addresses: Vec::new(),
        source_indices: Vec::new(),
        duplicate_of: vec![None; inputs.len()],
        errors: Vec::new(),
    };
    let mut keys: Vec<(String, String, usize)> = Vec::new();

    for (index, input) in inputs.iter().enumerate() {
        match normalize_email(input, policy) {
            Err(error) => result.errors.push((index, error)),
            Ok(address) => {
                if let Some((_, _, first_index)) = keys
                    .iter()
                    .find(|(local, domain, _)| {
                        local == &address.local && domain == &address.domain
                    })
                {
                    result.duplicate_of[index] = Some(*first_index);
                } else {
                    keys.push((address.local.clone(), address.domain.clone(), index));
                    result.source_indices.push(index);
                    result.addresses.push(address);
                }
            }
        }
    }
    result
}