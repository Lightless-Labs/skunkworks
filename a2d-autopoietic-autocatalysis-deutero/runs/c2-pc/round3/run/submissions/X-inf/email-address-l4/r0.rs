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
    pos: usize,
}

struct Run {
    start: usize,
    end: usize,
}

fn bad_fold_at(bytes: &[u8], i: usize) -> Option<usize> {
    if bytes[i] == b'\r' {
        if i + 2 < bytes.len()
            && bytes[i + 1] == b'\n'
            && matches!(bytes[i + 2], b' ' | b'\t')
        {
            None
        } else {
            Some(i)
        }
    } else if bytes[i] == b'\n' {
        Some(i)
    } else {
        None
    }
}

fn cleanup(input: &str) -> Result<(Vec<Byte>, Vec<Run>), EmailError> {
    let b = input.as_bytes();
    let mut kept = Vec::with_capacity(b.len());
    let mut runs = Vec::new();
    let mut i = 0;

    // Only the initial retained quote prefix protects content during cleanup.
    let mut quote_prefix = false;

    while i < b.len() {
        if quote_prefix {
            let start = i;
            while i < b.len() {
                let c = b[i];
                kept.push(Byte { value: c, pos: i });
                i += 1;
                if c == b'\\' {
                    if i < b.len() {
                        kept.push(Byte {
                            value: b[i],
                            pos: i,
                        });
                        i += 1;
                    }
                } else if c == b'"' {
                    quote_prefix = false;
                    break;
                }
            }
            let _ = start;
            continue;
        }

        let c = b[i];

        if c == b'"' && kept.is_empty() {
            quote_prefix = true;
            kept.push(Byte { value: c, pos: i });
            i += 1;
            continue;
        }

        if matches!(c, b' ' | b'\t' | b'\r' | b'\n' | b'(') {
            let run_start = i;
            loop {
                if i >= b.len() {
                    break;
                }
                match b[i] {
                    b' ' | b'\t' => i += 1,
                    b'\r' | b'\n' => {
                        if let Some(at) = bad_fold_at(b, i) {
                            return Err(EmailError::BadFold { at });
                        }
                        i += 2;
                        while i < b.len() && matches!(b[i], b' ' | b'\t') {
                            i += 1;
                        }
                    }
                    b'(' => {
                        let outer = i;
                        let mut depth = 0usize;
                        loop {
                            if i >= b.len() {
                                return Err(EmailError::UnclosedComment { at: outer });
                            }
                            let x = b[i];
                            match x {
                                b'\\' => {
                                    let slash = i;
                                    if i + 1 >= b.len()
                                        || !(0x20..=0x7e).contains(&b[i + 1])
                                    {
                                        return Err(EmailError::CommentEscape { at: slash });
                                    }
                                    i += 2;
                                }
                                b'(' => {
                                    depth += 1;
                                    if depth > 3 {
                                        return Err(EmailError::CommentDepth { at: i });
                                    }
                                    i += 1;
                                }
                                b')' => {
                                    depth -= 1;
                                    i += 1;
                                    if depth == 0 {
                                        break;
                                    }
                                }
                                b'\r' | b'\n' => {
                                    if let Some(at) = bad_fold_at(b, i) {
                                        return Err(EmailError::BadFold { at });
                                    }
                                    i += 2;
                                    while i < b.len() && matches!(b[i], b' ' | b'\t') {
                                        i += 1;
                                    }
                                }
                                b' ' | b'\t' => i += 1,
                                0x20..=0x7e => i += 1,
                                _ => return Err(EmailError::CommentChar { at: i }),
                            }
                        }
                    }
                    _ => break,
                }
            }
            runs.push(Run {
                start: run_start,
                end: i,
            });
            continue;
        }

        if c == b')' {
            return Err(EmailError::UnexpectedClose { at: i });
        }
        if let Some(at) = bad_fold_at(b, i) {
            return Err(EmailError::BadFold { at });
        }
        kept.push(Byte { value: c, pos: i });
        i += 1;
    }

    // Validate run boundaries only after all cleanup lexical errors are known.
    for r in &runs {
        let left = kept.iter().position(|x| x.pos >= r.end);
        let right = kept.iter().rposition(|x| x.pos < r.start);
        let left_ok = match left {
            None => true,
            Some(ix) => kept[ix].value == b'@',
        };
        let right_ok = match right {
            None => true,
            Some(ix) => kept[ix].value == b'@',
        };
        // A run is boundary legal if at sequence edge or adjacent to @.
        if !(left_ok || right_ok) {
            return Err(EmailError::CfwsBoundary { at: r.start });
        }
    }

    Ok((kept, runs))
}

fn pos(bytes: &[Byte], i: usize, input_len: usize) -> usize {
    bytes.get(i).map_or(input_len, |x| x.pos)
}

fn dot_atom_valid(s: &[u8]) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut prev_dot = true;
    for (i, &c) in s.iter().enumerate() {
        if c == b'.' {
            if prev_dot || i + 1 == s.len() {
                return false;
            }
            prev_dot = true;
        } else if c.is_ascii_alphanumeric()
            || b"!#$%&'*+-/=?^_`{|}~".contains(&c)
        {
            prev_dot = false;
        } else {
            return false;
        }
    }
    true
}

fn parse_core(
    input: &str,
    allow_single_label: bool,
) -> Result<(Address, Vec<usize>), EmailError> {
    if input.is_empty() {
        return Err(EmailError::EmptyInput);
    }
    if let Some(at) = input.as_bytes().iter().position(|&c| c >= 128) {
        return Err(EmailError::NonAscii { at });
    }

    let (bytes, _) = cleanup(input)?;
    if bytes.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let input_len = input.len();
    let quoted_prefix = bytes[0].value == b'"';
    let closing = if quoted_prefix {
        let mut i = 1;
        let mut found = None;
        while i < bytes.len() {
            match bytes[i].value {
                b'\\' => i += 2,
                b'"' => {
                    found = Some(i);
                    break;
                }
                _ => i += 1,
            }
        }
        match found {
            Some(i) => Some(i),
            None => return Err(EmailError::UnclosedQuote { at: bytes[0].pos }),
        }
    } else {
        None
    };

    let mut ats = Vec::new();
    for i in 0..bytes.len() {
        if bytes[i].value == b'@'
            && !(quoted_prefix && closing.map_or(false, |c| i <= c))
        {
            ats.push(i);
        }
    }
    if ats.is_empty() {
        return Err(EmailError::MissingAt);
    }
    if ats.len() > 1 {
        return Err(EmailError::MultipleAt {
            at: bytes[ats[1]].pos,
        });
    }
    let sep = ats[0];
    if sep == 0 {
        return Err(EmailError::EmptyLocal {
            at: bytes[sep].pos,
        });
    }

    let mut local = Vec::new();
    let mut origins = Vec::new();
    let mut is_quoted = false;

    if quoted_prefix {
        let close = closing.unwrap();
        if close + 1 != sep {
            return Err(EmailError::TrailingLocal {
                at: pos(&bytes, close + 1, input_len),
            });
        }
        is_quoted = true;
        let mut i = 1;
        while i < close {
            let c = bytes[i].value;
            if c == b'\\' {
                if i + 1 >= close {
                    return Err(EmailError::BadEscape { at: bytes[i].pos });
                }
                let next = bytes[i + 1].value;
                if !matches!(next, b'"' | b'\\' | b'@') {
                    return Err(EmailError::BadEscape { at: bytes[i].pos });
                }
                local.push(next);
                origins.push(bytes[i].pos);
                i += 2;
            } else {
                if !(0x20..=0x7e).contains(&c) {
                    return Err(EmailError::QuotedChar { at: bytes[i].pos });
                }
                local.push(c);
                origins.push(bytes[i].pos);
                i += 1;
            }
        }
        if local.is_empty() {
            return Err(EmailError::EmptyLocal {
                at: bytes[close].pos,
            });
        }
        if local.len() > 64 {
            return Err(EmailError::LocalTooLong { at: origins[64] });
        }
    } else {
        if sep > 64 {
            return Err(EmailError::LocalTooLong { at: bytes[64].pos });
        }
        let mut prev_dot = false;
        for i in 0..sep {
            let c = bytes[i].value;
            if c == b'.' {
                if i == 0 || i + 1 == sep || prev_dot {
                    return Err(EmailError::LocalDot { at: bytes[i].pos });
                }
                prev_dot = true;
            } else {
                if !c.is_ascii_alphanumeric()
                    && !b"!#$%&'*+-/=?^_`{|}~".contains(&c)
                {
                    return Err(EmailError::LocalChar { at: bytes[i].pos });
                }
                prev_dot = false;
            }
            local.push(c);
            origins.push(bytes[i].pos);
        }
    }

    let domain_start = sep + 1;
    let domain_end = bytes.len();
    let rooted = domain_end > domain_start && bytes[domain_end - 1].value == b'.';
    let effective_end = if rooted { domain_end - 1 } else { domain_end };
    if effective_end == domain_start {
        return Err(EmailError::EmptyDomain {
            at: pos(&bytes, domain_start, input_len),
        });
    }
    if effective_end - domain_start > 253 {
        return Err(EmailError::DomainTooLong {
            at: bytes[domain_start + 253].pos,
        });
    }

    let mut labels: Vec<(usize, usize)> = Vec::new();
    let mut start = domain_start;
    for i in domain_start..effective_end {
        if bytes[i].value == b'.' {
            labels.push((start, i));
            start = i + 1;
        }
    }
    labels.push((start, effective_end));

    for &(a, z) in &labels {
        if a == z {
            let at = if a == effective_end && rooted {
                bytes[domain_end - 1].pos
            } else if a == domain_start {
                pos(&bytes, a, input_len)
            } else {
                bytes[a - 1].pos
            };
            return Err(EmailError::EmptyLabel { at });
        }
        if z - a > 63 {
            return Err(EmailError::LabelTooLong { at: bytes[a + 63].pos });
        }
        if bytes[a].value == b'-' {
            return Err(EmailError::LabelHyphen { at: bytes[a].pos });
        }
        if bytes[z - 1].value == b'-' {
            return Err(EmailError::LabelHyphen {
                at: bytes[z - 1].pos,
            });
        }
        for i in a..z {
            let c = bytes[i].value;
            if !c.is_ascii_alphanumeric() && c != b'-' {
                return Err(EmailError::DomainChar { at: bytes[i].pos });
            }
        }
    }

    if labels.len() < 2 && !allow_single_label {
        return Err(EmailError::DomainNeedsDot {
            at: pos(&bytes, domain_start, input_len),
        });
    }

    let (last_start, last_end) = *labels.last().unwrap();
    if bytes[last_start..last_end]
        .iter()
        .all(|x| x.value.is_ascii_digit())
    {
        return Err(EmailError::NumericTld {
            at: bytes[last_start].pos,
        });
    }

    let local_string = String::from_utf8(local).unwrap();
    let domain_string = bytes[domain_start..effective_end]
        .iter()
        .map(|x| x.value as char)
        .collect::<String>()
        .to_ascii_lowercase();

    Ok((
        Address {
            local: local_string,
            domain: domain_string,
            quoted: is_quoted,
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
        if let Some(ix) = address.local.as_bytes().iter().position(|&c| c == b'+') {
            if ix == 0 {
                return Err(EmailError::EmptyNormalizedLocal {
                    at: origins.first().copied().unwrap_or(input.len()),
                });
            }
            address.local.truncate(ix);
        }
    }

    let safe = dot_atom_valid(address.local.as_bytes());
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
                    .find(|(local, domain, _)| {
                        local == &address.local && domain == &address.domain
                    })
                {
                    duplicate_of[index] = Some(*first_index);
                } else {
                    keys.push((
                        address.local.clone(),
                        address.domain.clone(),
                        index,
                    ));
                    addresses.push(address);
                    source_indices.push(index);
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