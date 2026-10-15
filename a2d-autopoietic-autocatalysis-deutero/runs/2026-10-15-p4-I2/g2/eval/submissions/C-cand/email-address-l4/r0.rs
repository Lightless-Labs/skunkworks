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

fn cleanup(input: &[u8]) -> Result<Cleaned, EmailError> {
    let mut bytes = Vec::with_capacity(input.len());
    let mut positions = Vec::with_capacity(input.len());
    let mut runs = Vec::new();
    let mut i = 0;

    // Quote protection begins only when the opening quote is the first
    // retained byte. Later quotes have no special meaning to cleanup.
    let mut protected_quote = false;
    let mut comment_depth = 0usize;
    let mut outer_comment_open = 0usize;

    while i < input.len() {
        let b = input[i];

        if protected_quote {
            bytes.push(b);
            positions.push(i);
            if b == b'\\' {
                if i + 1 < input.len() {
                    bytes.push(input[i + 1]);
                    positions.push(i + 1);
                    i += 2;
                } else {
                    i += 1;
                }
                continue;
            }
            if b == b'"' {
                protected_quote = false;
            }
            i += 1;
            continue;
        }

        if comment_depth > 0 {
            match b {
                b'\\' => {
                    let Some(&next) = input.get(i + 1) else {
                        return Err(EmailError::CommentEscape { at: i });
                    };
                    if !(0x20..=0x7e).contains(&next) {
                        return Err(EmailError::CommentEscape { at: i });
                    }
                    i += 2;
                }
                b'(' => {
                    if comment_depth == 3 {
                        return Err(EmailError::CommentDepth { at: i });
                    }
                    comment_depth += 1;
                    i += 1;
                }
                b')' => {
                    comment_depth -= 1;
                    i += 1;
                }
                b'\r' | b'\n' => {
                    if let Some(end) = fold_end(input, i) {
                        i = end;
                    } else {
                        return Err(EmailError::BadFold { at: i });
                    }
                }
                b'\t' => i += 1,
                0x20..=0x7e => i += 1,
                _ => return Err(EmailError::CommentChar { at: i }),
            }
            continue;
        }

        if is_hws(b) || b == b'\r' || b == b'\n' || b == b'(' {
            let run_start = i;

            loop {
                if i >= input.len() {
                    break;
                }

                let c = input[i];
                if is_hws(c) {
                    i += 1;
                } else if c == b'\r' || c == b'\n' {
                    if let Some(end) = fold_end(input, i) {
                        i = end;
                    } else {
                        return Err(EmailError::BadFold { at: i });
                    }
                } else if c == b'(' {
                    comment_depth = 1;
                    outer_comment_open = i;
                    i += 1;

                    while comment_depth > 0 {
                        if i >= input.len() {
                            return Err(EmailError::UnclosedComment {
                                at: outer_comment_open,
                            });
                        }
                        match input[i] {
                            b'\\' => {
                                let Some(&next) = input.get(i + 1) else {
                                    return Err(EmailError::CommentEscape { at: i });
                                };
                                if !(0x20..=0x7e).contains(&next) {
                                    return Err(EmailError::CommentEscape { at: i });
                                }
                                i += 2;
                            }
                            b'(' => {
                                if comment_depth == 3 {
                                    return Err(EmailError::CommentDepth { at: i });
                                }
                                comment_depth += 1;
                                i += 1;
                            }
                            b')' => {
                                comment_depth -= 1;
                                i += 1;
                            }
                            b'\r' | b'\n' => {
                                if let Some(end) = fold_end(input, i) {
                                    i = end;
                                } else {
                                    return Err(EmailError::BadFold { at: i });
                                }
                            }
                            b'\t' => i += 1,
                            0x20..=0x7e => i += 1,
                            _ => return Err(EmailError::CommentChar { at: i }),
                        }
                    }
                } else {
                    break;
                }
            }

            runs.push(CfwsRun {
                start: run_start,
                end: i,
            });
            continue;
        }

        if bytes.is_empty() && b == b'"' {
            protected_quote = true;
            bytes.push(b);
            positions.push(i);
            i += 1;
            continue;
        }

        if b == b')' {
            return Err(EmailError::UnexpectedClose { at: i });
        }

        bytes.push(b);
        positions.push(i);
        i += 1;
    }

    Ok(Cleaned {
        bytes,
        positions,
        runs,
    })
}

fn mapped(positions: &[usize], index: usize, input_len: usize) -> usize {
    positions.get(index).copied().unwrap_or(input_len)
}

fn is_dot_atom_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$%&'*+-/=?^_`{|}~".contains(&b)
}

fn dot_atom_error(bytes: &[u8]) -> Option<(usize, bool)> {
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'.' {
            if i == 0 || i + 1 == bytes.len() || bytes[i - 1] == b'.' {
                return Some((i, true));
            }
        } else if !is_dot_atom_byte(b) {
            return Some((i, false));
        }
    }
    None
}

fn parse_cleaned(
    cleaned: &Cleaned,
    input_len: usize,
    allow_single_label: bool,
) -> Result<(Address, Vec<usize>), EmailError> {
    let bytes = &cleaned.bytes;
    let positions = &cleaned.positions;
    let n = bytes.len();

    // Check boundaries only after the complete cleanup has succeeded.
    for run in &cleaned.runs {
        let legal = run.start == 0
            || run.end == n
            || bytes.get(run.start.wrapping_sub(1)) == Some(&b'@')
            || bytes.get(run.end) == Some(&b'@');
        if !legal {
            return Err(EmailError::CfwsBoundary {
                at: run.start,
            });
        }
    }

    if n == 0 {
        return Err(EmailError::EmptyInput);
    }

    let quote_end = if bytes.first() == Some(&b'"') {
        let mut i = 1;
        let mut found = None;
        while i < n {
            if bytes[i] == b'\\' {
                i = (i + 2).min(n);
            } else if bytes[i] == b'"' {
                found = Some(i);
                break;
            } else {
                i += 1;
            }
        }
        match found {
            Some(end) => Some(end),
            None => {
                return Err(EmailError::UnclosedQuote {
                    at: mapped(positions, 0, input_len),
                });
            }
        }
    } else {
        None
    };

    let mut separators = Vec::new();
    for i in 0..n {
        if quote_end.is_some_and(|end| i <= end) {
            continue;
        }
        if bytes[i] == b'@' {
            separators.push(i);
        }
    }

    if separators.is_empty() {
        return Err(EmailError::MissingAt);
    }
    if separators.len() > 1 {
        return Err(EmailError::MultipleAt {
            at: mapped(positions, separators[1], input_len),
        });
    }

    let at = separators[0];
    if at == 0 {
        return Err(EmailError::EmptyLocal {
            at: mapped(positions, at, input_len),
        });
    }

    let local_raw = &bytes[..at];
    let mut decoded = Vec::new();
    let mut origins = Vec::new();
    let quoted;

    if local_raw.first() == Some(&b'"') {
        let close = quote_end.expect("opening quote has a closing quote");
        if close + 1 != at {
            return Err(EmailError::TrailingLocal {
                at: mapped(positions, close + 1, input_len),
            });
        }
        quoted = true;

        let mut i = 1;
        while i < close {
            let b = bytes[i];
            if b == b'\\' {
                if i + 1 >= close {
                    return Err(EmailError::BadEscape {
                        at: mapped(positions, i, input_len),
                    });
                }
                let next = bytes[i + 1];
                if next != b'"' && next != b'\\' && next != b'@' {
                    return Err(EmailError::BadEscape {
                        at: mapped(positions, i, input_len),
                    });
                }
                decoded.push(next);
                origins.push(mapped(positions, i, input_len));
                i += 2;
            } else {
                if !(0x20..=0x7e).contains(&b) {
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
            return Err(EmailError::LocalTooLong { at: origins[64] });
        }
    } else {
        quoted = false;

        if local_raw.len() > 64 {
            return Err(EmailError::LocalTooLong {
                at: mapped(positions, 64, input_len),
            });
        }
        if let Some((i, is_dot)) = dot_atom_error(local_raw) {
            return Err(if is_dot {
                EmailError::LocalDot {
                    at: mapped(positions, i, input_len),
                }
            } else {
                EmailError::LocalChar {
                    at: mapped(positions, i, input_len),
                }
            });
        }

        decoded.extend_from_slice(local_raw);
        origins.extend((0..local_raw.len()).map(|i| mapped(positions, i, input_len)));
    }

    let domain_start = at + 1;
    let has_root_dot = bytes.last() == Some(&b'.');
    let domain_end = if has_root_dot { n - 1 } else { n };

    if domain_start == domain_end {
        return Err(EmailError::EmptyDomain {
            at: mapped(positions, domain_start, input_len),
        });
    }

    let domain = &bytes[domain_start..domain_end];
    if domain.len() > 253 {
        return Err(EmailError::DomainTooLong {
            at: mapped(positions, domain_start + 253, input_len),
        });
    }

    let mut labels = Vec::new();
    let mut label_start = 0;
    for i in 0..=domain.len() {
        if i == domain.len() || domain[i] == b'.' {
            labels.push((label_start, i));
            label_start = i + 1;
        }
    }

    for &(start, end) in &labels {
        if start == end {
            let error_at = if has_root_dot && end == domain.len() {
                mapped(positions, n - 1, input_len)
            } else {
                mapped(positions, domain_start + start, input_len)
            };
            return Err(EmailError::EmptyLabel { at: error_at });
        }

        if end - start > 63 {
            return Err(EmailError::LabelTooLong {
                at: mapped(positions, domain_start + start + 63, input_len),
            });
        }
        if domain[start] == b'-' {
            return Err(EmailError::LabelHyphen {
                at: mapped(positions, domain_start + start, input_len),
            });
        }
        if domain[end - 1] == b'-' {
            return Err(EmailError::LabelHyphen {
                at: mapped(positions, domain_start + end - 1, input_len),
            });
        }
        for i in start..end {
            if !(domain[i].is_ascii_alphanumeric() || domain[i] == b'-') {
                return Err(EmailError::DomainChar {
                    at: mapped(positions, domain_start + i, input_len),
                });
            }
        }
    }

    if labels.len() < 2 && !allow_single_label {
        return Err(EmailError::DomainNeedsDot {
            at: mapped(positions, domain_start, input_len),
        });
    }

    let (last_start, last_end) = labels[labels.len() - 1];
    if domain[last_start..last_end]
        .iter()
        .all(u8::is_ascii_digit)
    {
        return Err(EmailError::NumericTld {
            at: mapped(positions, domain_start + last_start, input_len),
        });
    }

    let local = String::from_utf8(decoded).expect("validated ASCII local");
    let domain = domain
        .iter()
        .map(u8::to_ascii_lowercase)
        .map(char::from)
        .collect();

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
    if input.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let raw = input.as_bytes();
    if let Some(at) = raw.iter().position(|&b| b >= 128) {
        return Err(EmailError::NonAscii { at });
    }

    let cleaned = cleanup(raw)?;
    parse_cleaned(&cleaned, input.len(), false).map(|result| result.0)
}

pub fn normalize_email(input: &str, policy: &Policy) -> Result<Address, EmailError> {
    if input.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let raw = input.as_bytes();
    if let Some(at) = raw.iter().position(|&b| b >= 128) {
        return Err(EmailError::NonAscii { at });
    }

    let cleaned = cleanup(raw)?;
    let (mut address, origins) =
        parse_cleaned(&cleaned, input.len(), policy.allow_single_label)?;

    let mut local = address.local.into_bytes();
    if policy.local_case == LocalCase::Lower {
        for b in &mut local {
            *b = b.to_ascii_lowercase();
        }
    }

    if policy.strip_subaddress {
        if let Some(plus) = local.iter().position(|&b| b == b'+') {
            if plus == 0 {
                return Err(EmailError::EmptyNormalizedLocal {
                    at: origins.first().copied().unwrap_or(input.len()),
                });
            }
            local.truncate(plus);
        }
    }

    let safe = dot_atom_error(&local).is_none();
    if !safe {
        address.quoted = true;
    } else if policy.unquote_safe {
        address.quoted = false;
    }

    address.local = String::from_utf8(local).expect("validated ASCII local");
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