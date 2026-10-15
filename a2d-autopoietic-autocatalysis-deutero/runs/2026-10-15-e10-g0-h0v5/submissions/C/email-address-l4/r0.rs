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
struct Run {
    start: usize,
    end: usize,
}

fn is_hws(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

fn fold_end(bytes: &[u8], i: usize) -> Option<usize> {
    if i + 2 >= bytes.len()
        || bytes[i] != b'\r'
        || bytes[i + 1] != b'\n'
        || !is_hws(bytes[i + 2])
    {
        return None;
    }
    let mut j = i + 2;
    while j < bytes.len() && is_hws(bytes[j]) {
        j += 1;
    }
    Some(j)
}

fn append_run(runs: &mut Vec<Run>, start: usize, end: usize) {
    if let Some(last) = runs.last_mut() {
        if last.end == start {
            last.end = end;
            return;
        }
    }
    runs.push(Run { start, end });
}

fn cleanup(input: &[u8]) -> Result<(Vec<u8>, Vec<usize>, Vec<Run>), EmailError> {
    let mut out = Vec::new();
    let mut map = Vec::new();
    let mut runs: Vec<Run> = Vec::new();
    let mut i = 0;
    let mut quote_protection = false;
    let mut quote_escaped = false;
    let mut quote_closed = false;

    while i < input.len() {
        let b = input[i];

        if quote_protection {
            out.push(b);
            map.push(i);
            if quote_escaped {
                quote_escaped = false;
            } else if b == b'\\' {
                quote_escaped = true;
            } else if b == b'"' {
                quote_protection = false;
                quote_closed = true;
            }
            i += 1;
            continue;
        }

        if !quote_closed && out.is_empty() && b == b'"' {
            quote_protection = true;
            quote_escaped = false;
            out.push(b);
            map.push(i);
            i += 1;
            continue;
        }

        if b == b'\r' || b == b'\n' {
            let end = fold_end(input, i).ok_or(EmailError::BadFold { at: i })?;
            append_run(&mut runs, i, end);
            i = end;
            continue;
        }

        if is_hws(b) {
            let start = i;
            while i < input.len() && is_hws(input[i]) {
                i += 1;
            }
            append_run(&mut runs, start, i);
            continue;
        }

        if b == b'(' {
            let start = i;
            let outer = i;
            let mut stack = vec![i];
            i += 1;

            while i < input.len() && !stack.is_empty() {
                let c = input[i];
                if c == b'\\' {
                    if i + 1 >= input.len() || !(0x20..=0x7e).contains(&input[i + 1]) {
                        return Err(EmailError::CommentEscape { at: i });
                    }
                    i += 2;
                } else if c == b'(' {
                    if stack.len() == 3 {
                        return Err(EmailError::CommentDepth { at: i });
                    }
                    stack.push(i);
                    i += 1;
                } else if c == b')' {
                    stack.pop();
                    i += 1;
                } else if c == b'\r' || c == b'\n' {
                    i = fold_end(input, i).ok_or(EmailError::BadFold { at: i })?;
                } else if c == b'\t' || (0x20..=0x7e).contains(&c) {
                    i += 1;
                } else {
                    return Err(EmailError::CommentChar { at: i });
                }
            }

            if !stack.is_empty() {
                return Err(EmailError::UnclosedComment { at: outer });
            }
            append_run(&mut runs, start, i);
            continue;
        }

        if b == b')' {
            return Err(EmailError::UnexpectedClose { at: i });
        }

        out.push(b);
        map.push(i);
        i += 1;
    }

    for run in &runs {
        let before_idx = map.iter().rposition(|&p| p < run.start);
        let after_idx = map.iter().position(|&p| p >= run.end);
        let at_left = before_idx.map(|idx| out[idx] == b'@').unwrap_or(false);
        let at_right = after_idx.map(|idx| out[idx] == b'@').unwrap_or(false);
        let at_start = before_idx.is_none();
        let at_end = after_idx.is_none();

        if !(at_start || at_end || at_left || at_right) {
            return Err(EmailError::CfwsBoundary { at: run.start });
        }
    }

    Ok((out, map, runs))
}

fn original(map: &[usize], idx: usize, input_len: usize) -> usize {
    map.get(idx).copied().unwrap_or(input_len)
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
        } else if b.is_ascii_alphanumeric() || b"!#$%&'*+-/=?^_`{|}~".contains(&b) {
            previous_dot = false;
        } else {
            return false;
        }
    }
    true
}

fn parse_impl(input: &str, policy: Option<&Policy>) -> Result<Address, EmailError> {
    let raw = input.as_bytes();
    if raw.is_empty() {
        return Err(EmailError::EmptyInput);
    }
    if let Some(at) = raw.iter().position(|&b| b >= 128) {
        return Err(EmailError::NonAscii { at });
    }

    let (bytes, map, _) = cleanup(raw)?;
    if bytes.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let mut quote_end = None;
    if bytes[0] == b'"' {
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
                at: original(&map, 0, raw.len()),
            });
        }
    }

    let search_from = quote_end.map(|q| q + 1).unwrap_or(0);
    let ats: Vec<usize> = (search_from..bytes.len())
        .filter(|&i| bytes[i] == b'@')
        .collect();

    if ats.is_empty() {
        return Err(EmailError::MissingAt);
    }
    if ats.len() > 1 {
        return Err(EmailError::MultipleAt {
            at: original(&map, ats[1], raw.len()),
        });
    }

    let at = ats[0];
    if at == 0 {
        return Err(EmailError::EmptyLocal {
            at: original(&map, at, raw.len()),
        });
    }

    let local_bytes = &bytes[..at];
    let mut local = Vec::new();
    let mut origins = Vec::new();
    let quoted;

    if local_bytes[0] == b'"' {
        let end_quote = quote_end.unwrap();
        if end_quote + 1 != at {
            return Err(EmailError::TrailingLocal {
                at: original(&map, end_quote + 1, raw.len()),
            });
        }

        quoted = true;
        let mut i = 1;
        while i < end_quote {
            let b = local_bytes[i];
            if b == b'\\' {
                if i + 1 >= end_quote
                    || !matches!(local_bytes[i + 1], b'"' | b'\\' | b'@')
                {
                    return Err(EmailError::BadEscape {
                        at: original(&map, i, raw.len()),
                    });
                }
                local.push(local_bytes[i + 1]);
                origins.push(original(&map, i, raw.len()));
                i += 2;
            } else {
                if !(0x20..=0x7e).contains(&b) {
                    return Err(EmailError::QuotedChar {
                        at: original(&map, i, raw.len()),
                    });
                }
                local.push(b);
                origins.push(original(&map, i, raw.len()));
                i += 1;
            }
        }

        if local.is_empty() {
            return Err(EmailError::EmptyLocal {
                at: original(&map, end_quote, raw.len()),
            });
        }
        if local.len() > 64 {
            return Err(EmailError::LocalTooLong { at: origins[64] });
        }
    } else {
        quoted = false;
        if local_bytes.len() > 64 {
            return Err(EmailError::LocalTooLong {
                at: original(&map, 64, raw.len()),
            });
        }

        let mut previous_dot = false;
        for (i, &b) in local_bytes.iter().enumerate() {
            if b == b'.' {
                if i == 0 || previous_dot || i + 1 == local_bytes.len() {
                    return Err(EmailError::LocalDot {
                        at: original(&map, i, raw.len()),
                    });
                }
                previous_dot = true;
            } else {
                previous_dot = false;
                if !(b.is_ascii_alphanumeric() || b"!#$%&'*+-/=?^_`{|}~".contains(&b)) {
                    return Err(EmailError::LocalChar {
                        at: original(&map, i, raw.len()),
                    });
                }
            }
            local.push(b);
            origins.push(original(&map, i, raw.len()));
        }
    }

    let domain_start = at + 1;
    let mut domain_end = bytes.len();
    let mut root_dot_pos = None;
    if domain_end > domain_start && bytes[domain_end - 1] == b'.' {
        root_dot_pos = Some(original(&map, domain_end - 1, raw.len()));
        domain_end -= 1;
    }

    if domain_end == domain_start {
        return Err(EmailError::EmptyDomain {
            at: original(&map, domain_start, raw.len()),
        });
    }
    if domain_end - domain_start > 253 {
        return Err(EmailError::DomainTooLong {
            at: original(&map, domain_start + 253, raw.len()),
        });
    }

    let domain = &bytes[domain_start..domain_end];
    let mut labels = Vec::new();
    let mut start = 0;

    for i in 0..=domain.len() {
        if i == domain.len() || domain[i] == b'.' {
            let end = i;
            if start == end {
                let pos = if start == domain.len() {
                    root_dot_pos.unwrap_or_else(|| {
                        original(&map, domain_start + start, raw.len())
                    })
                } else {
                    original(&map, domain_start + start, raw.len())
                };
                return Err(EmailError::EmptyLabel { at: pos });
            }

            let len = end - start;
            if len > 63 {
                return Err(EmailError::LabelTooLong {
                    at: original(&map, domain_start + start + 63, raw.len()),
                });
            }
            if domain[start] == b'-' {
                return Err(EmailError::LabelHyphen {
                    at: original(&map, domain_start + start, raw.len()),
                });
            }
            if domain[end - 1] == b'-' {
                return Err(EmailError::LabelHyphen {
                    at: original(&map, domain_start + end - 1, raw.len()),
                });
            }
            for j in start..end {
                let b = domain[j];
                if !(b.is_ascii_alphanumeric() || b == b'-') {
                    return Err(EmailError::DomainChar {
                        at: original(&map, domain_start + j, raw.len()),
                    });
                }
            }
            labels.push((start, end));
            start = i + 1;
        }
    }

    if labels.len() < 2 && !policy.map(|p| p.allow_single_label).unwrap_or(false) {
        return Err(EmailError::DomainNeedsDot {
            at: original(&map, domain_start, raw.len()),
        });
    }

    let &(last_start, last_end) = labels.last().unwrap();
    if domain[last_start..last_end]
        .iter()
        .all(|b| b.is_ascii_digit())
    {
        return Err(EmailError::NumericTld {
            at: original(&map, domain_start + last_start, raw.len()),
        });
    }

    if let Some(p) = policy {
        if p.local_case == LocalCase::Lower {
            for b in &mut local {
                b.make_ascii_lowercase();
            }
        }

        if p.strip_subaddress {
            if let Some(pos) = local.iter().position(|&b| b == b'+') {
                if pos == 0 {
                    return Err(EmailError::EmptyNormalizedLocal {
                        at: origins[pos],
                    });
                }
                local.truncate(pos);
                origins.truncate(pos);
            }
        }

        let safe = valid_dot_atom(&local);
        let result_quoted = if !safe {
            true
        } else if p.unquote_safe {
            false
        } else {
            quoted
        };

        return Ok(Address {
            local: String::from_utf8(local).unwrap(),
            domain: domain
                .iter()
                .map(|&b| (b as char).to_ascii_lowercase())
                .collect(),
            quoted: result_quoted,
        });
    }

    Ok(Address {
        local: String::from_utf8(local).unwrap(),
        domain: domain
            .iter()
            .map(|&b| (b as char).to_ascii_lowercase())
            .collect(),
        quoted,
    })
}

pub fn parse_email(input: &str) -> Result<Address, EmailError> {
    parse_impl(input, None)
}

pub fn normalize_email(input: &str, policy: &Policy) -> Result<Address, EmailError> {
    parse_impl(input, Some(policy))
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
    let mut addresses = Vec::new();
    let mut source_indices = Vec::new();
    let mut duplicate_of = vec![None; inputs.len()];
    let mut errors = Vec::new();
    let mut keys: Vec<(String, String, usize)> = Vec::new();

    for (idx, input) in inputs.iter().enumerate() {
        match normalize_email(input, policy) {
            Err(error) => errors.push((idx, error)),
            Ok(address) => {
                if let Some((_, _, first)) = keys
                    .iter()
                    .find(|(local, domain, _)| {
                        local == &address.local && domain == &address.domain
                    })
                {
                    duplicate_of[idx] = Some(*first);
                } else {
                    keys.push((address.local.clone(), address.domain.clone(), idx));
                    source_indices.push(idx);
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