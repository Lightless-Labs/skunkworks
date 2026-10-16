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

pub fn parse_email(input: &str) -> Result<Address, EmailError> {
    parse_internal(input, false)
}

pub fn normalize_email(input: &str, policy: &Policy) -> Result<Address, EmailError> {
    let mut address = parse_internal(input, policy.allow_single_label)?;
    let mut local = address.local.into_bytes();
    let origins = address_origins(input, &local, address.quoted);

    if policy.local_case == LocalCase::Lower {
        for byte in &mut local {
            if byte.is_ascii_uppercase() {
                *byte = byte.to_ascii_lowercase();
            }
        }
    }

    if policy.strip_subaddress {
        if let Some(index) = local.iter().position(|&byte| byte == b'+') {
            if index == 0 {
                return Err(EmailError::EmptyNormalizedLocal {
                    at: origins.get(index).copied().unwrap_or(input.len()),
                });
            }
            local.truncate(index);
        }
    }

    let local_string = String::from_utf8(local).expect("validated ASCII local");
    let safe_atom = valid_dot_atom(local_string.as_bytes());
    let quoted = if safe_atom {
        address.quoted && !policy.unquote_safe
    } else {
        true
    };

    address.local = local_string;
    address.quoted = quoted;
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
    use std::collections::HashMap;

    let mut result = DedupResult {
        addresses: Vec::new(),
        source_indices: Vec::new(),
        duplicate_of: vec![None; inputs.len()],
        errors: Vec::new(),
    };
    let mut seen: HashMap<(String, String), usize> = HashMap::new();

    for (index, input) in inputs.iter().enumerate() {
        match normalize_email(input, policy) {
            Err(error) => result.errors.push((index, error)),
            Ok(address) => {
                let key = (address.local.clone(), address.domain.clone());
                if let Some(&first_index) = seen.get(&key) {
                    result.duplicate_of[index] = Some(first_index);
                } else {
                    seen.insert(key, index);
                    result.addresses.push(address);
                    result.source_indices.push(index);
                }
            }
        }
    }
    result
}

#[derive(Clone)]
struct Cleaned {
    bytes: Vec<u8>,
    positions: Vec<usize>,
    runs: Vec<(usize, usize, usize)>,
}

fn parse_internal(input: &str, allow_single_label: bool) -> Result<Address, EmailError> {
    let raw = input.as_bytes();
    if raw.is_empty() {
        return Err(EmailError::EmptyInput);
    }
    if let Some(at) = raw.iter().position(|&byte| byte >= 128) {
        return Err(EmailError::NonAscii { at });
    }

    let cleaned = cleanup(raw)?;
    check_cfws_boundaries(&cleaned)?;
    if cleaned.bytes.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let bytes = &cleaned.bytes;
    let mut protected_end = 0;
    let mut closing_quote = None;

    if bytes[0] == b'"' {
        let mut index = 1;
        while index < bytes.len() {
            if bytes[index] == b'\\' {
                index += 2;
            } else if bytes[index] == b'"' {
                closing_quote = Some(index);
                protected_end = index + 1;
                break;
            } else {
                index += 1;
            }
        }
        if closing_quote.is_none() {
            return Err(EmailError::UnclosedQuote {
                at: cleaned.positions[0],
            });
        }
    }

    let mut separators = Vec::new();
    for index in protected_end..bytes.len() {
        if bytes[index] == b'@' {
            separators.push(index);
        }
    }
    if separators.is_empty() {
        return Err(EmailError::MissingAt);
    }
    if separators.len() > 1 {
        return Err(EmailError::MultipleAt {
            at: cleaned.positions[separators[1]],
        });
    }

    let separator = separators[0];
    if separator == 0 {
        return Err(EmailError::EmptyLocal {
            at: cleaned.positions[separator],
        });
    }

    let local = &bytes[..separator];
    let (decoded, origins, quoted) = if let Some(close) = closing_quote {
        if close + 1 != separator {
            return Err(EmailError::TrailingLocal {
                at: cleaned.positions[close + 1],
            });
        }
        decode_quoted(local, &cleaned.positions)?
    } else {
        validate_atom(local, &cleaned.positions)?;
        (
            local.to_vec(),
            cleaned.positions[..separator].to_vec(),
            false,
        )
    };

    let domain_start = separator + 1;
    let mut domain_end = bytes.len();
    let root_position = if domain_end > domain_start && bytes[domain_end - 1] == b'.' {
        domain_end -= 1;
        Some(cleaned.positions[domain_end])
    } else {
        None
    };

    if domain_start == domain_end {
        return Err(EmailError::EmptyDomain {
            at: cleaned
                .positions
                .get(domain_start)
                .copied()
                .unwrap_or(input.len()),
        });
    }

    let domain_len = domain_end - domain_start;
    if domain_len > 253 {
        return Err(EmailError::DomainTooLong {
            at: cleaned.positions[domain_start + 253],
        });
    }

    let mut labels = Vec::new();
    let mut label_start = domain_start;
    for cursor in domain_start..=domain_end {
        if cursor == domain_end || bytes[cursor] == b'.' {
            if cursor == label_start {
                let at = if cursor == domain_end {
                    root_position.unwrap_or_else(|| {
                        cleaned
                            .positions
                            .get(cursor)
                            .copied()
                            .unwrap_or(input.len())
                    })
                } else {
                    cleaned.positions[cursor]
                };
                return Err(EmailError::EmptyLabel { at });
            }

            let label = &bytes[label_start..cursor];
            if label.len() > 63 {
                return Err(EmailError::LabelTooLong {
                    at: cleaned.positions[label_start + 63],
                });
            }
            if label[0] == b'-' {
                return Err(EmailError::LabelHyphen {
                    at: cleaned.positions[label_start],
                });
            }
            if label[label.len() - 1] == b'-' {
                return Err(EmailError::LabelHyphen {
                    at: cleaned.positions[cursor - 1],
                });
            }
            if let Some(offset) = label
                .iter()
                .position(|&byte| !(byte.is_ascii_alphanumeric() || byte == b'-'))
            {
                return Err(EmailError::DomainChar {
                    at: cleaned.positions[label_start + offset],
                });
            }
            labels.push((label_start, cursor));
            label_start = cursor + 1;
        }
    }

    if labels.len() < 2 && !allow_single_label {
        return Err(EmailError::DomainNeedsDot {
            at: cleaned.positions[domain_start],
        });
    }

    let (last_start, last_end) = *labels.last().expect("nonempty domain labels");
    if bytes[last_start..last_end]
        .iter()
        .all(u8::is_ascii_digit)
    {
        return Err(EmailError::NumericTld {
            at: cleaned.positions[last_start],
        });
    }

    let local_string = String::from_utf8(decoded).expect("ASCII local");
    let domain_string = String::from_utf8(
        bytes[domain_start..domain_end]
            .iter()
            .map(u8::to_ascii_lowercase)
            .collect(),
    )
    .expect("ASCII domain");

    let _ = origins;
    Ok(Address {
        local: local_string,
        domain: domain_string,
        quoted,
    })
}

fn cleanup(raw: &[u8]) -> Result<Cleaned, EmailError> {
    let mut bytes = Vec::with_capacity(raw.len());
    let mut positions = Vec::with_capacity(raw.len());
    let mut runs = Vec::new();
    let mut index = 0;
    let mut protected = false;
    let mut quote_closed = false;

    while index < raw.len() {
        if protected {
            let byte = raw[index];
            bytes.push(byte);
            positions.push(index);
            if byte == b'\\' {
                index += 1;
                if index < raw.len() {
                    bytes.push(raw[index]);
                    positions.push(index);
                    index += 1;
                }
            } else {
                if byte == b'"' {
                    protected = false;
                    quote_closed = true;
                }
                index += 1;
            }
            continue;
        }

        let byte = raw[index];
        if !quote_closed && bytes.is_empty() && byte == b'"' {
            protected = true;
            bytes.push(byte);
            positions.push(index);
            index += 1;
            continue;
        }

        if byte == b' ' || byte == b'\t' || byte == b'\r' || byte == b'\n' || byte == b'(' {
            let start = index;
            let clean_start = bytes.len();
            loop {
                if index >= raw.len() {
                    break;
                }
                match raw[index] {
                    b' ' | b'\t' => index += 1,
                    b'\r' => {
                        if index + 2 >= raw.len()
                            || raw[index + 1] != b'\n'
                            || !(raw[index + 2] == b' ' || raw[index + 2] == b'\t')
                        {
                            return Err(EmailError::BadFold { at: index });
                        }
                        index += 2;
                        while index < raw.len() && (raw[index] == b' ' || raw[index] == b'\t') {
                            index += 1;
                        }
                    }
                    b'\n' => return Err(EmailError::BadFold { at: index }),
                    b'(' => {
                        index = scan_comment(raw, index)?;
                    }
                    _ => break,
                }
            }
            runs.push((start, index, clean_start));
            continue;
        }

        if byte == b')' {
            return Err(EmailError::UnexpectedClose { at: index });
        }
        bytes.push(byte);
        positions.push(index);
        index += 1;
    }

    let _ = quote_closed;
    Ok(Cleaned {
        bytes,
        positions,
        runs,
    })
}

fn scan_comment(raw: &[u8], opening: usize) -> Result<usize, EmailError> {
    let mut depth = 1usize;
    let mut outermost = opening;
    let mut index = opening + 1;

    while index < raw.len() {
        let byte = raw[index];
        if byte == b'\\' {
            if index + 1 >= raw.len()
                || !(0x20..=0x7e).contains(&raw[index + 1])
            {
                return Err(EmailError::CommentEscape { at: index });
            }
            index += 2;
        } else if byte == b'(' {
            depth += 1;
            if depth > 3 {
                return Err(EmailError::CommentDepth { at: index });
            }
            index += 1;
        } else if byte == b')' {
            depth -= 1;
            index += 1;
            if depth == 0 {
                return Ok(index);
            }
        } else if byte == b'\r' {
            if index + 2 >= raw.len()
                || raw[index + 1] != b'\n'
                || !(raw[index + 2] == b' ' || raw[index + 2] == b'\t')
            {
                return Err(EmailError::BadFold { at: index });
            }
            index += 2;
            while index < raw.len() && (raw[index] == b' ' || raw[index] == b'\t') {
                index += 1;
            }
        } else if byte == b'\n' {
            return Err(EmailError::BadFold { at: index });
        } else if byte == b'\t' || (0x20..=0x7e).contains(&byte) {
            index += 1;
        } else {
            return Err(EmailError::CommentChar { at: index });
        }
    }
    let _ = outermost;
    Err(EmailError::UnclosedComment { at: opening })
}

fn check_cfws_boundaries(cleaned: &Cleaned) -> Result<(), EmailError> {
    for &(start, end, clean_index) in &cleaned.runs {
        if clean_index == 0 || clean_index == cleaned.bytes.len() {
            continue;
        }
        let left_at = clean_index - 1;
        let right_at = clean_index;
        if cleaned.bytes[left_at] == b'@' || cleaned.bytes[right_at] == b'@' {
            continue;
        }
        return Err(EmailError::CfwsBoundary { at: start });
    }
    Ok(())
}

fn decode_quoted(
    local: &[u8],
    positions: &[usize],
) -> Result<(Vec<u8>, Vec<usize>, bool), EmailError> {
    let mut decoded = Vec::new();
    let mut origins = Vec::new();
    let mut index = 1;
    while index + 1 < local.len() {
        let byte = local[index];
        if byte == b'\\' {
            if index + 1 >= local.len() - 1 {
                return Err(EmailError::BadEscape {
                    at: positions[index],
                });
            }
            let escaped = local[index + 1];
            if escaped != b'"' && escaped != b'\\' && escaped != b'@' {
                return Err(EmailError::BadEscape {
                    at: positions[index],
                });
            }
            decoded.push(escaped);
            origins.push(positions[index]);
            index += 2;
        } else {
            if !(0x20..=0x7e).contains(&byte) {
                return Err(EmailError::QuotedChar {
                    at: positions[index],
                });
            }
            decoded.push(byte);
            origins.push(positions[index]);
            index += 1;
        }
    }

    if decoded.is_empty() {
        return Err(EmailError::EmptyLocal {
            at: positions[local.len() - 1],
        });
    }
    if decoded.len() > 64 {
        return Err(EmailError::LocalTooLong { at: origins[64] });
    }
    Ok((decoded, origins, true))
}

fn validate_atom(local: &[u8], positions: &[usize]) -> Result<(), EmailError> {
    if local.len() > 64 {
        return Err(EmailError::LocalTooLong {
            at: positions[64],
        });
    }
    for index in 0..local.len() {
        let byte = local[index];
        if byte == b'.' {
            if index == 0 || index + 1 == local.len() || local[index - 1] == b'.' {
                return Err(EmailError::LocalDot {
                    at: positions[index],
                });
            }
        } else if !(byte.is_ascii_alphanumeric() || b"!#$%&'*+-/=?^_`{|}~".contains(&byte)) {
            return Err(EmailError::LocalChar {
                at: positions[index],
            });
        }
    }
    Ok(())
}

fn valid_dot_atom(local: &[u8]) -> bool {
    if local.is_empty() || local.len() > 64 {
        return false;
    }
    let mut previous_dot = true;
    for (index, &byte) in local.iter().enumerate() {
        if byte == b'.' {
            if previous_dot || index + 1 == local.len() {
                return false;
            }
            previous_dot = true;
        } else {
            if !(byte.is_ascii_alphanumeric() || b"!#$%&'*+-/=?^_`{|}~".contains(&byte)) {
                return false;
            }
            previous_dot = false;
        }
    }
    true
}

fn address_origins(input: &str, local: &[u8], quoted: bool) -> Vec<usize> {
    let raw = input.as_bytes();
    let mut positions = Vec::new();
    let mut index = 0;
    let mut in_quote = false;
    while index < raw.len() && positions.len() < local.len() {
        if quoted && !in_quote && raw[index] == b'"' {
            in_quote = true;
            index += 1;
            continue;
        }
        if in_quote {
            if raw[index] == b'\\' && index + 1 < raw.len() {
                positions.push(index);
                index += 2;
            } else if raw[index] == b'"' {
                in_quote = false;
                index += 1;
            } else {
                positions.push(index);
                index += 1;
            }
        } else if raw[index] == b'@' {
            break;
        } else if raw[index] == b'(' {
            index = skip_comment_unchecked(raw, index);
        } else if raw[index] == b' ' || raw[index] == b'\t' || raw[index] == b'\r' || raw[index] == b'\n' {
            index += 1;
        } else {
            positions.push(index);
            index += 1;
        }
    }
    positions
}

fn skip_comment_unchecked(raw: &[u8], opening: usize) -> usize {
    let mut depth = 1usize;
    let mut index = opening + 1;
    while index < raw.len() && depth > 0 {
        if raw[index] == b'\\' {
            index += 2;
        } else if raw[index] == b'(' {
            depth += 1;
            index += 1;
        } else if raw[index] == b')' {
            depth -= 1;
            index += 1;
        } else {
            index += 1;
        }
    }
    index
}