use std::collections::HashMap;

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

struct Cleaned {
    bytes: Vec<u8>,
    positions: Vec<usize>,
    runs: Vec<(usize, usize)>,
}

fn is_hws(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

fn fold_end(input: &[u8], i: usize) -> Option<usize> {
    if input.get(i) != Some(&b'\r') || input.get(i + 1) != Some(&b'\n') {
        return None;
    }
    let mut j = i + 2;
    if !input.get(j).copied().map(is_hws).unwrap_or(false) {
        return None;
    }
    while input.get(j).copied().map(is_hws).unwrap_or(false) {
        j += 1;
    }
    Some(j)
}

fn consume_comment(input: &[u8], start: usize) -> Result<usize, EmailError> {
    let mut i = start;
    let mut depth = 0usize;
    let mut openings = Vec::new();

    while i < input.len() {
        let b = input[i];
        if b == b'\\' {
            if i + 1 >= input.len() || !(0x20..=0x7e).contains(&input[i + 1]) {
                return Err(EmailError::CommentEscape { at: i });
            }
            i += 2;
        } else if b == b'(' {
            depth += 1;
            if depth > 3 {
                return Err(EmailError::CommentDepth { at: i });
            }
            openings.push(i);
            i += 1;
        } else if b == b')' {
            depth -= 1;
            openings.pop();
            i += 1;
            if depth == 0 {
                return Ok(i);
            }
        } else if b == b'\r' || b == b'\n' {
            if let Some(end) = fold_end(input, i) {
                i = end;
            } else {
                return Err(EmailError::BadFold { at: i });
            }
        } else if b == b'\t' || (0x20..=0x7e).contains(&b) {
            i += 1;
        } else {
            return Err(EmailError::CommentChar { at: i });
        }
    }

    Err(EmailError::UnclosedComment {
        at: openings.first().copied().unwrap_or(start),
    })
}

fn cleanup(input: &[u8]) -> Result<Cleaned, EmailError> {
    let mut bytes = Vec::with_capacity(input.len());
    let mut positions = Vec::with_capacity(input.len());
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    let mut protected = false;
    let mut quote_escape = false;

    while i < input.len() {
        let b = input[i];

        if protected {
            bytes.push(b);
            positions.push(i);
            if quote_escape {
                quote_escape = false;
            } else if b == b'\\' {
                quote_escape = true;
            } else if b == b'"' {
                protected = false;
            }
            i += 1;
            continue;
        }

        if bytes.is_empty() && b == b'"' {
            protected = true;
            bytes.push(b);
            positions.push(i);
            i += 1;
            continue;
        }

        if is_hws(b) || b == b'(' || b == b'\r' || b == b'\n' {
            let start = i;
            let retained = bytes.len();

            loop {
                if i >= input.len() {
                    break;
                }
                match input[i] {
                    c if is_hws(c) => i += 1,
                    b'\r' | b'\n' => {
                        if let Some(end) = fold_end(input, i) {
                            i = end;
                        } else {
                            return Err(EmailError::BadFold { at: i });
                        }
                    }
                    b'(' => i = consume_comment(input, i)?,
                    _ => break,
                }
            }
            runs.push((start, retained));
            continue;
        }

        if b == b')' {
            return Err(EmailError::UnexpectedClose { at: i });
        }
        if b == b'\r' || b == b'\n' {
            return Err(EmailError::BadFold { at: i });
        }

        bytes.push(b);
        positions.push(i);
        i += 1;
    }

    for &(start, retained) in &runs {
        let left_at = retained > 0 && bytes.get(retained - 1) == Some(&b'@');
        let right_at = bytes.get(retained) == Some(&b'@');
        if retained != 0 && retained != bytes.len() && !left_at && !right_at {
            return Err(EmailError::CfwsBoundary { at: start });
        }
    }

    Ok(Cleaned {
        bytes,
        positions,
        runs,
    })
}

fn original_pos(clean: &Cleaned, index: usize, input_len: usize) -> usize {
    clean.positions.get(index).copied().unwrap_or(input_len)
}

fn local_atom_valid(bytes: &[u8]) -> bool {
    if bytes.is_empty() || bytes.len() > 64 {
        return false;
    }
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'.' {
            if i == 0 || i + 1 == bytes.len() || bytes[i - 1] == b'.' {
                return false;
            }
        } else if !b.is_ascii_alphanumeric() && !b"!#$%&'*+-/=?^_`{|}~".contains(&b) {
            return false;
        }
    }
    true
}

fn parse_inner(
    input: &str,
    allow_single_label: bool,
) -> Result<(Address, Vec<usize>), EmailError> {
    let raw = input.as_bytes();
    if raw.is_empty() {
        return Err(EmailError::EmptyInput);
    }
    if let Some(i) = raw.iter().position(|b| *b >= 128) {
        return Err(EmailError::NonAscii { at: i });
    }

    let clean = cleanup(raw)?;
    if clean.bytes.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let mut quote_end = None;
    if clean.bytes[0] == b'"' {
        let mut i = 1;
        while i < clean.bytes.len() {
            if clean.bytes[i] == b'\\' {
                i += 2;
            } else if clean.bytes[i] == b'"' {
                quote_end = Some(i);
                break;
            } else {
                i += 1;
            }
        }
        if quote_end.is_none() {
            return Err(EmailError::UnclosedQuote {
                at: original_pos(&clean, 0, raw.len()),
            });
        }
    }

    let mut ats = Vec::new();
    let mut i = 0;
    while i < clean.bytes.len() {
        if let Some(end) = quote_end {
            if i <= end {
                i = end + 1;
                continue;
            }
        }
        if clean.bytes[i] == b'@' {
            ats.push(i);
        }
        i += 1;
    }

    if ats.is_empty() {
        return Err(EmailError::MissingAt);
    }
    if ats.len() > 1 {
        return Err(EmailError::MultipleAt {
            at: original_pos(&clean, ats[1], raw.len()),
        });
    }

    let at_idx = ats[0];
    if at_idx == 0 {
        return Err(EmailError::EmptyLocal {
            at: original_pos(&clean, at_idx, raw.len()),
        });
    }

    let quoted = clean.bytes[0] == b'"';
    let mut decoded = Vec::new();
    let mut origins = Vec::new();

    if !quoted {
        let local = &clean.bytes[..at_idx];
        if local.len() > 64 {
            return Err(EmailError::LocalTooLong {
                at: original_pos(&clean, 64, raw.len()),
            });
        }

        for (j, &b) in local.iter().enumerate() {
            if b == b'.' {
                if j == 0 || j + 1 == local.len() || local[j - 1] == b'.' {
                    return Err(EmailError::LocalDot {
                        at: original_pos(&clean, j, raw.len()),
                    });
                }
            } else if !b.is_ascii_alphanumeric() && !b"!#$%&'*+-/=?^_`{|}~".contains(&b) {
                return Err(EmailError::LocalChar {
                    at: original_pos(&clean, j, raw.len()),
                });
            }
            decoded.push(b);
            origins.push(original_pos(&clean, j, raw.len()));
        }
    } else {
        let end = quote_end.unwrap();
        if end + 1 != at_idx {
            return Err(EmailError::TrailingLocal {
                at: original_pos(&clean, end + 1, raw.len()),
            });
        }

        let mut j = 1;
        while j < end {
            let b = clean.bytes[j];
            if b == b'\\' {
                let next = clean.bytes.get(j + 1).copied().unwrap_or(0);
                if next != b'"' && next != b'\\' && next != b'@' {
                    return Err(EmailError::BadEscape {
                        at: original_pos(&clean, j, raw.len()),
                    });
                }
                decoded.push(next);
                origins.push(original_pos(&clean, j, raw.len()));
                j += 2;
            } else {
                if !(0x20..=0x7e).contains(&b) {
                    return Err(EmailError::QuotedChar {
                        at: original_pos(&clean, j, raw.len()),
                    });
                }
                decoded.push(b);
                origins.push(original_pos(&clean, j, raw.len()));
                j += 1;
            }
        }

        if decoded.is_empty() {
            return Err(EmailError::EmptyLocal {
                at: original_pos(&clean, end, raw.len()),
            });
        }
        if decoded.len() > 64 {
            return Err(EmailError::LocalTooLong { at: origins[64] });
        }
    }

    let domain_start = at_idx + 1;
    let mut domain_end = clean.bytes.len();
    let mut root_dot = None;
    if domain_end > domain_start && clean.bytes[domain_end - 1] == b'.' {
        domain_end -= 1;
        root_dot = Some(domain_end);
    }

    if domain_end == domain_start {
        return Err(EmailError::EmptyDomain {
            at: original_pos(&clean, domain_start, raw.len()),
        });
    }

    let domain_len = domain_end - domain_start;
    if domain_len > 253 {
        return Err(EmailError::DomainTooLong {
            at: original_pos(&clean, domain_start + 253, raw.len()),
        });
    }

    let mut labels = Vec::new();
    let mut label_start = domain_start;
    for idx in domain_start..domain_end {
        if clean.bytes[idx] == b'.' {
            labels.push((label_start, idx));
            label_start = idx + 1;
        }
    }
    labels.push((label_start, domain_end));

    for &(start, end) in &labels {
        if start == end {
            let at = if start == domain_end {
                root_dot
                    .map(|x| original_pos(&clean, x, raw.len()))
                    .unwrap_or(raw.len())
            } else {
                original_pos(&clean, start, raw.len())
            };
            return Err(EmailError::EmptyLabel { at });
        }

        if end - start > 63 {
            return Err(EmailError::LabelTooLong {
                at: original_pos(&clean, start + 63, raw.len()),
            });
        }

        if clean.bytes[start] == b'-' {
            return Err(EmailError::LabelHyphen {
                at: original_pos(&clean, start, raw.len()),
            });
        }
        if clean.bytes[end - 1] == b'-' {
            return Err(EmailError::LabelHyphen {
                at: original_pos(&clean, end - 1, raw.len()),
            });
        }

        for idx in start..end {
            let b = clean.bytes[idx];
            if !b.is_ascii_alphanumeric() && b != b'-' {
                return Err(EmailError::DomainChar {
                    at: original_pos(&clean, idx, raw.len()),
                });
            }
        }
    }

    if labels.len() < 2 && !allow_single_label {
        return Err(EmailError::DomainNeedsDot {
            at: original_pos(&clean, domain_start, raw.len()),
        });
    }

    let (last_start, last_end) = *labels.last().unwrap();
    if clean.bytes[last_start..last_end]
        .iter()
        .all(|b| b.is_ascii_digit())
    {
        return Err(EmailError::NumericTld {
            at: original_pos(&clean, last_start, raw.len()),
        });
    }

    let local = String::from_utf8(decoded).unwrap();
    let domain = String::from_utf8(clean.bytes[domain_start..domain_end].to_vec())
        .unwrap()
        .to_ascii_lowercase();

    Ok((
        Address {
            local,
            domain,
            quoted,
        },
        origins,
    ))
}

pub fn parse_email(input: &str) -> Result<Address, EmailError> {
    parse_inner(input, false).map(|(address, _)| address)
}

pub fn normalize_email(input: &str, policy: &Policy) -> Result<Address, EmailError> {
    let (mut address, origins) = parse_inner(input, policy.allow_single_label)?;
    let mut local = address.local.into_bytes();

    if policy.local_case == LocalCase::Lower {
        for b in &mut local {
            if b.is_ascii_uppercase() {
                *b = b.to_ascii_lowercase();
            }
        }
    }

    if policy.strip_subaddress {
        if let Some(plus) = local.iter().position(|b| *b == b'+') {
            if plus == 0 {
                return Err(EmailError::EmptyNormalizedLocal {
                    at: origins.first().copied().unwrap_or(input.len()),
                });
            }
            local.truncate(plus);
        }
    }

    let atom_safe = local_atom_valid(&local);
    address.local = String::from_utf8(local).unwrap();

    if !atom_safe {
        address.quoted = true;
    } else if policy.unquote_safe {
        address.quoted = false;
    }

    Ok(address)
}

pub fn format_email(address: &Address) -> String {
    let mut out = String::new();
    if address.quoted {
        out.push('"');
        for ch in address.local.chars() {
            if ch == '"' || ch == '\\' {
                out.push('\\');
            }
            out.push(ch);
        }
        out.push('"');
    } else {
        out.push_str(&address.local);
    }
    out.push('@');
    out.push_str(&address.domain);
    out
}

pub fn deduplicate(inputs: &[&str], policy: &Policy) -> DedupResult {
    let mut addresses = Vec::new();
    let mut source_indices = Vec::new();
    let mut duplicate_of = vec![None; inputs.len()];
    let mut errors = Vec::new();
    let mut first_by_key: HashMap<(String, String), usize> = HashMap::new();

    for (index, input) in inputs.iter().enumerate() {
        match normalize_email(input, policy) {
            Ok(address) => {
                let key = (address.local.clone(), address.domain.clone());
                if let Some(&first_index) = first_by_key.get(&key) {
                    duplicate_of[index] = Some(first_index);
                } else {
                    first_by_key.insert(key, index);
                    addresses.push(address);
                    source_indices.push(index);
                }
            }
            Err(error) => errors.push((index, error)),
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