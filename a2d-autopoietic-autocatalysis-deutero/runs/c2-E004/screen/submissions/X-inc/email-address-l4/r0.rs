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

fn fold_end(bytes: &[u8], i: usize) -> Option<usize> {
    if bytes.get(i) != Some(&b'\r') || bytes.get(i + 1) != Some(&b'\n') {
        return None;
    }
    let mut j = i + 2;
    if !matches!(bytes.get(j), Some(b' ' | b'\t')) {
        return None;
    }
    while matches!(bytes.get(j), Some(b' ' | b'\t')) {
        j += 1;
    }
    Some(j)
}

fn cleanup(input: &[u8]) -> Result<Cleaned, EmailError> {
    let mut out = Vec::with_capacity(input.len());
    let mut positions = Vec::with_capacity(input.len());
    let mut runs = Vec::new();
    let mut i = 0usize;
    let mut comment_depth = 0usize;
    let mut outer_open = 0usize;
    let mut run_start: Option<usize> = None;
    let mut quote_protected = false;

    while i < input.len() {
        if quote_protected {
            let b = input[i];
            out.push(b);
            positions.push(i);
            if b == b'\\' {
                i += 1;
                if i < input.len() {
                    out.push(input[i]);
                    positions.push(i);
                    i += 1;
                }
                continue;
            }
            if b == b'"' {
                quote_protected = false;
            }
            i += 1;
            continue;
        }

        if comment_depth > 0 {
            let b = input[i];
            if b == b'\\' {
                if i + 1 >= input.len() || !(0x20..=0x7e).contains(&input[i + 1]) {
                    return Err(EmailError::CommentEscape { at: i });
                }
                if run_start.is_none() {
                    run_start = Some(i);
                }
                i += 2;
                continue;
            }
            if b == b'\r' || b == b'\n' {
                if let Some(end) = fold_end(input, i) {
                    if run_start.is_none() {
                        run_start = Some(i);
                    }
                    i = end;
                    continue;
                }
                return Err(EmailError::BadFold { at: i });
            }
            if b == b'(' {
                if comment_depth == 3 {
                    return Err(EmailError::CommentDepth { at: i });
                }
                comment_depth += 1;
                i += 1;
                continue;
            }
            if b == b')' {
                comment_depth -= 1;
                i += 1;
                if comment_depth == 0 {
                    if let Some(start) = run_start.take() {
                        runs.push(Run { start, end: i });
                    }
                }
                continue;
            }
            if b == b' ' || b == b'\t' {
                if run_start.is_none() {
                    run_start = Some(i);
                }
                i += 1;
                continue;
            }
            if !(0x20..=0x7e).contains(&b) {
                return Err(EmailError::CommentChar { at: i });
            }
            i += 1;
            continue;
        }

        let b = input[i];

        if b == b'\r' || b == b'\n' {
            if let Some(end) = fold_end(input, i) {
                if run_start.is_none() {
                    run_start = Some(i);
                }
                i = end;
                continue;
            }
            return Err(EmailError::BadFold { at: i });
        }

        if b == b' ' || b == b'\t' {
            if run_start.is_none() {
                run_start = Some(i);
            }
            i += 1;
            continue;
        }

        if b == b'(' {
            if run_start.is_none() {
                run_start = Some(i);
            }
            comment_depth = 1;
            outer_open = i;
            i += 1;
            continue;
        }

        if b == b')' {
            return Err(EmailError::UnexpectedClose { at: i });
        }

        if b == b'"' && out.is_empty() {
            if let Some(start) = run_start.take() {
                runs.push(Run { start, end: i });
            }
            quote_protected = true;
            out.push(b);
            positions.push(i);
            i += 1;
            continue;
        }

        if let Some(start) = run_start.take() {
            runs.push(Run { start, end: i });
        }
        out.push(b);
        positions.push(i);
        i += 1;
    }

    if comment_depth > 0 {
        return Err(EmailError::UnclosedComment { at: outer_open });
    }
    if let Some(start) = run_start {
        runs.push(Run {
            start,
            end: input.len(),
        });
    }

    for run in &runs {
        let before = out
            .iter()
            .zip(&positions)
            .rev()
            .find(|(_, p)| **p < run.start)
            .map(|(b, _)| *b);
        let after = out
            .iter()
            .zip(&positions)
            .find(|(_, p)| **p >= run.end)
            .map(|(b, _)| *b);
        let at_boundary = before.is_none()
            || after.is_none()
            || before == Some(b'@')
            || after == Some(b'@');
        if !at_boundary {
            return Err(EmailError::CfwsBoundary { at: run.start });
        }
    }

    Ok(Cleaned {
        bytes: out,
        positions,
        runs,
    })
}

fn pos(cleaned: &Cleaned, idx: usize, input_len: usize) -> usize {
    cleaned.positions.get(idx).copied().unwrap_or(input_len)
}

fn valid_dot_atom(local: &[u8]) -> bool {
    if local.is_empty() || local.len() > 64 {
        return false;
    }
    let specials = b"!#$%&'*+-/=?^_`{|}~";
    let mut prev_dot = true;
    for (i, &b) in local.iter().enumerate() {
        if b == b'.' {
            if prev_dot || i + 1 == local.len() {
                return false;
            }
            prev_dot = true;
        } else {
            if !b.is_ascii_alphanumeric() && !specials.contains(&b) {
                return false;
            }
            prev_dot = false;
        }
    }
    true
}

fn parse_core(input: &str, allow_single_label: bool) -> Result<(Address, Vec<usize>), EmailError> {
    let raw = input.as_bytes();
    if raw.is_empty() {
        return Err(EmailError::EmptyInput);
    }
    if let Some(at) = raw.iter().position(|b| *b >= 128) {
        return Err(EmailError::NonAscii { at });
    }

    let cleaned = cleanup(raw)?;
    let bytes = &cleaned.bytes;
    if bytes.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let mut protected_end = 0usize;
    let quoted_prefix = bytes.first() == Some(&b'"');
    if quoted_prefix {
        let mut i = 1usize;
        let mut close = None;
        while i < bytes.len() {
            if bytes[i] == b'\\' {
                i += 2;
                continue;
            }
            if bytes[i] == b'"' {
                close = Some(i);
                break;
            }
            i += 1;
        }
        let Some(end) = close else {
            return Err(EmailError::UnclosedQuote {
                at: pos(&cleaned, 0, raw.len()),
            });
        };
        protected_end = end + 1;
    }

    let separators: Vec<usize> = bytes
        .iter()
        .enumerate()
        .filter_map(|(i, b)| {
            if *b == b'@' && (!quoted_prefix || i >= protected_end) {
                Some(i)
            } else {
                None
            }
        })
        .collect();

    if separators.is_empty() {
        return Err(EmailError::MissingAt);
    }
    if separators.len() > 1 {
        return Err(EmailError::MultipleAt {
            at: pos(&cleaned, separators[1], raw.len()),
        });
    }

    let sep = separators[0];
    if sep == 0 {
        return Err(EmailError::EmptyLocal {
            at: pos(&cleaned, sep, raw.len()),
        });
    }

    let local_bytes = &bytes[..sep];
    let mut local = Vec::new();
    let mut local_origins = Vec::new();
    let quoted = local_bytes.first() == Some(&b'"');

    if !quoted {
        if local_bytes.len() > 64 {
            return Err(EmailError::LocalTooLong {
                at: pos(&cleaned, 64, raw.len()),
            });
        }
        let specials = b"!#$%&'*+-/=?^_`{|}~";
        let mut prev_dot = false;
        for (i, &b) in local_bytes.iter().enumerate() {
            let at = pos(&cleaned, i, raw.len());
            if b == b'.' {
                if i == 0 || i + 1 == local_bytes.len() || prev_dot {
                    return Err(EmailError::LocalDot { at });
                }
                prev_dot = true;
            } else {
                if !b.is_ascii_alphanumeric() && !specials.contains(&b) {
                    return Err(EmailError::LocalChar { at });
                }
                prev_dot = false;
            }
            local.push(b);
            local_origins.push(at);
        }
    } else {
        let mut close = None;
        let mut i = 1usize;
        while i < local_bytes.len() {
            if local_bytes[i] == b'\\' {
                i += 2;
                continue;
            }
            if local_bytes[i] == b'"' {
                close = Some(i);
                break;
            }
            i += 1;
        }
        let end = close.unwrap_or(0);
        if end + 1 != local_bytes.len() {
            return Err(EmailError::TrailingLocal {
                at: pos(&cleaned, end + 1, raw.len()),
            });
        }

        i = 1;
        while i < end {
            let b = local_bytes[i];
            let origin = pos(&cleaned, i, raw.len());
            if b == b'\\' {
                if i + 1 >= end {
                    return Err(EmailError::BadEscape { at: origin });
                }
                let next = local_bytes[i + 1];
                if !matches!(next, b'"' | b'\\' | b'@') {
                    return Err(EmailError::BadEscape { at: origin });
                }
                local.push(next);
                local_origins.push(origin);
                i += 2;
            } else {
                if !(0x20..=0x7e).contains(&b) {
                    return Err(EmailError::QuotedChar { at: origin });
                }
                local.push(b);
                local_origins.push(origin);
                i += 1;
            }
        }
        if local.is_empty() {
            return Err(EmailError::EmptyLocal {
                at: pos(&cleaned, end, raw.len()),
            });
        }
        if local.len() > 64 {
            return Err(EmailError::LocalTooLong {
                at: local_origins[64],
            });
        }
    }

    let domain_start = sep + 1;
    let raw_domain = &bytes[domain_start..];
    let mut domain_end = bytes.len();
    if domain_end > domain_start && bytes[domain_end - 1] == b'.' {
        domain_end -= 1;
    }
    if domain_end == domain_start {
        return Err(EmailError::EmptyDomain {
            at: pos(&cleaned, domain_start, raw.len()),
        });
    }
    let domain_len = domain_end - domain_start;
    if domain_len > 253 {
        return Err(EmailError::DomainTooLong {
            at: pos(&cleaned, domain_start + 253, raw.len()),
        });
    }

    let mut labels: Vec<(usize, usize)> = Vec::new();
    let mut label_start = domain_start;
    for i in domain_start..domain_end {
        if bytes[i] == b'.' {
            labels.push((label_start, i));
            label_start = i + 1;
        }
    }
    labels.push((label_start, domain_end));

    for (start, end) in &labels {
        if start == end {
            let at = if *start == domain_end && domain_end < bytes.len() {
                pos(&cleaned, domain_end, raw.len())
            } else {
                pos(&cleaned, *start, raw.len())
            };
            return Err(EmailError::EmptyLabel { at });
        }
        let len = end - start;
        if len > 63 {
            return Err(EmailError::LabelTooLong {
                at: pos(&cleaned, start + 63, raw.len()),
            });
        }
        if bytes[*start] == b'-' {
            return Err(EmailError::LabelHyphen {
                at: pos(&cleaned, *start, raw.len()),
            });
        }
        if bytes[*end - 1] == b'-' {
            return Err(EmailError::LabelHyphen {
                at: pos(&cleaned, end - 1, raw.len()),
            });
        }
        for i in *start..*end {
            let b = bytes[i];
            if !b.is_ascii_alphanumeric() && b != b'-' {
                return Err(EmailError::DomainChar {
                    at: pos(&cleaned, i, raw.len()),
                });
            }
        }
    }

    if labels.len() < 2 && !allow_single_label {
        return Err(EmailError::DomainNeedsDot {
            at: pos(&cleaned, domain_start, raw.len()),
        });
    }

    let (last_start, last_end) = *labels.last().unwrap();
    if bytes[last_start..last_end].iter().all(u8::is_ascii_digit) {
        return Err(EmailError::NumericTld {
            at: pos(&cleaned, last_start, raw.len()),
        });
    }

    let mut domain = Vec::with_capacity(domain_len);
    for &b in &bytes[domain_start..domain_end] {
        domain.push(b.to_ascii_lowercase());
    }

    let _ = raw_domain;
    Ok((
        Address {
            local: String::from_utf8(local).unwrap(),
            domain: String::from_utf8(domain).unwrap(),
            quoted,
        },
        local_origins,
    ))
}

pub fn parse_email(input: &str) -> Result<Address, EmailError> {
    parse_core(input, false).map(|(address, _)| address)
}

pub fn normalize_email(input: &str, policy: &Policy) -> Result<Address, EmailError> {
    let (mut address, origins) = parse_core(input, policy.allow_single_label)?;
    if policy.local_case == LocalCase::Lower {
        address.local.make_ascii_lowercase();
    }
    if policy.strip_subaddress {
        if let Some(index) = address.local.as_bytes().iter().position(|b| *b == b'+') {
            if index == 0 {
                return Err(EmailError::EmptyNormalizedLocal {
                    at: origins.get(index).copied().unwrap_or(input.len()),
                });
            }
            address.local.truncate(index);
        }
    }
    if !valid_dot_atom(address.local.as_bytes()) {
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
                if let Some(&first) = seen.get(&key) {
                    duplicate_of[index] = Some(first);
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