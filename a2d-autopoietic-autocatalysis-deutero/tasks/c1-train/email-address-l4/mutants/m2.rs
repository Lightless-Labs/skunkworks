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
pub enum LocalCase { Preserve, Lower }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    pub local_case: LocalCase,
    pub strip_subaddress: bool,
    pub unquote_safe: bool,
    pub allow_single_label: bool,
}

impl Default for Policy {
    fn default() -> Self {
        Self { local_case: LocalCase::Preserve, strip_subaddress: false,
               unquote_safe: false, allow_single_label: false }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DedupResult {
    pub addresses: Vec<Address>,
    pub source_indices: Vec<usize>,
    pub duplicate_of: Vec<Option<usize>>,
    pub errors: Vec<(usize, EmailError)>,
}

fn fold(raw: &[u8], i: &mut usize) -> Result<(), EmailError> {
    let start = *i;
    if raw.get(start) != Some(&b'\r') || raw.get(start + 1) != Some(&b'\n') ||
       !matches!(raw.get(start + 2), Some(b' ' | b'\t')) {
        return Err(EmailError::BadFold { at: start });
    }
    *i += 2;
    while matches!(raw.get(*i), Some(b' ' | b'\t')) { *i += 1; }
    Ok(())
}

fn comment(raw: &[u8], i: &mut usize) -> Result<(), EmailError> {
    let opening = *i;
    let mut depth = 1;
    *i += 1;
    while *i < raw.len() {
        let b = raw[*i];
        match b {
            b'(' => {
                depth += 1;
                if depth > 3 { return Err(EmailError::CommentDepth { at: *i }); }
                *i += 1;
            }
            b')' => {
                depth -= 1;
                *i += 1;
                if depth == 0 { return Ok(()); }
            }
            b'\\' => {
                if !raw.get(*i + 1).is_some_and(|b| (0x20..=0x7e).contains(b)) {
                    return Err(EmailError::CommentEscape { at: *i });
                }
                *i += 2;
            }
            b'\r' | b'\n' => fold(raw, i)?,
            b'\t' | 0x20..=0x7e => *i += 1,
            _ => return Err(EmailError::CommentChar { at: *i }),
        }
    }
    Err(EmailError::UnclosedComment { at: opening })
}

fn cleanup(raw: &[u8]) -> Result<(Vec<u8>, Vec<usize>), EmailError> {
    let mut bytes = Vec::new();
    let mut map = Vec::new();
    let mut runs = Vec::new();
    let mut i = 0;
    let mut quoted = false;
    let mut saw_prefix_quote = false;
    while i < raw.len() {
        if quoted {
            let b = raw[i];
            bytes.push(b); map.push(i); i += 1;
            if b == b'\\' && i < raw.len() { bytes.push(raw[i]); map.push(i); i += 1; }
            else if b == b'"' { quoted = false; }
            continue;
        }
        if matches!(raw[i], b' ' | b'\t' | b'\r' | b'\n' | b'(') {
            let run_start = i;
            let slot = bytes.len();
            while i < raw.len() && matches!(raw[i], b' ' | b'\t' | b'\r' | b'\n' | b'(') {
                match raw[i] {
                    b' ' | b'\t' => i += 1,
                    b'(' => comment(raw, &mut i)?,
                    _ => fold(raw, &mut i)?,
                }
            }
            runs.push((slot, run_start));
            continue;
        }
        if raw[i] == b')' { return Err(EmailError::UnexpectedClose { at: i }); }
        if bytes.is_empty() && !saw_prefix_quote && raw[i] == b'"' {
            quoted = true; saw_prefix_quote = true;
        }
        bytes.push(raw[i]); map.push(i); i += 1;
    }
    for (slot, at) in runs {
        if slot != 0 && slot != bytes.len() && bytes[slot - 1] != b'@' && bytes[slot] != b'@' {
            return Err(EmailError::CfwsBoundary { at });
        }
    }
    Ok((bytes, map))
}

fn offset(map: &[usize], i: usize, end: usize) -> usize {
    map.get(i).copied().unwrap_or(end)
}

fn atext(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$%&'*+-/=?^_`{|}~".contains(&b)
}

fn atom_ok(bytes: &[u8]) -> bool {
    !bytes.is_empty() && bytes.len() <= 64 &&
    bytes.iter().enumerate().all(|(i, &b)| {
        if b == b'.' { i != 0 && i + 1 != bytes.len() && bytes[i - 1] != b'.' }
        else { atext(b) }
    })
}

fn parse_core(input: &str, allow_single: bool) -> Result<(Address, Vec<usize>), EmailError> {
    use EmailError::*;
    let raw = input.as_bytes();
    if raw.is_empty() { return Err(EmptyInput); }
    if let Some(at) = raw.iter().position(|b| !b.is_ascii()) {
        return Err(NonAscii { at });
    }
    let (bytes, map) = cleanup(raw)?;
    let at_pos = |i| offset(&map, i, input.len());
    if bytes.is_empty() { return Err(EmptyInput); }
    let mut close = None;
    if bytes[0] == b'"' {
        let mut i = 1;
        while i < bytes.len() {
            if bytes[i] == b'\\' { i += 2; }
            else if bytes[i] == b'"' { close = Some(i); break; }
            else { i += 1; }
        }
        if close.is_none() { return Err(UnclosedQuote { at: at_pos(0) }); }
    }
    let ats: Vec<usize> = bytes.iter().enumerate()
        .filter_map(|(i, &b)| (b == b'@' && !close.is_some_and(|c| i <= c)).then_some(i)).collect();
    if ats.is_empty() { return Err(MissingAt); }
    if ats.len() > 1 { return Err(MultipleAt { at: at_pos(ats[1]) }); }
    let sep = ats[0];
    if sep == 0 { return Err(EmptyLocal { at: at_pos(0) }); }
    let local_bytes = &bytes[..sep];
    let mut local_map = Vec::new();
    let (local, quoted) = if let Some(c) = close {
        if c + 1 != sep { return Err(TrailingLocal { at: at_pos(c + 1) }); }
        let mut decoded = Vec::new();
        let mut i = 1;
        while i < c {
            let b = bytes[i];
            if b == b'\\' {
                if !b"\"\\@".contains(&bytes[i + 1]) {
                    return Err(BadEscape { at: at_pos(i) });
                }
                decoded.push(bytes[i + 1]);
                local_map.push(at_pos(i));
                i += 2;
            } else {
                if !(0x20..=0x7e).contains(&b) { return Err(QuotedChar { at: at_pos(i) }); }
                decoded.push(b);
                local_map.push(at_pos(i));
                i += 1;
            }
        }
        if decoded.is_empty() { return Err(EmptyLocal { at: at_pos(1) }); }
        if decoded.len() > 64 { return Err(LocalTooLong { at: local_map[64] }); }
        (String::from_utf8(decoded).unwrap(), true)
    } else {
    let (local, quoted) = {
        if local_bytes.len() > 64 { return Err(LocalTooLong { at: at_pos(64) }); }
        for (i, &b) in local_bytes.iter().enumerate() {
            if b == b'.' {
                if i == 0 || i + 1 == local_bytes.len() || local_bytes[i - 1] == b'.' {
                    return Err(LocalDot { at: at_pos(i) });
                }
            } else if !atext(b) { return Err(LocalChar { at: at_pos(i) }); }
            local_map.push(at_pos(i));
        }
        (String::from_utf8(local_bytes.to_vec()).unwrap(), false)
    };
    (local, quoted)
    };
    let domain_start = sep + 1;
    let mut domain_end = bytes.len();
    if domain_end > domain_start && bytes[domain_end - 1] == b'.' { domain_end -= 1; }
    if domain_start == domain_end { return Err(EmptyDomain { at: at_pos(domain_start) }); }
    if domain_end - domain_start > 253 {
        return Err(DomainTooLong { at: at_pos(domain_start + 253) });
    }
    let mut start = domain_start;
    let mut labels = 0;
    let mut last_start = start;
    loop {
        let end = (start..domain_end).find(|&i| bytes[i] == b'.').unwrap_or(domain_end);
        if end == start { return Err(EmptyLabel { at: at_pos(start) }); }
        if end - start > 63 { return Err(LabelTooLong { at: at_pos(start + 63) }); }
        if bytes[start] == b'-' { return Err(LabelHyphen { at: at_pos(start) }); }
        if bytes[end - 1] == b'-' { return Err(LabelHyphen { at: at_pos(end - 1) }); }
        for i in start..end {
            if !bytes[i].is_ascii_alphanumeric() && bytes[i] != b'-' {
                return Err(DomainChar { at: at_pos(i) });
            }
        }
        labels += 1;
        last_start = start;
        if end == domain_end { break; }
        start = end + 1;
    }
    if labels < 2 && !allow_single { return Err(DomainNeedsDot { at: at_pos(domain_start) }); }
    if bytes[last_start..domain_end].iter().all(|b| b.is_ascii_digit()) {
        return Err(NumericTld { at: at_pos(last_start) });
    }
    let domain = String::from_utf8(bytes[domain_start..domain_end].to_vec()).unwrap().to_ascii_lowercase();
    Ok((Address { local, domain, quoted }, local_map))
}

pub fn parse_email(input: &str) -> Result<Address, EmailError> {
    parse_core(input, false).map(|(address, _)| address)
}

pub fn normalize_email(input: &str, policy: &Policy) -> Result<Address, EmailError> {
    let (mut address, local_map) = parse_core(input, policy.allow_single_label)?;
    if policy.local_case == LocalCase::Lower { address.local.make_ascii_lowercase(); }
    if policy.strip_subaddress {
        if let Some(plus) = address.local.find('+') {
            if plus == 0 { return Err(EmailError::EmptyNormalizedLocal { at: local_map[plus] }); }
            address.local.truncate(plus);
        }
    }
    let safe = atom_ok(address.local.as_bytes());
    address.quoted = !safe || (address.quoted && !policy.unquote_safe);
    Ok(address)
}

pub fn format_email(address: &Address) -> String {
    let mut output = String::new();
    if address.quoted { output.push('"'); }
    for c in address.local.chars() {
        if address.quoted && matches!(c, '"' | '\\') { output.push('\\'); }
        output.push(c);
    }
    if address.quoted { output.push('"'); }
    output.push('@');
    output.push_str(&address.domain);
    output
}

pub fn deduplicate(inputs: &[&str], policy: &Policy) -> DedupResult {
    let mut result = DedupResult { addresses: Vec::new(), source_indices: Vec::new(),
        duplicate_of: vec![None; inputs.len()], errors: Vec::new() };
    let mut seen: std::collections::HashMap<(String, String), usize> = std::collections::HashMap::new();
    for (i, input) in inputs.iter().enumerate() {
        match normalize_email(input, policy) {
            Err(error) => result.errors.push((i, error)),
            Ok(address) => {
                let key = (address.local.clone(), address.domain.clone());
                if let Some(&first) = seen.get(&key) { result.duplicate_of[i] = Some(first); }
                else {
                    seen.insert(key, result.addresses.len());
                    result.addresses.push(address);
                    result.source_indices.push(i);
                }
            }
        }
    }
    result
}
