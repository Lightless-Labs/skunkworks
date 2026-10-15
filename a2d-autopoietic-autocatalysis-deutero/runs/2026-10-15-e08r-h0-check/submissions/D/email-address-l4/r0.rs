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

fn is_hws(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

fn fold_end(bytes: &[u8], i: usize) -> Option<usize> {
    if i + 2 >= bytes.len() || bytes[i] != b'\r' || bytes[i + 1] != b'\n' {
        return None;
    }
    if !is_hws(bytes[i + 2]) {
        return None;
    }
    let mut j = i + 3;
    while j < bytes.len() && is_hws(bytes[j]) {
        j += 1;
    }
    Some(j)
}

fn cleanup(input: &str) -> Result<(Vec<u8>, Vec<usize>, Vec<Run>), EmailError> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut map = Vec::with_capacity(bytes.len());
    let mut runs = Vec::new();
    let mut i = 0usize;
    let mut first_retained = true;
    let mut quote_protected = false;

    while i < bytes.len() {
        let b = bytes[i];

        if quote_protected {
            out.push(b);
            map.push(i);
            if b == b'\\' {
                if i + 1 < bytes.len() {
                    out.push(bytes[i + 1]);
                    map.push(i + 1);
                    i += 2;
                } else {
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

        let run_start = i;
        let mut had_run = false;

        loop {
            if i >= bytes.len() {
                break;
            }
            let c = bytes[i];
            if is_hws(c) {
                had_run = true;
                i += 1;
                continue;
            }
            if c == b'\r' || c == b'\n' {
                had_run = true;
                if let Some(end) = fold_end(bytes, i) {
                    i = end;
                    continue;
                }
                return Err(EmailError::BadFold { at: i });
            }
            if c == b'(' {
                had_run = true;
                let outer = i;
                let mut depth = 0usize;
                while i < bytes.len() {
                    let x = bytes[i];
                    if x == b'\\' {
                        let slash = i;
                        if i + 1 >= bytes.len()
                            || !(0x20..=0x7e).contains(&bytes[i + 1])
                        {
                            return Err(EmailError::CommentEscape { at: slash });
                        }
                        i += 2;
                    } else if x == b'\r' || x == b'\n' {
                        if let Some(end) = fold_end(bytes, i) {
                            i = end;
                        } else {
                            return Err(EmailError::BadFold { at: i });
                        }
                    } else if x == b'(' {
                        depth += 1;
                        if depth > 3 {
                            return Err(EmailError::CommentDepth { at: i });
                        }
                        i += 1;
                    } else if x == b')' {
                        depth -= 1;
                        i += 1;
                        if depth == 0 {
                            break;
                        }
                    } else if x == b'\t' || (0x20..=0x7e).contains(&x) {
                        i += 1;
                    } else {
                        return Err(EmailError::CommentChar { at: i });
                    }
                }
                if depth != 0 {
                    return Err(EmailError::UnclosedComment { at: outer });
                }
                continue;
            }
            if c == b')' {
                return Err(EmailError::UnexpectedClose { at: i });
            }
            break;
        }

        if had_run {
            runs.push(Run {
                start: run_start,
                end: i,
            });
        }

        if i >= bytes.len() {
            break;
        }

        let c = bytes[i];
        if first_retained && c == b'"' {
            quote_protected = true;
        }
        out.push(c);
        map.push(i);
        first_retained = false;
        i += 1;
    }

    Ok((out, map, runs))
}

fn pos(map: &[usize], index: usize, input_len: usize) -> usize {
    map.get(index).copied().unwrap_or(input_len)
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
        } else if !b.is_ascii_alphanumeric()
            && !b"!#$%&'*+-/=?^_`{|}~".contains(&b)
        {
            return false;
        }
    }
    true
}

fn parse_with_options(
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

    let (clean, map, runs) = cleanup(input)?;
    for run in &runs {
        let left = run.start == 0
            || map.iter().any(|&p| p + 1 == run.start);
        let right = run.end == raw.len()
            || map.iter().any(|&p| p == run.end);
        let clean_left = map.iter().any(|&p| p < run.start);
        let clean_right = map.iter().any(|&p| p >= run.end);
        let at_left = clean_left && {
            let idx = map.iter().rposition(|&p| p < run.start).unwrap();
            clean[idx] == b'@'
        };
        let at_right = clean_right && {
            let idx = map.iter().position(|&p| p >= run.end).unwrap();
            clean[idx] == b'@'
        };
        if !clean_left || !clean_right || at_left || at_right {
            continue;
        }
        // A run at the actual input boundary is legal even if empty input
        // remains on that side after cleanup.
        if run.start == 0 || run.end == raw.len() {
            continue;
        }
        return Err(EmailError::CfwsBoundary { at: run.start });
    }

    if clean.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let mut quoted_end = None;
    if clean[0] == b'"' {
        let mut i = 1usize;
        while i < clean.len() {
            if clean[i] == b'\\' {
                i += 2;
            } else if clean[i] == b'"' {
                quoted_end = Some(i);
                break;
            } else {
                i += 1;
            }
        }
        if quoted_end.is_none() {
            return Err(EmailError::UnclosedQuote { at: pos(&map, 0, raw.len()) });
        }
    }

    let mut ats = Vec::new();
    let protected_through = quoted_end.unwrap_or(0);
    for i in 0..clean.len() {
        if clean[i] == b'@' && (quoted_end.is_none() || i > protected_through) {
            ats.push(i);
        }
    }
    if ats.is_empty() {
        return Err(EmailError::MissingAt);
    }
    if ats.len() > 1 {
        return Err(EmailError::MultipleAt {
            at: pos(&map, ats[1], raw.len()),
        });
    }

    let sep = ats[0];
    if sep == 0 {
        return Err(EmailError::EmptyLocal {
            at: pos(&map, sep, raw.len()),
        });
    }

    let mut decoded = Vec::new();
    let mut origins = Vec::new();
    let quoted;
    if clean[0] == b'"' {
        let end = quoted_end.unwrap();
        if end + 1 != sep {
            return Err(EmailError::TrailingLocal {
                at: pos(&map, end + 1, raw.len()),
            });
        }
        quoted = true;
        let mut i = 1usize;
        while i < end {
            let b = clean[i];
            if b == b'\\' {
                if i + 1 >= end
                    || !matches!(clean[i + 1], b'"' | b'\\' | b'@')
                {
                    return Err(EmailError::BadEscape {
                        at: pos(&map, i, raw.len()),
                    });
                }
                decoded.push(clean[i + 1]);
                origins.push(pos(&map, i, raw.len()));
                i += 2;
            } else {
                if !(0x20..=0x7e).contains(&b) {
                    return Err(EmailError::QuotedChar {
                        at: pos(&map, i, raw.len()),
                    });
                }
                decoded.push(b);
                origins.push(pos(&map, i, raw.len()));
                i += 1;
            }
        }
        if decoded.is_empty() {
            return Err(EmailError::EmptyLocal {
                at: pos(&map, end, raw.len()),
            });
        }
        if decoded.len() > 64 {
            return Err(EmailError::LocalTooLong {
                at: origins[64],
            });
        }
    } else {
        quoted = false;
        if sep > 64 {
            return Err(EmailError::LocalTooLong {
                at: pos(&map, 64, raw.len()),
            });
        }
        for i in 0..sep {
            let b = clean[i];
            if b == b'.' {
                if i == 0 || i + 1 == sep || clean[i - 1] == b'.' {
                    return Err(EmailError::LocalDot {
                        at: pos(&map, i, raw.len()),
                    });
                }
            } else if !b.is_ascii_alphanumeric()
                && !b"!#$%&'*+-/=?^_`{|}~".contains(&b)
            {
                return Err(EmailError::LocalChar {
                    at: pos(&map, i, raw.len()),
                });
            }
            decoded.push(b);
            origins.push(pos(&map, i, raw.len()));
        }
    }

    let domain_start = sep + 1;
    let mut domain_end = clean.len();
    let mut root_pos = None;
    if domain_end > domain_start && clean[domain_end - 1] == b'.' {
        domain_end -= 1;
        root_pos = Some(pos(&map, domain_end, raw.len()));
    }
    if domain_end == domain_start {
        return Err(EmailError::EmptyDomain {
            at: pos(&map, domain_start, raw.len()),
        });
    }
    if domain_end - domain_start > 253 {
        return Err(EmailError::DomainTooLong {
            at: pos(&map, domain_start + 253, raw.len()),
        });
    }

    let mut labels: Vec<(usize, usize)> = Vec::new();
    let mut start = domain_start;
    for i in domain_start..domain_end {
        if clean[i] == b'.' {
            labels.push((start, i));
            start = i + 1;
        }
    }
    labels.push((start, domain_end));

    for (idx, &(s, e)) in labels.iter().enumerate() {
        if s == e {
            let p = if idx + 1 == labels.len() && root_pos.is_some() {
                root_pos.unwrap()
            } else if s < clean.len() {
                pos(&map, s, raw.len())
            } else {
                pos(&map, s, raw.len())
            };
            return Err(EmailError::EmptyLabel { at: p });
        }
        if e - s > 63 {
            return Err(EmailError::LabelTooLong {
                at: pos(&map, s + 63, raw.len()),
            });
        }
        if clean[s] == b'-' {
            return Err(EmailError::LabelHyphen {
                at: pos(&map, s, raw.len()),
            });
        }
        if clean[e - 1] == b'-' {
            return Err(EmailError::LabelHyphen {
                at: pos(&map, e - 1, raw.len()),
            });
        }
        for i in s..e {
            let b = clean[i];
            if !b.is_ascii_alphanumeric() && b != b'-' {
                return Err(EmailError::DomainChar {
                    at: pos(&map, i, raw.len()),
                });
            }
        }
    }

    if labels.len() < 2 && !allow_single_label {
        return Err(EmailError::DomainNeedsDot {
            at: pos(&map, domain_start, raw.len()),
        });
    }

    let (last_start, last_end) = *labels.last().unwrap();
    if clean[last_start..last_end].iter().all(u8::is_ascii_digit) {
        return Err(EmailError::NumericTld {
            at: pos(&map, last_start, raw.len()),
        });
    }

    let local = String::from_utf8(decoded).unwrap();
    let domain = clean[domain_start..domain_end]
        .iter()
        .map(|b| b.to_ascii_lowercase() as char)
        .collect::<String>();
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
    parse_with_options(input, false).map(|(address, _)| address)
}

pub fn normalize_email(input: &str, policy: &Policy) -> Result<Address, EmailError> {
    let (mut address, origins) = parse_with_options(input, policy.allow_single_label)?;
    if policy.local_case == LocalCase::Lower {
        address.local = address
            .local
            .bytes()
            .map(|b| b.to_ascii_lowercase() as char)
            .collect();
    }

    if policy.strip_subaddress {
        if let Some(index) = address.local.find('+') {
            if index == 0 {
                return Err(EmailError::EmptyNormalizedLocal {
                    at: origins.get(index).copied().unwrap_or(input.len()),
                });
            }
            address.local.truncate(index);
        }
    }

    let bytes = address.local.as_bytes();
    let safe = local_atom_valid(bytes);
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
    let mut first_by_key: HashMap<(String, String), usize> = HashMap::new();

    for (i, input) in inputs.iter().enumerate() {
        match normalize_email(input, policy) {
            Err(error) => errors.push((i, error)),
            Ok(address) => {
                let key = (address.local.clone(), address.domain.clone());
                if let Some(&first) = first_by_key.get(&key) {
                    duplicate_of[i] = Some(first);
                } else {
                    first_by_key.insert(key, i);
                    addresses.push(address);
                    source_indices.push(i);
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