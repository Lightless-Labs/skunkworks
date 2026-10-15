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
struct CfwsRun {
    start: usize,
    end: usize,
}

fn err_at(kind: fn(usize) -> EmailError, at: usize) -> EmailError {
    kind(at)
}

fn horizontal(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

fn fold_end(bytes: &[u8], i: usize) -> Option<usize> {
    if bytes.get(i) != Some(&b'\r') || bytes.get(i + 1) != Some(&b'\n') {
        return None;
    }
    let mut j = i + 2;
    if !bytes.get(j).is_some_and(|b| horizontal(*b)) {
        return None;
    }
    while bytes.get(j).is_some_and(|b| horizontal(*b)) {
        j += 1;
    }
    Some(j)
}

fn printable(b: u8) -> bool {
    (0x20..=0x7e).contains(&b)
}

fn cleanup(input: &str) -> Result<(Vec<u8>, Vec<usize>, Vec<CfwsRun>), EmailError> {
    let bytes = input.as_bytes();
    let mut kept = Vec::with_capacity(bytes.len());
    let mut positions = Vec::with_capacity(bytes.len());
    let mut runs = Vec::new();
    let mut i = 0;
    let mut quote_protected = false;

    while i < bytes.len() {
        if quote_protected {
            let b = bytes[i];
            kept.push(b);
            positions.push(i);
            i += 1;
            if b == b'\\' {
                if i < bytes.len() {
                    kept.push(bytes[i]);
                    positions.push(i);
                    i += 1;
                }
            } else if b == b'"' {
                quote_protected = false;
            }
            continue;
        }

        let b = bytes[i];
        if b == b'"' && kept.is_empty() {
            kept.push(b);
            positions.push(i);
            i += 1;
            quote_protected = true;
            continue;
        }

        let run_start = i;
        let mut in_run = false;

        loop {
            if i >= bytes.len() {
                break;
            }
            let c = bytes[i];
            if horizontal(c) {
                in_run = true;
                i += 1;
                continue;
            }
            if c == b'\r' || c == b'\n' {
                in_run = true;
                if let Some(end) = fold_end(bytes, i) {
                    i = end;
                    continue;
                }
                return Err(EmailError::BadFold { at: i });
            }
            if c == b'(' {
                in_run = true;
                let outer = i;
                let mut openings = Vec::with_capacity(3);
                openings.push(i);
                i += 1;
                while i < bytes.len() {
                    let c = bytes[i];
                    if c == b'\\' {
                        let slash = i;
                        if i + 1 >= bytes.len() || !printable(bytes[i + 1]) {
                            return Err(EmailError::CommentEscape { at: slash });
                        }
                        i += 2;
                        continue;
                    }
                    if c == b'\r' || c == b'\n' {
                        if let Some(end) = fold_end(bytes, i) {
                            i = end;
                            continue;
                        }
                        return Err(EmailError::BadFold { at: i });
                    }
                    if c == b'(' {
                        if openings.len() == 3 {
                            return Err(EmailError::CommentDepth { at: i });
                        }
                        openings.push(i);
                        i += 1;
                        continue;
                    }
                    if c == b')' {
                        openings.pop();
                        i += 1;
                        if openings.is_empty() {
                            break;
                        }
                        continue;
                    }
                    if c == b'\t' || printable(c) {
                        i += 1;
                        continue;
                    }
                    return Err(EmailError::CommentChar { at: i });
                }
                if !openings.is_empty() {
                    return Err(EmailError::UnclosedComment { at: outer });
                }
                continue;
            }
            if c == b')' {
                return Err(EmailError::UnexpectedClose { at: i });
            }
            break;
        }

        if in_run {
            runs.push(CfwsRun {
                start: run_start,
                end: i,
            });
        }

        if i < bytes.len() {
            kept.push(bytes[i]);
            positions.push(i);
            i += 1;
        }
    }

    Ok((kept, positions, runs))
}

fn original_pos(positions: &[usize], idx: usize, input_len: usize) -> usize {
    positions.get(idx).copied().unwrap_or(input_len)
}

fn is_atom_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric()
        || b"!#$%&'*+-/=?^_`{|}~".contains(&b)
}

fn valid_dot_atom(s: &[u8]) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut prev_dot = true;
    for (i, &b) in s.iter().enumerate() {
        if b == b'.' {
            if prev_dot || i + 1 == s.len() {
                return false;
            }
            prev_dot = true;
        } else if is_atom_byte(b) {
            prev_dot = false;
        } else {
            return false;
        }
    }
    true
}

fn parse_cleaned(
    input: &str,
    policy: Option<&Policy>,
) -> Result<(Address, Vec<usize>), EmailError> {
    if input.is_empty() {
        return Err(EmailError::EmptyInput);
    }
    let bytes = input.as_bytes();
    if let Some(at) = bytes.iter().position(|b| *b >= 128) {
        return Err(EmailError::NonAscii { at });
    }

    let (clean, positions, runs) = cleanup(input)?;

    for run in &runs {
        let left_is_at = run.start > 0 && bytes[run.start - 1] == b'@';
        let right_is_at = run.end < bytes.len() && bytes[run.end] == b'@';
        let at_boundary = run.start == 0 || run.end == bytes.len();
        if !(left_is_at || right_is_at || at_boundary) {
            return Err(EmailError::CfwsBoundary { at: run.start });
        }
    }

    if clean.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let mut quote_close = None;
    if clean[0] == b'"' {
        let mut i = 1;
        while i < clean.len() {
            if clean[i] == b'\\' {
                i += 2;
            } else if clean[i] == b'"' {
                quote_close = Some(i);
                break;
            } else {
                i += 1;
            }
        }
        if quote_close.is_none() {
            return Err(EmailError::UnclosedQuote {
                at: original_pos(&positions, 0, input.len()),
            });
        }
    }

    let protected_end = quote_close.map_or(0, |i| i + 1);
    let ats: Vec<usize> = clean
        .iter()
        .enumerate()
        .filter_map(|(i, b)| {
            if *b == b'@' && i >= protected_end {
                Some(i)
            } else {
                None
            }
        })
        .collect();

    if ats.is_empty() {
        return Err(EmailError::MissingAt);
    }
    if ats.len() > 1 {
        return Err(EmailError::MultipleAt {
            at: original_pos(&positions, ats[1], input.len()),
        });
    }

    let sep = ats[0];
    if sep == 0 {
        return Err(EmailError::EmptyLocal {
            at: original_pos(&positions, sep, input.len()),
        });
    }

    let local_raw = &clean[..sep];
    let mut local = Vec::new();
    let mut origins = Vec::new();
    let quoted;

    if local_raw[0] == b'"' {
        quoted = true;
        let close = quote_close.unwrap();
        if close + 1 != sep {
            return Err(EmailError::TrailingLocal {
                at: original_pos(&positions, close + 1, input.len()),
            });
        }
        let mut i = 1;
        while i < close {
            let b = local_raw[i];
            if b == b'\\' {
                let second = local_raw.get(i + 1).copied();
                match second {
                    Some(b'"' | b'\\' | b'@') => {
                        local.push(second.unwrap());
                        origins.push(positions[i]);
                        i += 2;
                    }
                    _ => {
                        return Err(EmailError::BadEscape {
                            at: positions[i],
                        })
                    }
                }
            } else if printable(b) {
                local.push(b);
                origins.push(positions[i]);
                i += 1;
            } else {
                return Err(EmailError::QuotedChar {
                    at: positions[i],
                });
            }
        }
        if local.is_empty() {
            return Err(EmailError::EmptyLocal {
                at: positions[close],
            });
        }
        if local.len() > 64 {
            return Err(EmailError::LocalTooLong { at: origins[64] });
        }
    } else {
        quoted = false;
        if local_raw.len() > 64 {
            return Err(EmailError::LocalTooLong {
                at: original_pos(&positions, 64, input.len()),
            });
        }
        for (i, &b) in local_raw.iter().enumerate() {
            if b == b'.' {
                if i == 0
                    || i + 1 == local_raw.len()
                    || local_raw.get(i.wrapping_sub(1)) == Some(&b'.')
                {
                    return Err(EmailError::LocalDot {
                        at: positions[i],
                    });
                }
            } else if !is_atom_byte(b) {
                return Err(EmailError::LocalChar {
                    at: positions[i],
                });
            }
        }
        local.extend_from_slice(local_raw);
        origins.extend_from_slice(&positions[..sep]);
    }

    let domain_start = sep + 1;
    let raw_domain = &clean[domain_start..];
    let mut domain_end = clean.len();
    let root_pos = if raw_domain.last() == Some(&b'.') {
        domain_end -= 1;
        Some(positions[domain_end])
    } else {
        None
    };

    if domain_end == domain_start {
        return Err(EmailError::EmptyDomain {
            at: original_pos(&positions, domain_start, input.len()),
        });
    }

    if domain_end - domain_start > 253 {
        return Err(EmailError::DomainTooLong {
            at: positions[domain_start + 253],
        });
    }

    let domain = &clean[domain_start..domain_end];
    let mut labels: Vec<(usize, usize)> = Vec::new();
    let mut start = 0;
    for i in 0..=domain.len() {
        if i == domain.len() || domain[i] == b'.' {
            labels.push((start, i));
            start = i + 1;
        }
    }

    for (label_index, &(lo, hi)) in labels.iter().enumerate() {
        if lo == hi {
            let at = if lo == domain.len() {
                root_pos.unwrap_or_else(|| {
                    original_pos(&positions, domain_start + lo, input.len())
                })
            } else {
                positions[domain_start + lo]
            };
            return Err(EmailError::EmptyLabel { at });
        }
        let len = hi - lo;
        if len > 63 {
            return Err(EmailError::LabelTooLong {
                at: positions[domain_start + lo + 63],
            });
        }
        if domain[lo] == b'-' {
            return Err(EmailError::LabelHyphen {
                at: positions[domain_start + lo],
            });
        }
        if domain[hi - 1] == b'-' {
            return Err(EmailError::LabelHyphen {
                at: positions[domain_start + hi - 1],
            });
        }
        for i in lo..hi {
            let b = domain[i];
            if !(b.is_ascii_alphanumeric() || b == b'-') {
                return Err(EmailError::DomainChar {
                    at: positions[domain_start + i],
                });
            }
        }
        let _ = label_index;
    }

    let allow_single = policy.is_some_and(|p| p.allow_single_label);
    if labels.len() < 2 && !allow_single {
        return Err(EmailError::DomainNeedsDot {
            at: original_pos(&positions, domain_start, input.len()),
        });
    }

    let (last_lo, last_hi) = labels[labels.len() - 1];
    if domain[last_lo..last_hi].iter().all(u8::is_ascii_digit) {
        return Err(EmailError::NumericTld {
            at: positions[domain_start + last_lo],
        });
    }

    let domain_string = domain
        .iter()
        .map(|b| b.to_ascii_lowercase() as char)
        .collect::<String>();

    Ok((
        Address {
            local: String::from_utf8(local).unwrap(),
            domain: domain_string,
            quoted,
        },
        origins,
    ))
}

pub fn parse_email(input: &str) -> Result<Address, EmailError> {
    parse_cleaned(input, None).map(|(address, _)| address)
}

pub fn normalize_email(input: &str, policy: &Policy) -> Result<Address, EmailError> {
    let (mut address, origins) = parse_cleaned(input, Some(policy))?;
    let mut local = address.local.into_bytes();

    if policy.local_case == LocalCase::Lower {
        for b in &mut local {
            *b = b.to_ascii_lowercase();
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

    if local.is_empty() {
        return Err(EmailError::EmptyNormalizedLocal {
            at: origins.first().copied().unwrap_or(input.len()),
        });
    }

    let safe_atom = valid_dot_atom(&local);
    address.local = String::from_utf8(local).unwrap();
    if !safe_atom {
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
    let mut keys: Vec<(String, String, usize)> = Vec::new();

    for (index, input) in inputs.iter().enumerate() {
        match normalize_email(input, policy) {
            Err(error) => errors.push((index, error)),
            Ok(address) => {
                if let Some((_, _, first_index)) = keys
                    .iter()
                    .find(|(local, domain, _)| local == &address.local && domain == &address.domain)
                {
                    duplicate_of[index] = Some(*first_index);
                } else {
                    keys.push((address.local.clone(), address.domain.clone(), index));
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