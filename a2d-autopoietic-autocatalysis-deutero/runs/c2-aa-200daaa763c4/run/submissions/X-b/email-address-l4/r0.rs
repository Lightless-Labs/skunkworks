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

struct Cleaned {
    bytes: Vec<u8>,
    positions: Vec<usize>,
    runs: Vec<CfwsRun>,
}

fn is_hws(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

fn is_printable(b: u8) -> bool {
    (0x20..=0x7e).contains(&b)
}

fn scan_fold(input: &[u8], i: usize) -> Option<usize> {
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

fn cleanup(input: &[u8]) -> Result<Cleaned, EmailError> {
    let mut out = Vec::with_capacity(input.len());
    let mut positions = Vec::with_capacity(input.len());
    let mut runs = Vec::new();
    let mut i = 0;
    let mut protected = false;
    let mut quote_closed = false;

    while i < input.len() {
        if protected {
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
            } else {
                if b == b'"' {
                    quote_closed = true;
                    protected = false;
                }
                i += 1;
            }
            continue;
        }

        if !quote_closed && out.is_empty() && input[i] == b'"' {
            protected = true;
            out.push(input[i]);
            positions.push(i);
            i += 1;
            continue;
        }

        if is_hws(input[i]) || input[i] == b'\r' || input[i] == b'\n' || input[i] == b'(' {
            let start = i;
            let mut comment_depth = 0usize;
            let mut outer_open = None;

            loop {
                if i >= input.len() {
                    break;
                }
                let b = input[i];

                if comment_depth > 0 {
                    if b == b'\\' {
                        let esc_at = i;
                        if i + 1 >= input.len() || !is_printable(input[i + 1]) {
                            return Err(EmailError::CommentEscape { at: esc_at });
                        }
                        i += 2;
                        continue;
                    }
                    if b == b'\r' || b == b'\n' {
                        if let Some(next) = scan_fold(input, i) {
                            i = next;
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
                            continue;
                        }
                        continue;
                    }
                    if !is_printable(b) && b != b'\t' {
                        return Err(EmailError::CommentChar { at: i });
                    }
                    i += 1;
                    continue;
                }

                if is_hws(b) {
                    i += 1;
                    continue;
                }
                if b == b'\r' || b == b'\n' {
                    if let Some(next) = scan_fold(input, i) {
                        i = next;
                        continue;
                    }
                    return Err(EmailError::BadFold { at: i });
                }
                if b == b'(' {
                    outer_open = Some(i);
                    comment_depth = 1;
                    i += 1;
                    continue;
                }
                break;
            }

            if comment_depth != 0 {
                return Err(EmailError::UnclosedComment {
                    at: outer_open.unwrap_or(start),
                });
            }
            runs.push(CfwsRun { start, end: i });
            continue;
        }

        if input[i] == b')' {
            return Err(EmailError::UnexpectedClose { at: i });
        }

        out.push(input[i]);
        positions.push(i);
        i += 1;
    }

    Ok(Cleaned {
        bytes: out,
        positions,
        runs,
    })
}

fn mapped(pos: &[usize], idx: usize, len: usize) -> usize {
    pos.get(idx).copied().unwrap_or(len)
}

fn valid_dot_atom(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }
    let mut previous_dot = true;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'.' {
            if previous_dot || i + 1 == bytes.len() {
                return false;
            }
            previous_dot = true;
        } else {
            if !is_local_atom_char(b) {
                return false;
            }
            previous_dot = false;
        }
    }
    true
}

fn is_local_atom_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$%&'*+-/=?^_`{|}~".contains(&b)
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
    let positions = &cleaned.positions;
    let input_len = raw.len();

    for run in &cleaned.runs {
        let left = cleaned
            .positions
            .iter()
            .position(|&p| p >= run.end)
            .map(|idx| idx.checked_sub(1))
            .flatten();
        let right = cleaned.positions.iter().position(|&p| p >= run.end);
        let at_left = left.is_some_and(|idx| bytes[idx] == b'@');
        let at_right = right.is_some_and(|idx| bytes[idx] == b'@');
        let at_boundary = right.is_none() || left.is_none();
        if !at_left && !at_right && !at_boundary {
            return Err(EmailError::CfwsBoundary { at: run.start });
        }
    }

    if bytes.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let mut prefix_end = 0usize;
    if bytes[0] == b'"' {
        let mut i = 1usize;
        let mut closing = None;
        while i < bytes.len() {
            if bytes[i] == b'\\' {
                i += 2;
            } else if bytes[i] == b'"' {
                closing = Some(i);
                break;
            } else {
                i += 1;
            }
        }
        match closing {
            Some(end) => prefix_end = end + 1,
            None => return Err(EmailError::UnclosedQuote { at: mapped(positions, 0, input_len) }),
        }
    }

    let separators: Vec<usize> = (prefix_end..bytes.len())
        .filter(|&i| bytes[i] == b'@')
        .collect();
    if separators.is_empty() {
        return Err(EmailError::MissingAt);
    }
    if separators.len() > 1 {
        return Err(EmailError::MultipleAt {
            at: mapped(positions, separators[1], input_len),
        });
    }
    let sep = separators[0];
    if sep == 0 {
        return Err(EmailError::EmptyLocal {
            at: mapped(positions, sep, input_len),
        });
    }

    let local_bytes = &bytes[..sep];
    let mut decoded = Vec::new();
    let mut origins = Vec::new();
    let quoted = local_bytes.first() == Some(&b'"');

    if !quoted {
        if local_bytes.len() > 64 {
            return Err(EmailError::LocalTooLong {
                at: mapped(positions, 64, input_len),
            });
        }
        let mut prev_dot = false;
        for (i, &b) in local_bytes.iter().enumerate() {
            let at = mapped(positions, i, input_len);
            if b == b'.' {
                if i == 0 || i + 1 == local_bytes.len() || prev_dot {
                    return Err(EmailError::LocalDot { at });
                }
                prev_dot = true;
            } else {
                if !is_local_atom_char(b) {
                    return Err(EmailError::LocalChar { at });
                }
                prev_dot = false;
            }
            decoded.push(b);
            origins.push(at);
        }
    } else {
        let close = prefix_end - 1;
        if close + 1 != sep {
            return Err(EmailError::TrailingLocal {
                at: mapped(positions, close + 1, input_len),
            });
        }
        let mut i = 1usize;
        while i < close {
            let b = local_bytes[i];
            if b == b'\\' {
                if i + 1 >= close
                    || !matches!(local_bytes[i + 1], b'"' | b'\\' | b'@')
                {
                    return Err(EmailError::BadEscape {
                        at: mapped(positions, i, input_len),
                    });
                }
                decoded.push(local_bytes[i + 1]);
                origins.push(mapped(positions, i, input_len));
                i += 2;
            } else {
                if !is_printable(b) {
                    return Err(EmailError::QuotedChar {
                        at: mapped(positions, i, input_len),
                    });
                }
                decoded.push(b);
                origins.push(mapped(positions, i, input_len));
                i += 1;
            }
        }
        if decoded.is_empty() {
            return Err(EmailError::EmptyLocal {
                at: mapped(positions, close, input_len),
            });
        }
        if decoded.len() > 64 {
            return Err(EmailError::LocalTooLong {
                at: origins[64],
            });
        }
    }

    let domain_start = sep + 1;
    let mut domain_end = bytes.len();
    let mut root_dot = None;
    if domain_end > domain_start && bytes[domain_end - 1] == b'.' {
        domain_end -= 1;
        root_dot = Some(domain_end);
    }

    if domain_start == domain_end {
        return Err(EmailError::EmptyDomain {
            at: root_dot
                .map(|idx| mapped(positions, idx, input_len))
                .unwrap_or_else(|| mapped(positions, domain_start, input_len)),
        });
    }
    let domain = &bytes[domain_start..domain_end];
    if domain.len() > 253 {
        return Err(EmailError::DomainTooLong {
            at: mapped(positions, domain_start + 253, input_len),
        });
    }

    let mut labels: Vec<(usize, usize)> = Vec::new();
    let mut label_start = domain_start;
    for i in domain_start..=domain_end {
        if i == domain_end || bytes[i] == b'.' {
            if i == label_start {
                let err_at = if i == domain_end {
                    root_dot.map(|idx| mapped(positions, idx, input_len))
                        .unwrap_or_else(|| mapped(positions, i, input_len))
                } else {
                    mapped(positions, i, input_len)
                };
                return Err(EmailError::EmptyLabel { at: err_at });
            }
            let len = i - label_start;
            if len > 63 {
                return Err(EmailError::LabelTooLong {
                    at: mapped(positions, label_start + 63, input_len),
                });
            }
            if bytes[label_start] == b'-' {
                return Err(EmailError::LabelHyphen {
                    at: mapped(positions, label_start, input_len),
                });
            }
            if bytes[i - 1] == b'-' {
                return Err(EmailError::LabelHyphen {
                    at: mapped(positions, i - 1, input_len),
                });
            }
            for j in label_start..i {
                if !bytes[j].is_ascii_alphanumeric() && bytes[j] != b'-' {
                    return Err(EmailError::DomainChar {
                        at: mapped(positions, j, input_len),
                    });
                }
            }
            labels.push((label_start, i));
            label_start = i + 1;
        }
    }

    if !allow_single_label && labels.len() < 2 {
        return Err(EmailError::DomainNeedsDot {
            at: mapped(positions, domain_start, input_len),
        });
    }

    let (last_start, last_end) = *labels.last().unwrap();
    if bytes[last_start..last_end].iter().all(u8::is_ascii_digit) {
        return Err(EmailError::NumericTld {
            at: mapped(positions, last_start, input_len),
        });
    }

    let local = String::from_utf8(decoded).unwrap();
    let domain_string = String::from_utf8(domain.to_vec())
        .unwrap()
        .to_ascii_lowercase();
    Ok((
        Address {
            local,
            domain: domain_string,
            quoted,
        },
        origins,
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

    let mut local = address.local.into_bytes();
    let mut local_origins = origins;
    if policy.strip_subaddress {
        if let Some(plus) = local.iter().position(|&b| b == b'+') {
            if plus == 0 {
                return Err(EmailError::EmptyNormalizedLocal {
                    at: local_origins.first().copied().unwrap_or(input.len()),
                });
            }
            local.truncate(plus);
            local_origins.truncate(plus);
        }
    }

    if local.is_empty() {
        return Err(EmailError::EmptyNormalizedLocal { at: input.len() });
    }
    let safe = valid_dot_atom(&local);
    if !safe {
        address.quoted = true;
    } else if policy.unquote_safe {
        address.quoted = false;
    }
    address.local = String::from_utf8(local).unwrap();
    Ok(address)
}

pub fn format_email(address: &Address) -> String {
    let mut output = String::new();
    if address.quoted {
        output.push('"');
        for ch in address.local.chars() {
            if ch == '"' || ch == '\\' {
                output.push('\\');
            }
            output.push(ch);
        }
        output.push('"');
    } else {
        output.push_str(&address.local);
    }
    output.push('@');
    output.push_str(&address.domain);
    output
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
                    result.source_indices.push(index);
                    result.addresses.push(address);
                }
            }
        }
    }
    result
}