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

struct Scan {
    kept: Vec<(u8, usize)>,
    runs: Vec<(usize, usize, usize)>,
}
fn hws(b: u8) -> bool { matches!(b, b' ' | b'\t') }
fn printable(b: u8) -> bool { (0x20..=0x7e).contains(&b) }
fn fold_at(s: &[u8], i: usize) -> bool {
    i + 2 < s.len() && s[i] == b'\r' && s[i + 1] == b'\n' && hws(s[i + 2])
}
fn push_run(runs: &mut Vec<(usize, usize, usize)>, start: usize, end: usize, boundary: usize, len: usize) {
    if end > start {
        if let Some(last) = runs.last_mut() {
            if last.1 == start && last.2 == boundary {
                last.1 = end;
                return;
            }
        }
        runs.push((start, end, boundary.min(len)));
    }
}
fn scan_cfws(s: &[u8]) -> Result<Scan, EmailError> {
    let mut kept = Vec::with_capacity(s.len());
    let mut runs = Vec::new();
    let mut i = 0;
    let mut protected = false;
    let mut closed = false;
    while i < s.len() {
        if protected && !closed {
            let b = s[i];
            kept.push((b, i));
            if b == b'\\' {
                i += 1;
                if i < s.len() { kept.push((s[i], i)); i += 1; }
            } else {
                if b == b'"' { closed = true; }
                i += 1;
            }
            continue;
        }
        let run_start_i = i;
        let run_start_k = kept.len();
        let mut had = false;
        loop {
            if i >= s.len() { break; }
            let b = s[i];
            if hws(b) {
                had = true;
                i += 1;
            } else if b == b'\r' || b == b'\n' {
                if !fold_at(s, i) { return Err(EmailError::BadFold { at: i }); }
                had = true;
                i += 2;
                while i < s.len() && hws(s[i]) { i += 1; }
            } else if b == b'(' {
                had = true;
                let outer = i;
                let mut depth = 1usize;
                i += 1;
                while i < s.len() && depth > 0 {
                    let c = s[i];
                    if c == b'\\' {
                        let at = i;
                        if i + 1 >= s.len() || !printable(s[i + 1]) {
                            return Err(EmailError::CommentEscape { at });
                        }
                        i += 2;
                    } else if c == b'\r' || c == b'\n' {
                        if !fold_at(s, i) { return Err(EmailError::BadFold { at: i }); }
                        i += 2;
                        while i < s.len() && hws(s[i]) { i += 1; }
                    } else if c == b'(' {
                        if depth == 3 { return Err(EmailError::CommentDepth { at: i }); }
                        depth += 1;
                        i += 1;
                    } else if c == b')' {
                        depth -= 1;
                        i += 1;
                    } else if !printable(c) && !hws(c) {
                        return Err(EmailError::CommentChar { at: i });
                    } else {
                        i += 1;
                    }
                }
                if depth != 0 { return Err(EmailError::UnclosedComment { at: outer }); }
            } else {
                break;
            }
        }
        if had {
            push_run(&mut runs, run_start_i, i, run_start_k, s.len());
        }
        if i < s.len() {
            if s[i] == b')' { return Err(EmailError::UnexpectedClose { at: i }); }
            let b = s[i];
            kept.push((b, i));
            if kept.len() == 1 && b == b'"' { protected = true; }
            i += 1;
        }
    }
    Ok(Scan { kept, runs })
}

fn parse_impl(input: &str, allow_single: bool) -> Result<(Address, Vec<usize>), EmailError> {
    let s = input.as_bytes();
    if s.is_empty() { return Err(EmailError::EmptyInput); }
    if let Some(i) = s.iter().position(|b| *b >= 128) { return Err(EmailError::NonAscii { at: i }); }
    let scan = scan_cfws(s)?;
    let bytes: Vec<u8> = scan.kept.iter().map(|x| x.0).collect();
    let pos: Vec<usize> = scan.kept.iter().map(|x| x.1).collect();

    for &(start, end, boundary) in &scan.runs {
        let left = boundary == 0 || bytes.get(boundary.wrapping_sub(1)) == Some(&b'@');
        let right = boundary == bytes.len() || bytes.get(boundary) == Some(&b'@');
        if !left && !right { return Err(EmailError::CfwsBoundary { at: start }); }
        let _ = end;
    }
    if bytes.is_empty() { return Err(EmailError::EmptyInput); }

    let mut qend = None;
    if bytes[0] == b'"' {
        let mut i = 1;
        while i < bytes.len() {
            if bytes[i] == b'\\' { i += 2; continue; }
            if bytes[i] == b'"' { qend = Some(i); break; }
            i += 1;
        }
        if qend.is_none() { return Err(EmailError::UnclosedQuote { at: pos[0] }); }
    }
    let mut ats = Vec::new();
    for i in 0..bytes.len() {
        if qend.map_or(false, |q| i <= q) { continue; }
        if bytes[i] == b'@' { ats.push(i); }
    }
    if ats.is_empty() { return Err(EmailError::MissingAt); }
    if ats.len() > 1 { return Err(EmailError::MultipleAt { at: pos[ats[1]] }); }
    let sep = ats[0];
    if sep == 0 { return Err(EmailError::EmptyLocal { at: pos[sep] }); }

    let mut origin = Vec::new();
    let (local, quoted) = if bytes[0] == b'"' {
        let q = qend.unwrap();
        if q + 1 != sep { return Err(EmailError::TrailingLocal { at: pos[q + 1] }); }
        let mut out = Vec::new();
        let mut origins = Vec::new();
        let mut j = 1;
        while j < q {
            if bytes[j] == b'\\' {
                if j + 1 >= q || !matches!(bytes[j + 1], b'"' | b'\\' | b'@') {
                    return Err(EmailError::BadEscape { at: pos[j] });
                }
                out.push(bytes[j + 1]);
                origins.push(pos[j]);
                j += 2;
            } else {
                if !printable(bytes[j]) { return Err(EmailError::QuotedChar { at: pos[j] }); }
                out.push(bytes[j]);
                origins.push(pos[j]);
                j += 1;
            }
        }
        if out.is_empty() { return Err(EmailError::EmptyLocal { at: pos[q] }); }
        if out.len() > 64 { return Err(EmailError::LocalTooLong { at: origins[64] }); }
        origin = origins;
        (out, true)
    } else {
        if sep > 64 { return Err(EmailError::LocalTooLong { at: pos[64] }); }
        let mut prev_dot = false;
        for j in 0..sep {
            let b = bytes[j];
            if b == b'.' {
                if j == 0 || j + 1 == sep || prev_dot { return Err(EmailError::LocalDot { at: pos[j] }); }
                prev_dot = true;
            } else {
                prev_dot = false;
                if !b.is_ascii_alphanumeric() && !b"!#$%&'*+-/=?^_`{|}~".contains(&b) {
                    return Err(EmailError::LocalChar { at: pos[j] });
                }
            }
            origin.push(pos[j]);
        }
        (bytes[..sep].to_vec(), false)
    };

    let ds = sep + 1;
    let mut de = bytes.len();
    let root = if de > ds && bytes[de - 1] == b'.' { de -= 1; Some(pos[de]) } else { None };
    if de == ds {
        return Err(EmailError::EmptyDomain { at: pos.get(ds).copied().unwrap_or(input.len()) });
    }
    if de - ds > 253 { return Err(EmailError::DomainTooLong { at: pos[ds + 253] }); }

    let mut starts = Vec::new();
    let mut ends = Vec::new();
    let mut st = ds;
    for j in ds..=de {
        if j == de || bytes[j] == b'.' {
            if st == j {
                let at = if j == de {
                    root.unwrap_or_else(|| pos.get(j).copied().unwrap_or(input.len()))
                } else { pos[j] };
                return Err(EmailError::EmptyLabel { at });
            }
            let n = j - st;
            if n > 63 { return Err(EmailError::LabelTooLong { at: pos[st + 63] }); }
            if bytes[st] == b'-' { return Err(EmailError::LabelHyphen { at: pos[st] }); }
            if bytes[j - 1] == b'-' { return Err(EmailError::LabelHyphen { at: pos[j - 1] }); }
            for k in st..j {
                if !bytes[k].is_ascii_alphanumeric() && bytes[k] != b'-' {
                    return Err(EmailError::DomainChar { at: pos[k] });
                }
            }
            starts.push(st);
            ends.push(j);
            st = j + 1;
        }
    }
    if starts.len() < 2 && !allow_single { return Err(EmailError::DomainNeedsDot { at: pos[ds] }); }
    let ls = starts[starts.len() - 1];
    let le = ends[ends.len() - 1];
    if bytes[ls..le].iter().all(u8::is_ascii_digit) { return Err(EmailError::NumericTld { at: pos[ls] }); }

    let mut domain = bytes[ds..de].to_vec();
    domain.make_ascii_lowercase();
    Ok((Address {
        local: String::from_utf8(local).unwrap(),
        domain: String::from_utf8(domain).unwrap(),
        quoted,
    }, origin))
}

pub fn parse_email(input: &str) -> Result<Address, EmailError> {
    parse_impl(input, false).map(|v| v.0)
}

fn dot_atom(s: &[u8]) -> bool {
    if s.is_empty() || s.len() > 64 { return false; }
    let mut prev = false;
    for (i, &b) in s.iter().enumerate() {
        if b == b'.' {
            if i == 0 || i + 1 == s.len() || prev { return false; }
            prev = true;
        } else if b.is_ascii_alphanumeric() || b"!#$%&'*+-/=?^_`{|}~".contains(&b) {
            prev = false;
        } else {
            return false;
        }
    }
    true
}

pub fn normalize_email(input: &str, policy: &Policy) -> Result<Address, EmailError> {
    let (mut a, mut origins) = parse_impl(input, policy.allow_single_label)?;
    let mut local = a.local.into_bytes();
    if policy.local_case == LocalCase::Lower { local.make_ascii_lowercase(); }
    if policy.strip_subaddress {
        if let Some(p) = local.iter().position(|b| *b == b'+') {
            if p == 0 {
                return Err(EmailError::EmptyNormalizedLocal {
                    at: origins.first().copied().unwrap_or(input.len()),
                });
            }
            local.truncate(p);
            origins.truncate(p);
        }
    }
    let safe = dot_atom(&local);
    a.local = String::from_utf8(local).unwrap();
    if !safe { a.quoted = true; }
    else if policy.unquote_safe { a.quoted = false; }
    Ok(a)
}

pub fn format_email(address: &Address) -> String {
    let mut out = String::new();
    if address.quoted {
        out.push('"');
        for c in address.local.chars() {
            if c == '"' || c == '\\' { out.push('\\'); }
            out.push(c);
        }
        out.push('"');
    } else { out.push_str(&address.local); }
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
    for (i, s) in inputs.iter().enumerate() {
        match normalize_email(s, policy) {
            Err(e) => errors.push((i, e)),
            Ok(a) => {
                let key = (a.local.clone(), a.domain.clone());
                if let Some(&first) = seen.get(&key) {
                    duplicate_of[i] = Some(first);
                } else {
                    seen.insert(key, i);
                    source_indices.push(i);
                    addresses.push(a);
                }
            }
        }
    }
    DedupResult { addresses, source_indices, duplicate_of, errors }
}
