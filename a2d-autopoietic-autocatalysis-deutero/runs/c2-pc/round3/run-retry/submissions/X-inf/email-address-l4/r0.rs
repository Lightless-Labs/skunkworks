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

#[derive(Debug)]
struct Cleaned {
    bytes: Vec<u8>,
    positions: Vec<usize>,
    runs: Vec<(usize, usize, usize)>,
}

fn is_hws(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

fn fold_end(input: &[u8], i: usize) -> Option<usize> {
    if input.get(i) != Some(&b'\r') || input.get(i + 1) != Some(&b'\n') {
        return None;
    }
    let mut j = i + 2;
    if !input.get(j).copied().is_some_and(is_hws) {
        return None;
    }
    while input.get(j).copied().is_some_and(is_hws) {
        j += 1;
    }
    Some(j)
}

fn lexical_cleanup(input: &[u8]) -> Result<Cleaned, EmailError> {
    let mut bytes = Vec::with_capacity(input.len());
    let mut positions = Vec::with_capacity(input.len());
    let mut runs = Vec::new();
    let mut i = 0;
    let mut depth = 0usize;
    let mut outer_open = 0usize;
    let mut first_retained = true;
    let mut quoted_prefix = false;

    while i < input.len() {
        let b = input[i];

        if depth > 0 {
            if b == b'\\' {
                if i + 1 >= input.len() || !(0x20..=0x7e).contains(&input[i + 1]) {
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
            if !(0x20..=0x7e).contains(&b) && !is_hws(b) {
                return Err(EmailError::CommentChar { at: i });
            }
            i += 1;
            continue;
        }

        if quoted_prefix {
            bytes.push(b);
            positions.push(i);
            if b == b'\\' {
                if i + 1 < input.len() {
                    i += 1;
                    bytes.push(input[i]);
                    positions.push(i);
                }
            } else if b == b'"' {
                quoted_prefix = false;
            }
            first_retained = false;
            i += 1;
            continue;
        }

        if b == b' ' || b == b'\t' || b == b'\r' || b == b'\n' || b == b'(' {
            let start = i;
            let mut end = i;
            loop {
                if end >= input.len() {
                    break;
                }
                let c = input[end];
                if is_hws(c) {
                    end += 1;
                } else if c == b'\r' || c == b'\n' {
                    if let Some(next) = fold_end(input, end) {
                        end = next;
                    } else {
                        return Err(EmailError::BadFold { at: end });
                    }
                } else if c == b'(' {
                    if depth == 0 {
                        outer_open = end;
                    }
                    depth = 1;
                    end += 1;
                    while end < input.len() && depth > 0 {
                        let x = input[end];
                        if x == b'\\' {
                            if end + 1 >= input.len()
                                || !(0x20..=0x7e).contains(&input[end + 1])
                            {
                                return Err(EmailError::CommentEscape { at: end });
                            }
                            end += 2;
                        } else if x == b'\r' || x == b'\n' {
                            if let Some(next) = fold_end(input, end) {
                                end = next;
                            } else {
                                return Err(EmailError::BadFold { at: end });
                            }
                        } else if x == b'(' {
                            if depth == 3 {
                                return Err(EmailError::CommentDepth { at: end });
                            }
                            depth += 1;
                            end += 1;
                        } else if x == b')' {
                            depth -= 1;
                            end += 1;
                        } else if !(0x20..=0x7e).contains(&x) && !is_hws(x) {
                            return Err(EmailError::CommentChar { at: end });
                        } else {
                            end += 1;
                        }
                    }
                    if depth > 0 {
                        return Err(EmailError::UnclosedComment { at: outer_open });
                    }
                } else {
                    break;
                }
            }
            runs.push((start, end, bytes.len()));
            i = end;
            continue;
        }

        if b == b')' {
            return Err(EmailError::UnexpectedClose { at: i });
        }
        if first_retained && b == b'"' {
            quoted_prefix = true;
        }
        bytes.push(b);
        positions.push(i);
        first_retained = false;
        i += 1;
    }

    if depth > 0 {
        return Err(EmailError::UnclosedComment { at: outer_open });
    }

    Ok(Cleaned {
        bytes,
        positions,
        runs,
    })
}

fn is_atom_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$%&'*+-/=?^_`{|}~".contains(&b)
}

fn valid_dot_atom(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }
    let mut prev_dot = true;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'.' {
            if prev_dot || i + 1 == bytes.len() {
                return false;
            }
            prev_dot = true;
        } else if is_atom_char(b) {
            prev_dot = false;
        } else {
            return false;
        }
    }
    true
}

fn parse_core(input: &str, allow_single_label: bool) -> Result<(Address, Vec<usize>), EmailError> {
    let raw = input.as_bytes();
    if raw.is_empty() {
        return Err(EmailError::EmptyInput);
    }
    if let Some(i) = raw.iter().position(|&b| b >= 128) {
        return Err(EmailError::NonAscii { at: i });
    }

    let clean = lexical_cleanup(raw)?;
    for &(start, end, index) in &clean.runs {
        let before = index > 0 && clean.bytes[index - 1] == b'@';
        let after = index < clean.bytes.len() && clean.bytes[index] == b'@';
        if index == 0 || index == clean.bytes.len() || before || after {
            continue;
        }
        return Err(EmailError::CfwsBoundary { at: start.min(end) });
    }
    if clean.bytes.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let bytes = &clean.bytes;
    let mut quote_end = None;
    if bytes.first() == Some(&b'"') {
        let mut i = 1;
        while i < bytes.len() {
            if bytes[i] == b'\\' {
                i += 2;
            } else if bytes[i] == b'"' {
                quote_end = Some(i);
                break;
            } else {
                i += 1;
            }
        }
        if quote_end.is_none() {
            return Err(EmailError::UnclosedQuote {
                at: clean.positions[0],
            });
        }
    }

    let protected_end = quote_end.unwrap_or(0);
    let ats: Vec<usize> = bytes
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| {
            if b == b'@' && (quote_end.is_none() || i > protected_end) {
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
            at: clean.positions[ats[1]],
        });
    }
    let at = ats[0];
    if at == 0 {
        return Err(EmailError::EmptyLocal {
            at: clean.positions[at],
        });
    }

    let mut local = Vec::new();
    let mut origins = Vec::new();
    let quoted;
    if bytes[0] == b'"' {
        let close = quote_end.unwrap();
        if close + 1 != at {
            return Err(EmailError::TrailingLocal {
                at: clean.positions[close + 1],
            });
        }
        quoted = true;
        let mut i = 1;
        while i < close {
            let b = bytes[i];
            if b == b'\\' {
                if i + 1 >= close || !matches!(bytes[i + 1], b'"' | b'\\' | b'@') {
                    return Err(EmailError::BadEscape {
                        at: clean.positions[i],
                    });
                }
                local.push(bytes[i + 1]);
                origins.push(clean.positions[i]);
                i += 2;
            } else {
                if !(0x20..=0x7e).contains(&b) {
                    return Err(EmailError::QuotedChar {
                        at: clean.positions[i],
                    });
                }
                local.push(b);
                origins.push(clean.positions[i]);
                i += 1;
            }
        }
        if local.is_empty() {
            return Err(EmailError::EmptyLocal {
                at: clean.positions[close],
            });
        }
        if local.len() > 64 {
            return Err(EmailError::LocalTooLong { at: origins[64] });
        }
    } else {
        quoted = false;
        let lb = &bytes[..at];
        if lb.len() > 64 {
            return Err(EmailError::LocalTooLong {
                at: clean.positions[64],
            });
        }
        let mut prev_dot = false;
        for (i, &b) in lb.iter().enumerate() {
            if b == b'.' {
                if i == 0 || i + 1 == lb.len() || prev_dot {
                    return Err(EmailError::LocalDot {
                        at: clean.positions[i],
                    });
                }
                prev_dot = true;
            } else {
                if !is_atom_char(b) {
                    return Err(EmailError::LocalChar {
                        at: clean.positions[i],
                    });
                }
                prev_dot = false;
            }
            local.push(b);
            origins.push(clean.positions[i]);
        }
    }

    let domain_start = at + 1;
    let mut domain_end = bytes.len();
    let mut root_pos = None;
    if domain_end > domain_start && bytes[domain_end - 1] == b'.' {
        domain_end -= 1;
        root_pos = Some(clean.positions[domain_end]);
    }
    if domain_end == domain_start {
        return Err(EmailError::EmptyDomain {
            at: if domain_start < clean.positions.len() {
                clean.positions[domain_start]
            } else {
                input.len()
            },
        });
    }
    let domain_len = domain_end - domain_start;
    if domain_len > 253 {
        return Err(EmailError::DomainTooLong {
            at: clean.positions[domain_start + 253],
        });
    }

    let mut labels: Vec<(usize, usize)> = Vec::new();
    let mut start = domain_start;
    for i in domain_start..domain_end {
        if bytes[i] == b'.' {
            labels.push((start, i));
            start = i + 1;
        }
    }
    labels.push((start, domain_end));

    for (li, &(s, e)) in labels.iter().enumerate() {
        if s == e {
            let pos = if li + 1 == labels.len() && root_pos.is_some() {
                root_pos.unwrap()
            } else if s < clean.positions.len() {
                clean.positions[s]
            } else {
                input.len()
            };
            return Err(EmailError::EmptyLabel { at: pos });
        }
        if e - s > 63 {
            return Err(EmailError::LabelTooLong {
                at: clean.positions[s + 63],
            });
        }
        if bytes[s] == b'-' {
            return Err(EmailError::LabelHyphen {
                at: clean.positions[s],
            });
        }
        if bytes[e - 1] == b'-' {
            return Err(EmailError::LabelHyphen {
                at: clean.positions[e - 1],
            });
        }
        for i in s..e {
            let b = bytes[i];
            if !b.is_ascii_alphanumeric() && b != b'-' {
                return Err(EmailError::DomainChar {
                    at: clean.positions[i],
                });
            }
        }
    }
    if labels.len() < 2 && !allow_single_label {
        return Err(EmailError::DomainNeedsDot {
            at: clean.positions[domain_start],
        });
    }
    let (last_start, last_end) = *labels.last().unwrap();
    if bytes[last_start..last_end].iter().all(u8::is_ascii_digit) {
        return Err(EmailError::NumericTld {
            at: clean.positions[last_start],
        });
    }

    let local_string = String::from_utf8(local).expect("ASCII local");
    let domain_string = String::from_utf8(
        bytes[domain_start..domain_end]
            .iter()
            .map(u8::to_ascii_lowercase)
            .collect(),
    )
    .expect("ASCII domain");
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
    parse_core(input, false).map(|(address, _)| address)
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
    let local = address.local.as_bytes();
    let safe = valid_dot_atom(local);
    if !safe {
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