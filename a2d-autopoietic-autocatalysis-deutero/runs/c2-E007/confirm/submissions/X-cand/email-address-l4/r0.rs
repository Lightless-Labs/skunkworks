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
struct Byte {
    value: u8,
    pos: usize,
}

struct Cleanup {
    bytes: Vec<Byte>,
    runs: Vec<usize>,
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

fn cleanup(input: &[u8]) -> Result<Cleanup, EmailError> {
    let mut bytes = Vec::new();
    let mut runs = Vec::new();
    let mut i = 0usize;
    let mut comment_depth = 0usize;
    let mut outer_open = 0usize;
    let mut run_open = false;
    let mut quote_mode = false;

    while i < input.len() {
        let b = input[i];

        if quote_mode {
            bytes.push(Byte { value: b, pos: i });
            if b == b'\\' {
                i += 1;
                if i < input.len() {
                    bytes.push(Byte { value: input[i], pos: i });
                    i += 1;
                }
            } else {
                if b == b'"' {
                    quote_mode = false;
                }
                i += 1;
            }
            continue;
        }

        if comment_depth > 0 {
            if b == b'\\' {
                run_open = true;
                if i + 1 >= input.len() || !(0x20..=0x7e).contains(&input[i + 1]) {
                    return Err(EmailError::CommentEscape { at: i });
                }
                i += 2;
                continue;
            }
            if b == b'\r' || b == b'\n' {
                run_open = true;
                if let Some(end) = fold_end(input, i) {
                    i = end;
                    continue;
                }
                return Err(EmailError::BadFold { at: i });
            }
            if is_hws(b) {
                run_open = true;
                i += 1;
                continue;
            }
            if b == b'(' {
                run_open = true;
                if comment_depth == 3 {
                    return Err(EmailError::CommentDepth { at: i });
                }
                comment_depth += 1;
                i += 1;
                continue;
            }
            if b == b')' {
                run_open = true;
                comment_depth -= 1;
                i += 1;
                continue;
            }
            if !(0x20..=0x7e).contains(&b) {
                return Err(EmailError::CommentChar { at: i });
            }
            i += 1;
            continue;
        }

        if b == b'\r' || b == b'\n' {
            run_open = true;
            if let Some(end) = fold_end(input, i) {
                i = end;
                continue;
            }
            return Err(EmailError::BadFold { at: i });
        }
        if is_hws(b) {
            run_open = true;
            i += 1;
            continue;
        }
        if b == b'(' {
            if !run_open {
                run_open = true;
                runs.push(i);
            }
            comment_depth = 1;
            outer_open = i;
            i += 1;
            continue;
        }
        if b == b')' {
            return Err(EmailError::UnexpectedClose { at: i });
        }

        if run_open {
            run_open = false;
        }
        let is_first_retained = bytes.is_empty();
        if b == b'"' && is_first_retained {
            quote_mode = true;
        }
        bytes.push(Byte { value: b, pos: i });
        i += 1;
    }

    if comment_depth > 0 {
        return Err(EmailError::UnclosedComment { at: outer_open });
    }

    // The quote mode intentionally permits an unclosed quote; address parsing reports it.
    let _ = quote_mode;

    // Record whitespace/comment runs not already opened by a comment.
    // Runs are detected by rescanning the source and associating removed spans.
    let mut scan = 0usize;
    let mut protected_quote = false;
    let mut retained_seen = false;
    while scan < input.len() {
        let b = input[scan];
        if protected_quote {
            if b == b'\\' {
                scan += 1;
                if scan < input.len() {
                    scan += 1;
                }
                continue;
            }
            if b == b'"' {
                protected_quote = false;
            }
            retained_seen = true;
            scan += 1;
            continue;
        }
        if (is_hws(b) || b == b'\r' || b == b'\n' || b == b'(') {
            let start = scan;
            let mut depth = 0usize;
            while scan < input.len() {
                let c = input[scan];
                if is_hws(c) {
                    scan += 1;
                } else if c == b'\r' || c == b'\n' {
                    if let Some(end) = fold_end(input, scan) {
                        scan = end;
                    } else {
                        scan += 1;
                    }
                } else if c == b'(' {
                    depth += 1;
                    scan += 1;
                } else if c == b')' && depth > 0 {
                    depth -= 1;
                    scan += 1;
                    if depth == 0 {
                        while scan < input.len() && is_hws(input[scan]) {
                            scan += 1;
                        }
                    }
                } else if depth > 0 {
                    if c == b'\\' && scan + 1 < input.len() {
                        scan += 2;
                    } else {
                        scan += 1;
                    }
                } else {
                    break;
                }
            }
            if !runs.contains(&start) {
                runs.push(start);
            }
            continue;
        }
        if b == b'"' && !retained_seen {
            protected_quote = true;
        }
        retained_seen = true;
        scan += 1;
    }
    runs.sort_unstable();

    Ok(Cleanup { bytes, runs })
}

fn valid_dot_atom(local: &[u8]) -> bool {
    if local.is_empty() {
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
        } else if b.is_ascii_alphanumeric() || specials.contains(&b) {
            prev_dot = false;
        } else {
            return false;
        }
    }
    true
}

fn parse_clean(
    clean: &Cleanup,
    input_len: usize,
    allow_single_label: bool,
) -> Result<(Address, Vec<usize>), EmailError> {
    let bytes = &clean.bytes;

    for &start in &clean.runs {
        let before = bytes.iter().rev().find(|b| b.pos < start);
        let after = bytes.iter().find(|b| b.pos >= start);
        let at_before = before.map(|b| b.value == b'@').unwrap_or(false);
        let at_after = after.map(|b| b.value == b'@').unwrap_or(false);
        let at_boundary = before.is_none() || after.is_none();
        if !(at_boundary || at_before || at_after) {
            return Err(EmailError::CfwsBoundary { at: start });
        }
    }

    if bytes.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let mut quote_close = None;
    if bytes[0].value == b'"' {
        let mut i = 1usize;
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
            return Err(EmailError::UnclosedQuote { at: bytes[0].pos });
        }
    }

    let mut ats = Vec::new();
    for (i, b) in bytes.iter().enumerate() {
        if b.value == b'@' {
            if quote_close.map(|q| i <= q).unwrap_or(false) {
                continue;
            }
            ats.push(i);
        }
    }
    if ats.is_empty() {
        return Err(EmailError::MissingAt);
    }
    if ats.len() >= 2 {
        return Err(EmailError::MultipleAt {
            at: bytes[ats[1]].pos,
        });
    }

    let at_i = ats[0];
    let at_pos = bytes[at_i].pos;
    if at_i == 0 {
        return Err(EmailError::EmptyLocal { at: at_pos });
    }

    let mut decoded = Vec::new();
    let mut origins = Vec::new();
    let quoted;

    if let Some(q) = quote_close {
        if q + 1 != at_i {
            return Err(EmailError::TrailingLocal {
                at: bytes[q + 1].pos,
            });
        }
        quoted = true;
        let mut i = 1usize;
        while i < q {
            let b = bytes[i].value;
            if b == b'\\' {
                if i + 1 >= q {
                    return Err(EmailError::BadEscape { at: bytes[i].pos });
                }
                let next = bytes[i + 1].value;
                if next != b'"' && next != b'\\' && next != b'@' {
                    return Err(EmailError::BadEscape { at: bytes[i].pos });
                }
                decoded.push(next);
                origins.push(bytes[i].pos);
                i += 2;
            } else {
                if !(0x20..=0x7e).contains(&b) {
                    return Err(EmailError::QuotedChar { at: bytes[i].pos });
                }
                decoded.push(b);
                origins.push(bytes[i].pos);
                i += 1;
            }
        }
        if decoded.is_empty() {
            return Err(EmailError::EmptyLocal {
                at: bytes[q].pos,
            });
        }
        if decoded.len() > 64 {
            return Err(EmailError::LocalTooLong { at: origins[64] });
        }
    } else {
        quoted = false;
        let local = &bytes[..at_i];
        if local.len() > 64 {
            return Err(EmailError::LocalTooLong {
                at: local[64].pos,
            });
        }
        let specials = b"!#$%&'*+-/=?^_`{|}~";
        let mut prev_dot = false;
        for (i, b) in local.iter().enumerate() {
            if b.value == b'.' {
                if i == 0 || i + 1 == local.len() || prev_dot {
                    return Err(EmailError::LocalDot { at: b.pos });
                }
                prev_dot = true;
            } else {
                if !b.value.is_ascii_alphanumeric() && !specials.contains(&b.value) {
                    return Err(EmailError::LocalChar { at: b.pos });
                }
                prev_dot = false;
            }
            decoded.push(b.value);
            origins.push(b.pos);
        }
    }

    let domain_start = if at_i + 1 < bytes.len() {
        bytes[at_i + 1].pos
    } else {
        input_len
    };
    let domain_raw = &bytes[at_i + 1..];
    if domain_raw.is_empty() {
        return Err(EmailError::EmptyDomain { at: domain_start });
    }

    let mut domain_end = domain_raw.len();
    let root_pos = if domain_raw.last().map(|b| b.value == b'.').unwrap_or(false) {
        domain_end -= 1;
        Some(domain_raw[domain_end].pos)
    } else {
        None
    };
    if domain_end == 0 {
        return Err(EmailError::EmptyDomain { at: domain_start });
    }
    if domain_end > 253 {
        return Err(EmailError::DomainTooLong {
            at: domain_raw[253].pos,
        });
    }

    let domain = &domain_raw[..domain_end];
    let mut label_ranges = Vec::new();
    let mut start = 0usize;
    for i in 0..=domain.len() {
        if i == domain.len() || domain[i].value == b'.' {
            label_ranges.push((start, i));
            start = i + 1;
        }
    }
    for (li, &(s, e)) in label_ranges.iter().enumerate() {
        if s == e {
            let pos = if li == label_ranges.len() - 1 && root_pos.is_some() {
                root_pos.unwrap()
            } else if s < domain.len() {
                domain[s].pos
            } else {
                input_len
            };
            return Err(EmailError::EmptyLabel { at: pos });
        }
        if e - s > 63 {
            return Err(EmailError::LabelTooLong {
                at: domain[s + 63].pos,
            });
        }
        if domain[s].value == b'-' {
            return Err(EmailError::LabelHyphen { at: domain[s].pos });
        }
        if domain[e - 1].value == b'-' {
            return Err(EmailError::LabelHyphen {
                at: domain[e - 1].pos,
            });
        }
        for b in &domain[s..e] {
            if !b.value.is_ascii_alphanumeric() && b.value != b'-' {
                return Err(EmailError::DomainChar { at: b.pos });
            }
        }
    }

    if label_ranges.len() < 2 && !allow_single_label {
        return Err(EmailError::DomainNeedsDot { at: domain_start });
    }
    let (last_s, last_e) = *label_ranges.last().unwrap();
    if domain[last_s..last_e]
        .iter()
        .all(|b| b.value.is_ascii_digit())
    {
        return Err(EmailError::NumericTld {
            at: domain[last_s].pos,
        });
    }

    let local = String::from_utf8(decoded).unwrap();
    let domain = domain
        .iter()
        .map(|b| b.value.to_ascii_lowercase() as char)
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
    normalize_email(input, &Policy::default())
}

pub fn normalize_email(input: &str, policy: &Policy) -> Result<Address, EmailError> {
    if input.is_empty() {
        return Err(EmailError::EmptyInput);
    }
    let raw = input.as_bytes();
    if let Some(at) = raw.iter().position(|b| *b >= 128) {
        return Err(EmailError::NonAscii { at });
    }
    let clean = cleanup(raw)?;
    let (mut address, origins) = parse_clean(&clean, input.len(), policy.allow_single_label)?;

    if policy.local_case == LocalCase::Lower {
        address.local.make_ascii_lowercase();
    }

    if policy.strip_subaddress {
        if let Some(plus) = address.local.find('+') {
            if plus == 0 {
                return Err(EmailError::EmptyNormalizedLocal {
                    at: origins.get(0).copied().unwrap_or(input.len()),
                });
            }
            address.local.truncate(plus);
        }
    }

    let safe = valid_dot_atom(address.local.as_bytes());
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
    let mut result = DedupResult {
        addresses: Vec::new(),
        source_indices: Vec::new(),
        duplicate_of: vec![None; inputs.len()],
        errors: Vec::new(),
    };
    let mut keys: std::collections::HashMap<(String, String), usize> =
        std::collections::HashMap::new();

    for (index, input) in inputs.iter().enumerate() {
        match normalize_email(input, policy) {
            Err(error) => result.errors.push((index, error)),
            Ok(address) => {
                let key = (address.local.clone(), address.domain.clone());
                if let Some(&first_index) = keys.get(&key) {
                    result.duplicate_of[index] = Some(first_index);
                } else {
                    keys.insert(key, index);
                    result.addresses.push(address);
                    result.source_indices.push(index);
                }
            }
        }
    }
    result
}