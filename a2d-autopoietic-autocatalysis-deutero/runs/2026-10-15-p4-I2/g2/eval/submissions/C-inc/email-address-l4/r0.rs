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

fn error_at<F>(make: F, at: usize) -> EmailError
where
    F: FnOnce(usize) -> EmailError,
{
    make(at)
}

fn is_hws(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

fn fold_end(bytes: &[u8], i: usize) -> Option<usize> {
    if bytes.get(i) == Some(&b'\r') && bytes.get(i + 1) == Some(&b'\n') {
        let mut j = i + 2;
        if !bytes.get(j).copied().is_some_and(is_hws) {
            return None;
        }
        while bytes.get(j).copied().is_some_and(is_hws) {
            j += 1;
        }
        Some(j)
    } else {
        None
    }
}

#[derive(Clone)]
struct Run {
    start: usize,
    end: usize,
}

struct Cleaned {
    bytes: Vec<u8>,
    positions: Vec<usize>,
    runs: Vec<Run>,
}

fn cleanup(input: &[u8]) -> Result<Cleaned, EmailError> {
    let mut bytes = Vec::with_capacity(input.len());
    let mut positions = Vec::with_capacity(input.len());
    let mut runs = Vec::new();
    let mut i = 0usize;
    let mut comment_depth = 0usize;
    let mut outer_comment = 0usize;
    let mut protected_quote = false;
    let mut quoted_escape = false;

    while i < input.len() {
        if protected_quote {
            let b = input[i];
            bytes.push(b);
            positions.push(i);
            if quoted_escape {
                quoted_escape = false;
            } else if b == b'\\' {
                quoted_escape = true;
            } else if b == b'"' {
                protected_quote = false;
            }
            i += 1;
            continue;
        }

        if comment_depth > 0 {
            let b = input[i];
            if b == b'\\' {
                let at = i;
                if i + 1 >= input.len() || !(0x20..=0x7e).contains(&input[i + 1]) {
                    return Err(EmailError::CommentEscape { at });
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
                continue;
            }
            if !(b == b'\t' || (0x20..=0x7e).contains(&b)) {
                return Err(EmailError::CommentChar { at: i });
            }
            i += 1;
            continue;
        }

        if input[i] == b'"' && bytes.is_empty() {
            protected_quote = true;
            bytes.push(input[i]);
            positions.push(i);
            i += 1;
            continue;
        }

        let b = input[i];
        let is_cfws_start = is_hws(b)
            || b == b'('
            || b == b'\r'
            || b == b'\n';
        if is_cfws_start {
            let start = i;
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
                    outer_comment = i;
                    comment_depth = 1;
                    i += 1;
                    while i < input.len() && comment_depth > 0 {
                        let x = input[i];
                        if x == b'\\' {
                            let at = i;
                            if i + 1 >= input.len()
                                || !(0x20..=0x7e).contains(&input[i + 1])
                            {
                                return Err(EmailError::CommentEscape { at });
                            }
                            i += 2;
                        } else if x == b'\r' || x == b'\n' {
                            if let Some(end) = fold_end(input, i) {
                                i = end;
                            } else {
                                return Err(EmailError::BadFold { at: i });
                            }
                        } else if x == b'(' {
                            if comment_depth == 3 {
                                return Err(EmailError::CommentDepth { at: i });
                            }
                            comment_depth += 1;
                            i += 1;
                        } else if x == b')' {
                            comment_depth -= 1;
                            i += 1;
                        } else if x == b'\t' || (0x20..=0x7e).contains(&x) {
                            i += 1;
                        } else {
                            return Err(EmailError::CommentChar { at: i });
                        }
                    }
                    if comment_depth != 0 {
                        return Err(EmailError::UnclosedComment { at: outer_comment });
                    }
                } else {
                    break;
                }
            }
            runs.push(Run { start, end: i });
            continue;
        }

        if b == b')' {
            return Err(EmailError::UnexpectedClose { at: i });
        }
        bytes.push(b);
        positions.push(i);
        i += 1;
    }

    if comment_depth > 0 {
        return Err(EmailError::UnclosedComment { at: outer_comment });
    }

    Ok(Cleaned {
        bytes,
        positions,
        runs,
    })
}

fn map_pos(cleaned: &Cleaned, idx: usize, input_len: usize) -> usize {
    cleaned.positions.get(idx).copied().unwrap_or(input_len)
}

fn valid_atom(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
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

fn parse_internal(input: &str, allow_single_label: bool) -> Result<(Address, usize), EmailError> {
    let raw = input.as_bytes();
    if raw.is_empty() {
        return Err(EmailError::EmptyInput);
    }
    if let Some(at) = raw.iter().position(|&b| b >= 128) {
        return Err(EmailError::NonAscii { at });
    }

    let cleaned = cleanup(raw)?;
    let b = &cleaned.bytes;

    for run in &cleaned.runs {
        let left_at = run.start > 0 && b.get(run.start - 1) == Some(&b'@');
        let right_at = b.get(run.start) == Some(&b'@');
        let at_edge = run.start == 0 || run.end == raw.len();
        if !(left_at || right_at || at_edge) {
            return Err(EmailError::CfwsBoundary {
                at: run.start,
            });
        }
    }

    if b.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let mut quote_end = None;
    if b[0] == b'"' {
        let mut i = 1;
        while i < b.len() {
            if b[i] == b'\\' {
                i += 2;
            } else if b[i] == b'"' {
                quote_end = Some(i);
                break;
            } else {
                i += 1;
            }
        }
        if quote_end.is_none() {
            return Err(EmailError::UnclosedQuote {
                at: map_pos(&cleaned, 0, raw.len()),
            });
        }
    }

    let mut ats = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if let Some(end) = quote_end {
            if i <= end {
                i = end + 1;
                continue;
            }
        }
        if b[i] == b'@' {
            ats.push(i);
        }
        i += 1;
    }

    if ats.is_empty() {
        return Err(EmailError::MissingAt);
    }
    if ats.len() >= 2 {
        return Err(EmailError::MultipleAt {
            at: map_pos(&cleaned, ats[1], raw.len()),
        });
    }
    let sep = ats[0];
    if sep == 0 {
        return Err(EmailError::EmptyLocal {
            at: map_pos(&cleaned, sep, raw.len()),
        });
    }

    let mut local = Vec::new();
    let mut origins = Vec::new();
    let quoted;
    if b[0] == b'"' {
        let end = quote_end.unwrap();
        if end + 1 != sep {
            return Err(EmailError::TrailingLocal {
                at: map_pos(&cleaned, end + 1, raw.len()),
            });
        }
        quoted = true;
        let mut j = 1;
        while j < end {
            let c = b[j];
            if c == b'\\' {
                if j + 1 >= end || ![b'"', b'\\', b'@'].contains(&b[j + 1]) {
                    return Err(EmailError::BadEscape {
                        at: map_pos(&cleaned, j, raw.len()),
                    });
                }
                local.push(b[j + 1]);
                origins.push(map_pos(&cleaned, j, raw.len()));
                j += 2;
            } else {
                if !(0x20..=0x7e).contains(&c) {
                    return Err(EmailError::QuotedChar {
                        at: map_pos(&cleaned, j, raw.len()),
                    });
                }
                local.push(c);
                origins.push(map_pos(&cleaned, j, raw.len()));
                j += 1;
            }
        }
        if local.is_empty() {
            return Err(EmailError::EmptyLocal {
                at: map_pos(&cleaned, end, raw.len()),
            });
        }
        if local.len() > 64 {
            return Err(EmailError::LocalTooLong {
                at: origins[64],
            });
        }
    } else {
        quoted = false;
        let llen = sep;
        if llen > 64 {
            return Err(EmailError::LocalTooLong {
                at: map_pos(&cleaned, 64, raw.len()),
            });
        }
        for j in 0..llen {
            let c = b[j];
            if c == b'.' {
                if j == 0 || j + 1 == llen || b[j - 1] == b'.' {
                    return Err(EmailError::LocalDot {
                        at: map_pos(&cleaned, j, raw.len()),
                    });
                }
            } else if !c.is_ascii_alphanumeric()
                && !b"!#$%&'*+-/=?^_`{|}~".contains(&c)
            {
                return Err(EmailError::LocalChar {
                    at: map_pos(&cleaned, j, raw.len()),
                });
            }
            local.push(c);
            origins.push(map_pos(&cleaned, j, raw.len()));
        }
    }

    let domain_start = sep + 1;
    let mut domain_end = b.len();
    let root_pos = if domain_end > domain_start && b[domain_end - 1] == b'.' {
        domain_end -= 1;
        Some(domain_end)
    } else {
        None
    };

    if domain_end == domain_start {
        return Err(EmailError::EmptyDomain {
            at: map_pos(&cleaned, domain_start, raw.len()),
        });
    }
    if domain_end - domain_start > 253 {
        return Err(EmailError::DomainTooLong {
            at: map_pos(&cleaned, domain_start + 253, raw.len()),
        });
    }

    let mut label_starts = Vec::new();
    let mut label_ends = Vec::new();
    let mut start = domain_start;
    for j in domain_start..domain_end {
        if b[j] == b'.' {
            if j == start {
                let at = map_pos(&cleaned, j, raw.len());
                return Err(EmailError::EmptyLabel { at });
            }
            label_starts.push(start);
            label_ends.push(j);
            start = j + 1;
        }
    }
    if start == domain_end {
        let at = root_pos
            .map(|p| map_pos(&cleaned, p, raw.len()))
            .unwrap_or_else(|| map_pos(&cleaned, domain_end, raw.len()));
        return Err(EmailError::EmptyLabel { at });
    }
    label_starts.push(start);
    label_ends.push(domain_end);

    for (&ls, &le) in label_starts.iter().zip(label_ends.iter()) {
        let len = le - ls;
        if len > 63 {
            return Err(EmailError::LabelTooLong {
                at: map_pos(&cleaned, ls + 63, raw.len()),
            });
        }
        if b[ls] == b'-' {
            return Err(EmailError::LabelHyphen {
                at: map_pos(&cleaned, ls, raw.len()),
            });
        }
        if b[le - 1] == b'-' {
            return Err(EmailError::LabelHyphen {
                at: map_pos(&cleaned, le - 1, raw.len()),
            });
        }
        for j in ls..le {
            if !b[j].is_ascii_alphanumeric() && b[j] != b'-' {
                return Err(EmailError::DomainChar {
                    at: map_pos(&cleaned, j, raw.len()),
                });
            }
        }
    }

    if label_starts.len() < 2 && !allow_single_label {
        return Err(EmailError::DomainNeedsDot {
            at: map_pos(&cleaned, domain_start, raw.len()),
        });
    }

    let last_start = *label_starts.last().unwrap();
    let last_end = *label_ends.last().unwrap();
    if b[last_start..last_end].iter().all(u8::is_ascii_digit) {
        return Err(EmailError::NumericTld {
            at: map_pos(&cleaned, last_start, raw.len()),
        });
    }

    let domain = String::from_utf8(
        b[domain_start..domain_end]
            .iter()
            .map(u8::to_ascii_lowercase)
            .collect(),
    )
    .unwrap();

    Ok((
        Address {
            local: String::from_utf8(local).unwrap(),
            domain,
            quoted,
        },
        origins.first().copied().unwrap_or_else(|| {
            map_pos(&cleaned, 0, raw.len())
        }),
    ))
}

pub fn parse_email(input: &str) -> Result<Address, EmailError> {
    parse_internal(input, false).map(|(address, _)| address)
}

pub fn normalize_email(input: &str, policy: &Policy) -> Result<Address, EmailError> {
    let (mut address, first_origin) = parse_internal(input, policy.allow_single_label)?;
    if policy.local_case == LocalCase::Lower {
        address.local.make_ascii_lowercase();
    }

    if policy.strip_subaddress {
        if let Some(plus) = address.local.find('+') {
            if plus == 0 {
                return Err(EmailError::EmptyNormalizedLocal { at: first_origin });
            }
            address.local.truncate(plus);
        }
    }

    let local_bytes = address.local.as_bytes();
    let safe_atom = valid_atom(local_bytes);
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
        for c in address.local.chars() {
            if c == '"' || c == '\\' {
                out.push('\\');
            }
            out.push(c);
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