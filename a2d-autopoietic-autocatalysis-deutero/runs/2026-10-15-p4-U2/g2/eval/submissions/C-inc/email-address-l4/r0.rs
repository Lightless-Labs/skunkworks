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

#[derive(Clone, Copy)]
struct Byte {
    value: u8,
    original: usize,
}

struct Cleaned {
    bytes: Vec<Byte>,
    runs: Vec<usize>,
}

fn is_hws(byte: u8) -> bool {
    byte == b' ' || byte == b'\t'
}

fn fold_end(input: &[u8], at: usize) -> Option<usize> {
    if input.get(at) != Some(&b'\r') || input.get(at + 1) != Some(&b'\n') {
        return None;
    }
    let mut end = at + 2;
    if !input.get(end).copied().map(is_hws).unwrap_or(false) {
        return None;
    }
    while input.get(end).copied().map(is_hws).unwrap_or(false) {
        end += 1;
    }
    Some(end)
}

fn cleanup(input: &[u8]) -> Result<Cleaned, EmailError> {
    let mut bytes = Vec::with_capacity(input.len());
    let mut runs = Vec::new();
    let mut i = 0;
    let mut comment_starts: Vec<usize> = Vec::new();
    let mut first_retained_seen = false;
    let mut protected_quote = false;

    while i < input.len() {
        if protected_quote {
            let b = input[i];
            bytes.push(Byte { value: b, original: i });
            i += 1;
            if b == b'\\' {
                if i < input.len() {
                    bytes.push(Byte {
                        value: input[i],
                        original: i,
                    });
                    i += 1;
                }
            } else if b == b'"' {
                protected_quote = false;
            }
            continue;
        }

        if !comment_starts.is_empty() {
            let b = input[i];
            if b == b'\\' {
                let Some(&next) = input.get(i + 1) else {
                    return Err(EmailError::CommentEscape { at: i });
                };
                if !(0x20..=0x7e).contains(&next) {
                    return Err(EmailError::CommentEscape { at: i });
                }
                i += 2;
                continue;
            }
            if b == b'\r' || b == b'\n' {
                if let Some(end) = fold_end(input, i) {
                    i = end;
                    continue;
                }
                return Err(EmailError::BadFold { at: i });
            }
            if b == b'(' {
                if comment_starts.len() == 3 {
                    return Err(EmailError::CommentDepth { at: i });
                }
                comment_starts.push(i);
                i += 1;
                continue;
            }
            if b == b')' {
                comment_starts.pop();
                i += 1;
                continue;
            }
            if !(0x20..=0x7e).contains(&b) && b != b'\t' {
                return Err(EmailError::CommentChar { at: i });
            }
            i += 1;
            continue;
        }

        let b = input[i];
        if b == b'\r' || b == b'\n' {
            if let Some(end) = fold_end(input, i) {
                runs.push(i);
                i = end;
                continue;
            }
            return Err(EmailError::BadFold { at: i });
        }
        if is_hws(b) {
            runs.push(i);
            while i < input.len() && is_hws(input[i]) {
                i += 1;
            }
            continue;
        }
        if b == b'(' {
            runs.push(i);
            comment_starts.push(i);
            i += 1;
            continue;
        }
        if b == b')' {
            return Err(EmailError::UnexpectedClose { at: i });
        }
        if b == b'"' && !first_retained_seen {
            protected_quote = true;
        }
        first_retained_seen = true;
        bytes.push(Byte { value: b, original: i });
        i += 1;
    }

    if let Some(&at) = comment_starts.first() {
        return Err(EmailError::UnclosedComment { at });
    }

    Ok(Cleaned { bytes, runs })
}

fn pos(bytes: &[Byte], index: usize, input_len: usize) -> usize {
    bytes.get(index).map(|b| b.original).unwrap_or(input_len)
}

fn valid_atom(local: &[u8]) -> bool {
    if local.is_empty() || local[0] == b'.' || local[local.len() - 1] == b'.' {
        return false;
    }
    let mut previous_dot = false;
    for &b in local {
        if b == b'.' {
            if previous_dot {
                return false;
            }
            previous_dot = true;
        } else {
            previous_dot = false;
            if !b.is_ascii_alphanumeric()
                && !b"!#$%&'*+-/=?^_`{|}~".contains(&b)
            {
                return false;
            }
        }
    }
    true
}

fn parse_impl(input: &str, allow_single_label: bool) -> Result<(Address, Vec<usize>), EmailError> {
    let raw = input.as_bytes();
    if raw.is_empty() {
        return Err(EmailError::EmptyInput);
    }
    if let Some(at) = raw.iter().position(|&b| b >= 128) {
        return Err(EmailError::NonAscii { at });
    }

    let cleaned = cleanup(raw)?;
    let bytes = &cleaned.bytes;

    for &run_at in &cleaned.runs {
        let left_at = bytes.iter().position(|b| b.original > run_at);
        let right_at = bytes.iter().rposition(|b| b.original < run_at);
        let left_is_at = right_at.map(|i| bytes[i].value == b'@').unwrap_or(false);
        let right_is_at = left_at.map(|i| bytes[i].value == b'@').unwrap_or(false);
        if right_at.is_some() && left_at.is_some() && !left_is_at && !right_is_at {
            return Err(EmailError::CfwsBoundary { at: run_at });
        }
    }

    if bytes.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let mut quote_close = None;
    if bytes[0].value == b'"' {
        let mut i = 1;
        while i < bytes.len() {
            if bytes[i].value == b'\\' {
                i += 2;
                continue;
            }
            if bytes[i].value == b'"' {
                quote_close = Some(i);
                break;
            }
            i += 1;
        }
        if quote_close.is_none() {
            return Err(EmailError::UnclosedQuote {
                at: bytes[0].original,
            });
        }
    }

    let protected_end = quote_close.unwrap_or(0);
    let separators: Vec<usize> = bytes
        .iter()
        .enumerate()
        .filter(|(i, b)| b.value == b'@' && (quote_close.is_none() || *i > protected_end))
        .map(|(i, _)| i)
        .collect();

    if separators.is_empty() {
        return Err(EmailError::MissingAt);
    }
    if separators.len() > 1 {
        return Err(EmailError::MultipleAt {
            at: bytes[separators[1]].original,
        });
    }

    let sep = separators[0];
    if sep == 0 {
        return Err(EmailError::EmptyLocal {
            at: bytes[sep].original,
        });
    }

    let mut local = Vec::new();
    let mut local_origins = Vec::new();
    let quoted = quote_close.is_some();

    if let Some(close) = quote_close {
        if close + 1 != sep {
            return Err(EmailError::TrailingLocal {
                at: pos(bytes, close + 1, input.len()),
            });
        }
        let mut i = 1;
        while i < close {
            let b = bytes[i].value;
            if b == b'\\' {
                let next = bytes.get(i + 1).map(|v| v.value);
                match next {
                    Some(b'"' | b'\\' | b'@') => {
                        local.push(next.unwrap());
                        local_origins.push(bytes[i].original);
                        i += 2;
                    }
                    _ => return Err(EmailError::BadEscape { at: bytes[i].original }),
                }
            } else {
                if !(0x20..=0x7e).contains(&b) {
                    return Err(EmailError::QuotedChar { at: bytes[i].original });
                }
                local.push(b);
                local_origins.push(bytes[i].original);
                i += 1;
            }
        }
        if local.is_empty() {
            return Err(EmailError::EmptyLocal {
                at: bytes[close].original,
            });
        }
        if local.len() > 64 {
            return Err(EmailError::LocalTooLong {
                at: local_origins[64],
            });
        }
    } else {
        if sep > 64 {
            return Err(EmailError::LocalTooLong {
                at: bytes[64].original,
            });
        }
        let mut previous_dot = false;
        for i in 0..sep {
            let b = bytes[i].value;
            if b == b'.' {
                if i == 0 || i + 1 == sep || previous_dot {
                    return Err(EmailError::LocalDot {
                        at: bytes[i].original,
                    });
                }
                previous_dot = true;
            } else {
                previous_dot = false;
                if !b.is_ascii_alphanumeric()
                    && !b"!#$%&'*+-/=?^_`{|}~".contains(&b)
                {
                    return Err(EmailError::LocalChar {
                        at: bytes[i].original,
                    });
                }
            }
            local.push(b);
            local_origins.push(bytes[i].original);
        }
    }

    let domain_start_index = sep + 1;
    let domain_start = pos(bytes, domain_start_index, input.len());
    let mut domain_end = bytes.len();
    let has_root_dot = domain_end > domain_start_index
        && bytes[domain_end - 1].value == b'.';
    if has_root_dot {
        domain_end -= 1;
    }
    if domain_start_index == domain_end {
        let at = if has_root_dot {
            bytes[domain_end].original
        } else {
            domain_start
        };
        return Err(EmailError::EmptyDomain { at });
    }
    let domain_len = domain_end - domain_start_index;
    if domain_len > 253 {
        return Err(EmailError::DomainTooLong {
            at: bytes[domain_start_index + 253].original,
        });
    }

    let mut labels: Vec<(usize, usize)> = Vec::new();
    let mut start = domain_start_index;
    for i in domain_start_index..=domain_end {
        if i == domain_end || bytes[i].value == b'.' {
            labels.push((start, i));
            start = i + 1;
        }
    }

    for &(start, end) in &labels {
        if start == end {
            let at = if start == domain_end && has_root_dot {
                bytes[domain_end].original
            } else {
                pos(bytes, start, input.len())
            };
            return Err(EmailError::EmptyLabel { at });
        }
        if end - start > 63 {
            return Err(EmailError::LabelTooLong {
                at: bytes[start + 63].original,
            });
        }
        if bytes[start].value == b'-' {
            return Err(EmailError::LabelHyphen {
                at: bytes[start].original,
            });
        }
        if bytes[end - 1].value == b'-' {
            return Err(EmailError::LabelHyphen {
                at: bytes[end - 1].original,
            });
        }
        for b in &bytes[start..end] {
            if !b.value.is_ascii_alphanumeric() && b.value != b'-' {
                return Err(EmailError::DomainChar { at: b.original });
            }
        }
    }

    if labels.len() < 2 && !allow_single_label {
        return Err(EmailError::DomainNeedsDot { at: domain_start });
    }
    let (last_start, last_end) = *labels.last().unwrap();
    if bytes[last_start..last_end]
        .iter()
        .all(|b| b.value.is_ascii_digit())
    {
        return Err(EmailError::NumericTld {
            at: bytes[last_start].original,
        });
    }

    let domain: String = bytes[domain_start_index..domain_end]
        .iter()
        .map(|b| (b.value as char).to_ascii_lowercase())
        .collect();

    Ok((
        Address {
            local: String::from_utf8(local).unwrap(),
            domain,
            quoted,
        },
        local_origins,
    ))
}

pub fn parse_email(input: &str) -> Result<Address, EmailError> {
    parse_impl(input, false).map(|(address, _)| address)
}

pub fn normalize_email(input: &str, policy: &Policy) -> Result<Address, EmailError> {
    let (mut address, origins) = parse_impl(input, policy.allow_single_label)?;
    if policy.local_case == LocalCase::Lower {
        address.local.make_ascii_lowercase();
    }
    if policy.strip_subaddress {
        if let Some(plus) = address.local.find('+') {
            if plus == 0 {
                return Err(EmailError::EmptyNormalizedLocal {
                    at: origins.first().copied().unwrap_or(input.len()),
                });
            }
            address.local.truncate(plus);
        }
    }

    let bytes = address.local.as_bytes();
    let safe = valid_atom(bytes);
    if !safe {
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
    use std::collections::HashMap;

    let mut addresses = Vec::new();
    let mut source_indices = Vec::new();
    let mut duplicate_of = vec![None; inputs.len()];
    let mut errors = Vec::new();
    let mut seen: HashMap<(String, String), usize> = HashMap::new();

    for (index, input) in inputs.iter().enumerate() {
        match normalize_email(input, policy) {
            Err(error) => errors.push((index, error)),
            Ok(address) => {
                let key = (address.local.clone(), address.domain.clone());
                if let Some(&first_index) = seen.get(&key) {
                    duplicate_of[index] = Some(first_index);
                } else {
                    seen.insert(key, index);
                    source_indices.push(index);
                    addresses.push(address);
                }
            }
        }
    }

    DedupResult {
        addresses,
        source_indices,
        duplicate_of,
        errors,
    }
}

fn main() {}