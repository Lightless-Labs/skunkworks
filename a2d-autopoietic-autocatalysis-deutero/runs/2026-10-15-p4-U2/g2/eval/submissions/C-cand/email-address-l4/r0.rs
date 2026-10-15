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
struct Run {
    start: usize,
    end: usize,
}

struct Cleaned {
    bytes: Vec<u8>,
    positions: Vec<usize>,
    runs: Vec<Run>,
}

fn err_at(kind: fn(usize) -> EmailError, at: usize) -> EmailError {
    kind(at)
}

fn valid_fold(bytes: &[u8], i: usize) -> Option<usize> {
    if bytes.get(i) != Some(&b'\r') || bytes.get(i + 1) != Some(&b'\n') {
        return None;
    }
    let mut j = i + 2;
    if !matches!(bytes.get(j), Some(b' ') | Some(b'\t')) {
        return None;
    }
    while matches!(bytes.get(j), Some(b' ') | Some(b'\t')) {
        j += 1;
    }
    Some(j)
}

fn cleanup(input: &[u8]) -> Result<Cleaned, EmailError> {
    let mut bytes = Vec::with_capacity(input.len());
    let mut positions = Vec::with_capacity(input.len());
    let mut runs = Vec::new();
    let mut i = 0;
    let mut quote_prefix = false;
    let mut quote_closed = false;

    while i < input.len() {
        if quote_prefix && !quote_closed {
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
                continue;
            }
            if b == b'"' {
                quote_closed = true;
            }
            i += 1;
            continue;
        }

        let b = input[i];
        if !quote_prefix && bytes.is_empty() && b == b'"' {
            quote_prefix = true;
            bytes.push(b);
            positions.push(i);
            i += 1;
            continue;
        }

        let construct_start = i;
        let is_ws = b == b' ' || b == b'\t';
        let is_fold = b == b'\r' || b == b'\n';
        let is_comment = b == b'(';
        let is_close = b == b')';

        if is_ws || is_fold || is_comment || is_close {
            if is_fold && valid_fold(input, i).is_none() {
                return Err(EmailError::BadFold { at: i });
            }
            if is_close {
                return Err(EmailError::UnexpectedClose { at: i });
            }

            let mut depth = 0usize;
            let mut outer_open = None;
            let mut in_construct = true;
            let mut j = i;
            while in_construct && j < input.len() {
                let c = input[j];
                if depth > 0 {
                    if c == b'\\' {
                        if j + 1 >= input.len()
                            || !(0x20..=0x7e).contains(&input[j + 1])
                        {
                            return Err(EmailError::CommentEscape { at: j });
                        }
                        j += 2;
                        continue;
                    }
                    if c == b'\r' || c == b'\n' {
                        if let Some(end) = valid_fold(input, j) {
                            j = end;
                            continue;
                        }
                        return Err(EmailError::BadFold { at: j });
                    }
                    if c == b'(' {
                        if depth == 3 {
                            return Err(EmailError::CommentDepth { at: j });
                        }
                        depth += 1;
                        j += 1;
                        continue;
                    }
                    if c == b')' {
                        depth -= 1;
                        j += 1;
                        continue;
                    }
                    if c == b'\t' || (0x20..=0x7e).contains(&c) {
                        j += 1;
                        continue;
                    }
                    return Err(EmailError::CommentChar { at: j });
                }

                if c == b' ' || c == b'\t' {
                    j += 1;
                } else if c == b'\r' || c == b'\n' {
                    if let Some(end) = valid_fold(input, j) {
                        j = end;
                    } else {
                        return Err(EmailError::BadFold { at: j });
                    }
                } else if c == b'(' {
                    outer_open = Some(j);
                    depth = 1;
                    j += 1;
                } else {
                    in_construct = false;
                }
            }
            if depth != 0 {
                return Err(EmailError::UnclosedComment {
                    at: outer_open.unwrap_or(construct_start),
                });
            }
            if j > construct_start {
                runs.push(Run {
                    start: construct_start,
                    end: j,
                });
                i = j;
                continue;
            }
        }

        if b == b'\r' || b == b'\n' {
            return Err(EmailError::BadFold { at: i });
        }
        if b == b')' {
            return Err(EmailError::UnexpectedClose { at: i });
        }
        bytes.push(b);
        positions.push(i);
        i += 1;
    }

    // CFWS boundaries are checked only after all lexical cleanup succeeds.
    for run in &runs {
        let left_at = bytes
            .iter()
            .enumerate()
            .filter(|(_, _)| true)
            .take_while(|(idx, _)| positions[*idx] < run.start)
            .last()
            .map(|(idx, b)| (*b, idx));
        let right_at = bytes
            .iter()
            .enumerate()
            .find(|(idx, _)| positions[*idx] >= run.end)
            .map(|(idx, b)| (*b, idx));

        let at_start = left_at.is_none();
        let at_end = right_at.is_none();
        let touches_at = left_at.is_some_and(|(b, _)| b == b'@')
            || right_at.is_some_and(|(b, _)| b == b'@');
        if !at_start && !at_end && !touches_at {
            return Err(EmailError::CfwsBoundary { at: run.start });
        }
    }

    Ok(Cleaned {
        bytes,
        positions,
        runs,
    })
}

fn pos(cleaned: &Cleaned, index: usize, end_position: usize) -> usize {
    cleaned
        .positions
        .get(index)
        .copied()
        .unwrap_or(end_position)
}

fn atom_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$%&'*+-/=?^_`{|}~".contains(&b)
}

fn atom_valid(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'.' {
            if i == 0 || i + 1 == bytes.len() || bytes[i - 1] == b'.' {
                return false;
            }
        } else if !atom_byte(b) {
            return false;
        }
    }
    true
}

fn parse_cleaned(cleaned: &Cleaned, allow_single_label: bool) -> Result<(Address, usize), EmailError> {
    let bytes = &cleaned.bytes;
    let n = bytes.len();
    let mut quoted_end = None;
    if bytes.first() == Some(&b'"') {
        let mut i = 1;
        while i < n {
            if bytes[i] == b'\\' {
                i += 2;
                continue;
            }
            if bytes[i] == b'"' {
                quoted_end = Some(i);
                break;
            }
            i += 1;
        }
        if quoted_end.is_none() {
            return Err(EmailError::UnclosedQuote {
                at: pos(cleaned, 0, n),
            });
        }
    }

    let protected_end = quoted_end.map(|x| x + 1).unwrap_or(0);
    let mut ats = Vec::new();
    for i in protected_end..n {
        if bytes[i] == b'@' {
            ats.push(i);
        }
    }
    if ats.is_empty() {
        return Err(EmailError::MissingAt);
    }
    if ats.len() > 1 {
        return Err(EmailError::MultipleAt {
            at: pos(cleaned, ats[1], n),
        });
    }
    let at = ats[0];
    if at == 0 {
        return Err(EmailError::EmptyLocal {
            at: pos(cleaned, at, n),
        });
    }

    let (local, quoted, local_origins) = if let Some(qend) = quoted_end {
        if qend + 1 != at {
            return Err(EmailError::TrailingLocal {
                at: pos(cleaned, qend + 1, n),
            });
        }
        let mut decoded = Vec::new();
        let mut origins = Vec::new();
        let mut i = 1;
        while i < qend {
            let b = bytes[i];
            if b == b'\\' {
                if i + 1 >= qend || !matches!(bytes[i + 1], b'"' | b'\\' | b'@') {
                    return Err(EmailError::BadEscape {
                        at: pos(cleaned, i, n),
                    });
                }
                decoded.push(bytes[i + 1]);
                origins.push(pos(cleaned, i, n));
                i += 2;
            } else {
                if !(0x20..=0x7e).contains(&b) {
                    return Err(EmailError::QuotedChar {
                        at: pos(cleaned, i, n),
                    });
                }
                decoded.push(b);
                origins.push(pos(cleaned, i, n));
                i += 1;
            }
        }
        if decoded.is_empty() {
            return Err(EmailError::EmptyLocal {
                at: pos(cleaned, qend, n),
            });
        }
        if decoded.len() > 64 {
            return Err(EmailError::LocalTooLong { at: origins[64] });
        }
        (decoded, true, origins)
    } else {
        let local_bytes = &bytes[..at];
        if local_bytes.len() > 64 {
            return Err(EmailError::LocalTooLong {
                at: pos(cleaned, 64, n),
            });
        }
        for (i, &b) in local_bytes.iter().enumerate() {
            if b == b'.' {
                if i == 0 || i + 1 == local_bytes.len() || local_bytes[i - 1] == b'.' {
                    return Err(EmailError::LocalDot {
                        at: pos(cleaned, i, n),
                    });
                }
            } else if !atom_byte(b) {
                return Err(EmailError::LocalChar {
                    at: pos(cleaned, i, n),
                });
            }
        }
        (
            local_bytes.to_vec(),
            false,
            (0..local_bytes.len()).map(|i| pos(cleaned, i, n)).collect(),
        )
    };

    let domain_start = at + 1;
    let raw_domain = &bytes[domain_start..];
    let mut domain_end = n;
    let root_dot = if raw_domain.last() == Some(&b'.') {
        domain_end -= 1;
        Some(domain_end)
    } else {
        None
    };
    if domain_end == domain_start {
        return Err(EmailError::EmptyDomain {
            at: pos(cleaned, domain_start, n),
        });
    }
    let domain_len = domain_end - domain_start;
    if domain_len > 253 {
        return Err(EmailError::DomainTooLong {
            at: pos(cleaned, domain_start + 253, n),
        });
    }

    let mut labels: Vec<(usize, usize)> = Vec::new();
    let mut start = domain_start;
    for i in domain_start..=domain_end {
        if i == domain_end || bytes[i] == b'.' {
            if start == i {
                let empty_at = if i == domain_end && root_dot.is_some() {
                    pos(cleaned, root_dot.unwrap(), n)
                } else {
                    pos(cleaned, i, n)
                };
                return Err(EmailError::EmptyLabel { at: empty_at });
            }
            let len = i - start;
            if len > 63 {
                return Err(EmailError::LabelTooLong {
                    at: pos(cleaned, start + 63, n),
                });
            }
            if bytes[start] == b'-' {
                return Err(EmailError::LabelHyphen {
                    at: pos(cleaned, start, n),
                });
            }
            if bytes[i - 1] == b'-' {
                return Err(EmailError::LabelHyphen {
                    at: pos(cleaned, i - 1, n),
                });
            }
            for k in start..i {
                if !bytes[k].is_ascii_alphanumeric() && bytes[k] != b'-' {
                    return Err(EmailError::DomainChar {
                        at: pos(cleaned, k, n),
                    });
                }
            }
            labels.push((start, i));
            start = i + 1;
        }
    }
    if labels.len() < 2 && !allow_single_label {
        return Err(EmailError::DomainNeedsDot {
            at: pos(cleaned, domain_start, n),
        });
    }
    let (last_start, last_end) = *labels.last().unwrap();
    if bytes[last_start..last_end].iter().all(u8::is_ascii_digit) {
        return Err(EmailError::NumericTld {
            at: pos(cleaned, last_start, n),
        });
    }

    let local = String::from_utf8(local).unwrap();
    let domain = bytes[domain_start..domain_end]
        .iter()
        .map(|b| b.to_ascii_lowercase() as char)
        .collect::<String>();
    let _ = local_origins;
    Ok((
        Address {
            local,
            domain,
            quoted,
        },
        at,
    ))
}

fn parse_with_policy(input: &str, policy: &Policy, allow_single: bool) -> Result<(Address, usize), EmailError> {
    if input.is_empty() {
        return Err(EmailError::EmptyInput);
    }
    if let Some(at) = input.as_bytes().iter().position(|b| *b >= 128) {
        return Err(EmailError::NonAscii { at });
    }
    let cleaned = cleanup(input.as_bytes())?;
    if cleaned.bytes.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let (mut address, _) = parse_cleaned(&cleaned, allow_single)?;
    let mut origins = Vec::new();
    if cleaned.bytes.first() == Some(&b'"') {
        let qend = cleaned.bytes.iter().enumerate().skip(1).find_map(|(i, b)| {
            if *b == b'\\' {
                None
            } else if *b == b'"' {
                Some(i)
            } else {
                None
            }
        });
        if let Some(qend) = qend {
            let mut i = 1;
            while i < qend {
                if cleaned.bytes[i] == b'\\' {
                    origins.push(pos(&cleaned, i, cleaned.bytes.len()));
                    i += 2;
                } else {
                    origins.push(pos(&cleaned, i, cleaned.bytes.len()));
                    i += 1;
                }
            }
        }
    } else {
        let at = cleaned.bytes.iter().position(|b| *b == b'@').unwrap();
        origins.extend((0..at).map(|i| pos(&cleaned, i, cleaned.bytes.len())));
    }

    if policy.local_case == LocalCase::Lower {
        address.local = address
            .local
            .bytes()
            .map(|b| b.to_ascii_lowercase() as char)
            .collect();
    }

    if policy.strip_subaddress {
        if let Some(plus) = address.local.as_bytes().iter().position(|b| *b == b'+') {
            if plus == 0 {
                return Err(EmailError::EmptyNormalizedLocal {
                    at: origins.get(plus).copied().unwrap_or(input.len()),
                });
            }
            address.local.truncate(plus);
            origins.truncate(plus);
        }
    }

    if !atom_valid(address.local.as_bytes()) {
        address.quoted = true;
    } else if policy.unquote_safe {
        address.quoted = false;
    }
    Ok((address, 0))
}

pub fn parse_email(input: &str) -> Result<Address, EmailError> {
    let policy = Policy::default();
    parse_with_policy(input, &policy, false).map(|x| x.0)
}

pub fn normalize_email(input: &str, policy: &Policy) -> Result<Address, EmailError> {
    parse_with_policy(input, policy, policy.allow_single_label).map(|x| x.0)
}

pub fn format_email(address: &Address) -> String {
    let local = if address.quoted {
        let mut quoted = String::with_capacity(address.local.len() + 2);
        quoted.push('"');
        for ch in address.local.chars() {
            if ch == '"' || ch == '\\' {
                quoted.push('\\');
            }
            quoted.push(ch);
        }
        quoted.push('"');
        quoted
    } else {
        address.local.clone()
    };
    format!("{}@{}", local, address.domain)
}

pub fn deduplicate(inputs: &[&str], policy: &Policy) -> DedupResult {
    use std::collections::HashMap;

    let mut addresses = Vec::new();
    let mut source_indices = Vec::new();
    let mut duplicate_of = vec![None; inputs.len()];
    let mut errors = Vec::new();
    let mut first_by_key: HashMap<(String, String), usize> = HashMap::new();

    for (index, input) in inputs.iter().enumerate() {
        match normalize_email(input, policy) {
            Err(error) => errors.push((index, error)),
            Ok(address) => {
                let key = (address.local.clone(), address.domain.clone());
                if let Some(&first_index) = first_by_key.get(&key) {
                    duplicate_of[index] = Some(first_index);
                } else {
                    first_by_key.insert(key, index);
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