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

#[derive(Clone)]
struct CfwsRun {
    original: usize,
    before: Option<u8>,
    after: Option<u8>,
}

fn is_hws(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

fn fold_end(bytes: &[u8], i: usize) -> Option<usize> {
    if i + 2 >= bytes.len() || bytes[i] != b'\r' || bytes[i + 1] != b'\n' || !is_hws(bytes[i + 2]) {
        return None;
    }
    let mut j = i + 3;
    while j < bytes.len() && is_hws(bytes[j]) {
        j += 1;
    }
    Some(j)
}

fn cleanup(input: &str) -> Result<(Vec<Byte>, Vec<CfwsRun>), EmailError> {
    let raw = input.as_bytes();
    let mut kept: Vec<Byte> = Vec::with_capacity(raw.len());
    let mut runs = Vec::new();
    let mut i = 0;
    let mut depth = 0usize;
    let mut outer_open = 0usize;
    let mut in_prefix_quote = false;
    let mut quote_escaped = false;
    let mut run_start: Option<usize> = None;

    while i < raw.len() {
        let b = raw[i];

        if in_prefix_quote {
            kept.push(Byte { value: b, original: i });
            if quote_escaped {
                quote_escaped = false;
            } else if b == b'\\' {
                quote_escaped = true;
            } else if b == b'"' {
                in_prefix_quote = false;
            }
            i += 1;
            continue;
        }

        if depth > 0 {
            if b == b'\\' {
                if i + 1 >= raw.len() || !(0x20..=0x7e).contains(&raw[i + 1]) {
                    return Err(EmailError::CommentEscape { at: i });
                }
                i += 2;
                continue;
            }
            if b == b'\r' || b == b'\n' {
                if let Some(end) = fold_end(raw, i) {
                    i = end;
                    continue;
                }
                return Err(EmailError::BadFold { at: i });
            }
            if b == b'(' {
                if depth == 3 {
                    return Err(EmailError::CommentDepth { at: i });
                }
                depth += 1;
                i += 1;
                continue;
            }
            if b == b')' {
                depth -= 1;
                i += 1;
                continue;
            }
            if is_hws(b) {
                i += 1;
                continue;
            }
            if !(0x20..=0x7e).contains(&b) {
                return Err(EmailError::CommentChar { at: i });
            }
            i += 1;
            continue;
        }

        if b == b'(' || is_hws(b) || b == b'\r' || b == b'\n' {
            if run_start.is_none() {
                run_start = Some(i);
            }
            if b == b'(' {
                depth = 1;
                outer_open = i;
                i += 1;
            } else if b == b'\r' || b == b'\n' {
                if let Some(end) = fold_end(raw, i) {
                    i = end;
                } else {
                    return Err(EmailError::BadFold { at: i });
                }
            } else {
                i += 1;
            }
            continue;
        }

        if b == b')' {
            return Err(EmailError::UnexpectedClose { at: i });
        }

        if run_start.is_some() {
            runs.push(CfwsRun {
                original: run_start.take().unwrap(),
                before: kept.last().map(|x| x.value),
                after: Some(b),
            });
        }

        if kept.is_empty() && b == b'"' {
            in_prefix_quote = true;
        }
        kept.push(Byte { value: b, original: i });
        i += 1;
    }

    if depth > 0 {
        return Err(EmailError::UnclosedComment { at: outer_open });
    }
    if let Some(start) = run_start {
        runs.push(CfwsRun {
            original: start,
            before: kept.last().map(|x| x.value),
            after: None,
        });
    }

    for run in &runs {
        let boundary_ok = run.before.is_none()
            || run.after.is_none()
            || run.before == Some(b'@')
            || run.after == Some(b'@');
        if !boundary_ok {
            return Err(EmailError::CfwsBoundary { at: run.original });
        }
    }

    Ok((kept, runs))
}

fn at_pos(bytes: &[Byte], idx: usize, input_len: usize) -> usize {
    bytes.get(idx).map(|b| b.original).unwrap_or(input_len)
}

fn local_atom_valid(local: &[u8]) -> bool {
    if local.is_empty() || local.len() > 64 || local[0] == b'.' || local[local.len() - 1] == b'.' {
        return false;
    }
    let mut prev_dot = false;
    for &b in local {
        if b == b'.' {
            if prev_dot {
                return false;
            }
            prev_dot = true;
        } else {
            prev_dot = false;
            if !b.is_ascii_alphanumeric() && !b"!#$%&'*+-/=?^_`{|}~".contains(&b) {
                return false;
            }
        }
    }
    true
}

fn parse_core(input: &str, allow_single_label: bool) -> Result<(Address, Vec<usize>), EmailError> {
    let raw = input.as_bytes();
    if raw.is_empty() {
        return Err(EmailError::EmptyInput);
    }
    if let Some(at) = raw.iter().position(|&b| b >= 128) {
        return Err(EmailError::NonAscii { at });
    }

    let (bytes, _) = cleanup(input)?;
    if bytes.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let mut quote_close = None;
    if bytes[0].value == b'"' {
        let mut i = 1;
        while i < bytes.len() {
            if bytes[i].value == b'\\' {
                i += 2;
            } else if bytes[i].value == b'"' {
                quote_close = Some(i);
                break;
            } else {
                i += 1;
            }
        }
        if quote_close.is_none() {
            return Err(EmailError::UnclosedQuote {
                at: bytes[0].original,
            });
        }
    }

    let protected_end = quote_close.unwrap_or(0);
    let mut separators = Vec::new();
    for i in 0..bytes.len() {
        if i <= protected_end && quote_close.is_some() {
            continue;
        }
        if bytes[i].value == b'@' {
            separators.push(i);
        }
    }
    if separators.is_empty() {
        return Err(EmailError::MissingAt);
    }
    if separators.len() >= 2 {
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
    let mut origins = Vec::new();
    let mut quoted = quote_close.is_some();
    if let Some(close) = quote_close {
        if close + 1 != sep {
            return Err(EmailError::TrailingLocal {
                at: bytes[close + 1].original,
            });
        }
        let mut i = 1;
        while i < close {
            let b = bytes[i].value;
            if b == b'\\' {
                if i + 1 >= close || !matches!(bytes[i + 1].value, b'"' | b'\\' | b'@') {
                    return Err(EmailError::BadEscape { at: bytes[i].original });
                }
                local.push(bytes[i + 1].value);
                origins.push(bytes[i].original);
                i += 2;
            } else {
                if !(0x20..=0x7e).contains(&b) {
                    return Err(EmailError::QuotedChar { at: bytes[i].original });
                }
                local.push(b);
                origins.push(bytes[i].original);
                i += 1;
            }
        }
        if local.is_empty() {
            return Err(EmailError::EmptyLocal {
                at: bytes[close].original,
            });
        }
        if local.len() > 64 {
            return Err(EmailError::LocalTooLong { at: origins[64] });
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
                    return Err(EmailError::LocalDot { at: bytes[i].original });
                }
                previous_dot = true;
            } else {
                previous_dot = false;
                if !b.is_ascii_alphanumeric() && !b"!#$%&'*+-/=?^_`{|}~".contains(&b) {
                    return Err(EmailError::LocalChar { at: bytes[i].original });
                }
            }
            local.push(b);
            origins.push(bytes[i].original);
        }
    }

    let domain_start = sep + 1;
    let mut domain_end = bytes.len();
    let mut root_dot = None;
    if domain_end > domain_start && bytes[domain_end - 1].value == b'.' {
        domain_end -= 1;
        root_dot = Some(bytes[domain_end].original);
    }
    if domain_start == domain_end {
        return Err(EmailError::EmptyDomain {
            at: at_pos(&bytes, domain_start, input.len()),
        });
    }
    if domain_end - domain_start > 253 {
        return Err(EmailError::DomainTooLong {
            at: bytes[domain_start + 253].original,
        });
    }

    let mut labels: Vec<(usize, usize)> = Vec::new();
    let mut start = domain_start;
    let mut i = domain_start;
    while i <= domain_end {
        if i == domain_end || bytes[i].value == b'.' {
            if start == i {
                let pos = if i == domain_end {
                    root_dot.unwrap_or_else(|| at_pos(&bytes, i, input.len()))
                } else {
                    bytes[i].original
                };
                return Err(EmailError::EmptyLabel { at: pos });
            }
            let len = i - start;
            if len > 63 {
                return Err(EmailError::LabelTooLong {
                    at: bytes[start + 63].original,
                });
            }
            if bytes[start].value == b'-' {
                return Err(EmailError::LabelHyphen {
                    at: bytes[start].original,
                });
            }
            if bytes[i - 1].value == b'-' {
                return Err(EmailError::LabelHyphen {
                    at: bytes[i - 1].original,
                });
            }
            for j in start..i {
                let b = bytes[j].value;
                if !b.is_ascii_alphanumeric() && b != b'-' {
                    return Err(EmailError::DomainChar {
                        at: bytes[j].original,
                    });
                }
            }
            labels.push((start, i));
            start = i + 1;
        }
        i += 1;
    }

    if labels.len() < 2 && !allow_single_label {
        return Err(EmailError::DomainNeedsDot {
            at: bytes[domain_start].original,
        });
    }
    let (last_start, last_end) = *labels.last().unwrap();
    if bytes[last_start..last_end].iter().all(|b| b.value.is_ascii_digit()) {
        return Err(EmailError::NumericTld {
            at: bytes[last_start].original,
        });
    }

    let local_string = String::from_utf8(local).unwrap();
    let domain_string: String = bytes[domain_start..domain_end]
        .iter()
        .map(|b| (b.value as char).to_ascii_lowercase())
        .collect();
    Ok((
        Address {
            local: local_string,
            domain: domain_string,
            quoted,
        },
        origins,
    ))
}

pub fn parse_email(input: &str) -> Result<Address, EmailError> {
    parse_core(input, false).map(|x| x.0)
}

pub fn normalize_email(input: &str, policy: &Policy) -> Result<Address, EmailError> {
    let (mut address, origins) = parse_core(input, policy.allow_single_label)?;
    if policy.local_case == LocalCase::Lower {
        address.local.make_ascii_lowercase();
    }
    if policy.strip_subaddress {
        if let Some(plus) = address.local.as_bytes().iter().position(|&b| b == b'+') {
            if plus == 0 {
                return Err(EmailError::EmptyNormalizedLocal {
                    at: origins.first().copied().unwrap_or(input.len()),
                });
            }
            address.local.truncate(plus);
        }
    }
    if !local_atom_valid(address.local.as_bytes()) {
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