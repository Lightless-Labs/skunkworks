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
struct Retained {
    byte: u8,
    pos: usize,
}

#[derive(Clone, Copy)]
struct CfwsRun {
    start: usize,
    end: usize,
}

fn is_hws(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

fn is_printable(b: u8) -> bool {
    (0x20..=0x7e).contains(&b)
}

fn fold_end(bytes: &[u8], i: usize) -> Option<usize> {
    if bytes.get(i) != Some(&b'\r') || bytes.get(i + 1) != Some(&b'\n') {
        return None;
    }
    let mut j = i + 2;
    if !bytes.get(j).copied().is_some_and(is_hws) {
        return None;
    }
    while bytes.get(j).copied().is_some_and(is_hws) {
        j += 1;
    }
    Some(j)
}

fn cleanup(input: &[u8]) -> Result<(Vec<Retained>, Vec<CfwsRun>), EmailError> {
    let mut retained = Vec::with_capacity(input.len());
    let mut runs = Vec::new();
    let mut i = 0;
    let mut in_quote = false;
    let mut quote_closed = false;
    let mut depth = 0usize;
    let mut outer_open = 0usize;
    let mut comment_stack: Vec<usize> = Vec::new();

    while i < input.len() {
        if in_quote {
            let b = input[i];
            retained.push(Retained { byte: b, pos: i });
            if b == b'\\' {
                i += 1;
                if i < input.len() {
                    retained.push(Retained {
                        byte: input[i],
                        pos: i,
                    });
                    i += 1;
                }
                continue;
            }
            if b == b'"' {
                in_quote = false;
                quote_closed = true;
            }
            i += 1;
            continue;
        }

        if depth > 0 {
            let b = input[i];
            if b == b'\\' {
                if i + 1 >= input.len() || !is_printable(input[i + 1]) {
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
                comment_stack.push(i);
                i += 1;
                continue;
            }
            if b == b')' {
                depth -= 1;
                comment_stack.pop();
                i += 1;
                if depth == 0 {
                    runs.push(CfwsRun {
                        start: outer_open,
                        end: i,
                    });
                }
                continue;
            }
            if !is_printable(b) && b != b'\t' {
                return Err(EmailError::CommentChar { at: i });
            }
            i += 1;
            continue;
        }

        let b = input[i];

        if !quote_closed && retained.is_empty() && b == b'"' {
            in_quote = true;
            retained.push(Retained { byte: b, pos: i });
            i += 1;
            continue;
        }

        if b == b' ' || b == b'\t' || b == b'\r' || b == b'\n' || b == b'(' {
            let run_start = i;
            loop {
                if i >= input.len() {
                    break;
                }
                let c = input[i];
                if is_hws(c) {
                    i += 1;
                    continue;
                }
                if c == b'\r' || c == b'\n' {
                    if let Some(end) = fold_end(input, i) {
                        i = end;
                        continue;
                    }
                    return Err(EmailError::BadFold { at: i });
                }
                if c == b'(' {
                    outer_open = i;
                    depth = 1;
                    comment_stack.push(i);
                    i += 1;
                    while i < input.len() && depth > 0 {
                        let d = input[i];
                        if d == b'\\' {
                            if i + 1 >= input.len() || !is_printable(input[i + 1]) {
                                return Err(EmailError::CommentEscape { at: i });
                            }
                            i += 2;
                        } else if d == b'\r' || d == b'\n' {
                            if let Some(end) = fold_end(input, i) {
                                i = end;
                            } else {
                                return Err(EmailError::BadFold { at: i });
                            }
                        } else if d == b'(' {
                            if depth == 3 {
                                return Err(EmailError::CommentDepth { at: i });
                            }
                            depth += 1;
                            comment_stack.push(i);
                            i += 1;
                        } else if d == b')' {
                            depth -= 1;
                            comment_stack.pop();
                            i += 1;
                        } else if !is_printable(d) && d != b'\t' {
                            return Err(EmailError::CommentChar { at: i });
                        } else {
                            i += 1;
                        }
                    }
                    if depth != 0 {
                        return Err(EmailError::UnclosedComment { at: outer_open });
                    }
                    continue;
                }
                break;
            }
            runs.push(CfwsRun {
                start: run_start,
                end: i,
            });
            continue;
        }

        if b == b')' {
            return Err(EmailError::UnexpectedClose { at: i });
        }

        retained.push(Retained { byte: b, pos: i });
        i += 1;
    }

    if depth != 0 {
        return Err(EmailError::UnclosedComment { at: outer_open });
    }

    let _ = quote_closed;
    Ok((retained, runs))
}

fn error_pos(items: &[Retained], index: usize, input_len: usize) -> usize {
    items.get(index).map_or(input_len, |x| x.pos)
}

fn is_atom_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$%&'*+-/=?^_`{|}~".contains(&b)
}

fn validate_atom(bytes: &[u8], positions: &[usize], fallback: usize) -> Result<(), EmailError> {
    if bytes.len() > 64 {
        return Err(EmailError::LocalTooLong {
            at: positions.get(64).copied().unwrap_or(fallback),
        });
    }
    for i in 0..bytes.len() {
        let b = bytes[i];
        if b == b'.' {
            if i == 0 || i + 1 == bytes.len() || bytes[i - 1] == b'.' {
                return Err(EmailError::LocalDot {
                    at: positions.get(i).copied().unwrap_or(fallback),
                });
            }
        } else if !is_atom_char(b) {
            return Err(EmailError::LocalChar {
                at: positions.get(i).copied().unwrap_or(fallback),
            });
        }
    }
    Ok(())
}

fn parse_core(input: &str, allow_single_label: bool) -> Result<(Address, Vec<usize>), EmailError> {
    let raw = input.as_bytes();
    if raw.is_empty() {
        return Err(EmailError::EmptyInput);
    }
    if let Some(i) = raw.iter().position(|&b| b >= 128) {
        return Err(EmailError::NonAscii { at: i });
    }

    let (items, runs) = cleanup(raw)?;
    let bytes: Vec<u8> = items.iter().map(|x| x.byte).collect();

    for run in &runs {
        let left = items.iter().position(|x| x.pos >= run.end);
        let right = items.iter().rposition(|x| x.pos < run.start);
        let at_left = left.is_some_and(|idx| bytes[idx] == b'@');
        let at_right = right.is_some_and(|idx| bytes[idx] == b'@');
        let at_boundary = run.start == 0 || run.end == raw.len();
        if !at_left && !at_right && !at_boundary {
            return Err(EmailError::CfwsBoundary { at: run.start });
        }
    }

    if items.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let mut quoted_end = None;
    if bytes[0] == b'"' {
        let mut i = 1;
        while i < bytes.len() {
            if bytes[i] == b'\\' {
                i += 2;
            } else if bytes[i] == b'"' {
                quoted_end = Some(i);
                break;
            } else {
                i += 1;
            }
        }
        if quoted_end.is_none() {
            return Err(EmailError::UnclosedQuote { at: items[0].pos });
        }
    }

    let mut ats = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if let Some(end) = quoted_end {
            if i <= end {
                i = end + 1;
                continue;
            }
        }
        if bytes[i] == b'@' {
            ats.push(i);
        }
        i += 1;
    }
    if ats.is_empty() {
        return Err(EmailError::MissingAt);
    }
    if ats.len() >= 2 {
        return Err(EmailError::MultipleAt {
            at: items[ats[1]].pos,
        });
    }
    let at_idx = ats[0];
    if at_idx == 0 {
        return Err(EmailError::EmptyLocal {
            at: items[at_idx].pos,
        });
    }

    let mut local = Vec::new();
    let mut local_positions = Vec::new();
    let quoted = bytes[0] == b'"';
    if quoted {
        let end = quoted_end.expect("quoted prefix has a closing quote");
        if end + 1 != at_idx {
            return Err(EmailError::TrailingLocal {
                at: error_pos(&items, end + 1, raw.len()),
            });
        }
        let mut j = 1;
        while j < end {
            if bytes[j] == b'\\' {
                if j + 1 >= end {
                    return Err(EmailError::BadEscape { at: items[j].pos });
                }
                let c = bytes[j + 1];
                if c != b'"' && c != b'\\' && c != b'@' {
                    return Err(EmailError::BadEscape { at: items[j].pos });
                }
                local.push(c);
                local_positions.push(items[j].pos);
                j += 2;
            } else {
                if !is_printable(bytes[j]) {
                    return Err(EmailError::QuotedChar { at: items[j].pos });
                }
                local.push(bytes[j]);
                local_positions.push(items[j].pos);
                j += 1;
            }
        }
        if local.is_empty() {
            return Err(EmailError::EmptyLocal {
                at: items[end].pos,
            });
        }
        if local.len() > 64 {
            return Err(EmailError::LocalTooLong {
                at: local_positions[64],
            });
        }
    } else {
        local.extend_from_slice(&bytes[..at_idx]);
        local_positions.extend(items[..at_idx].iter().map(|x| x.pos));
        validate_atom(&local, &local_positions, raw.len())?;
    }

    let domain_start_idx = at_idx + 1;
    let domain_start_pos = error_pos(&items, domain_start_idx, raw.len());
    let mut domain_end_idx = bytes.len();
    let had_root_dot = domain_end_idx > domain_start_idx && bytes[domain_end_idx - 1] == b'.';
    if had_root_dot {
        domain_end_idx -= 1;
    }
    if domain_start_idx == domain_end_idx {
        return Err(EmailError::EmptyDomain {
            at: if had_root_dot {
                items[domain_end_idx].pos
            } else {
                domain_start_pos
            },
        });
    }
    let domain_len = domain_end_idx - domain_start_idx;
    if domain_len > 253 {
        return Err(EmailError::DomainTooLong {
            at: items[domain_start_idx + 253].pos,
        });
    }

    let mut labels: Vec<(usize, usize)> = Vec::new();
    let mut start = domain_start_idx;
    for idx in domain_start_idx..domain_end_idx {
        if bytes[idx] == b'.' {
            labels.push((start, idx));
            start = idx + 1;
        }
    }
    labels.push((start, domain_end_idx));

    for &(s, e) in &labels {
        if s == e {
            let pos = if s == domain_start_idx {
                items[s].pos
            } else if e == domain_end_idx && had_root_dot {
                items[domain_end_idx].pos
            } else {
                items[s].pos
            };
            return Err(EmailError::EmptyLabel { at: pos });
        }
        if e - s > 63 {
            return Err(EmailError::LabelTooLong {
                at: items[s + 63].pos,
            });
        }
        if bytes[s] == b'-' {
            return Err(EmailError::LabelHyphen { at: items[s].pos });
        }
        if bytes[e - 1] == b'-' {
            return Err(EmailError::LabelHyphen {
                at: items[e - 1].pos,
            });
        }
        for idx in s..e {
            if !bytes[idx].is_ascii_alphanumeric() && bytes[idx] != b'-' {
                return Err(EmailError::DomainChar { at: items[idx].pos });
            }
        }
    }

    if labels.len() < 2 && !allow_single_label {
        return Err(EmailError::DomainNeedsDot {
            at: items[domain_start_idx].pos,
        });
    }
    let (last_start, last_end) = *labels.last().unwrap();
    if bytes[last_start..last_end].iter().all(u8::is_ascii_digit) {
        return Err(EmailError::NumericTld {
            at: items[last_start].pos,
        });
    }

    let mut domain = String::from_utf8(bytes[domain_start_idx..domain_end_idx].to_vec()).unwrap();
    domain.make_ascii_lowercase();
    let local_string = String::from_utf8(local).unwrap();
    Ok((
        Address {
            local: local_string,
            domain,
            quoted,
        },
        local_positions,
    ))
}

pub fn parse_email(input: &str) -> Result<Address, EmailError> {
    parse_core(input, false).map(|(address, _)| address)
}

fn is_valid_dot_atom(local: &str) -> bool {
    let bytes = local.as_bytes();
    if bytes.is_empty() || bytes.len() > 64 {
        return false;
    }
    for i in 0..bytes.len() {
        if bytes[i] == b'.' {
            if i == 0 || i + 1 == bytes.len() || bytes[i - 1] == b'.' {
                return false;
            }
        } else if !is_atom_char(bytes[i]) {
            return false;
        }
    }
    true
}

pub fn normalize_email(input: &str, policy: &Policy) -> Result<Address, EmailError> {
    let (mut address, positions) = parse_core(input, policy.allow_single_label)?;
    if policy.local_case == LocalCase::Lower {
        address.local.make_ascii_lowercase();
    }
    if policy.strip_subaddress {
        if let Some(i) = address.local.as_bytes().iter().position(|&b| b == b'+') {
            if i == 0 {
                return Err(EmailError::EmptyNormalizedLocal {
                    at: positions.first().copied().unwrap_or(input.len()),
                });
            }
            address.local.truncate(i);
        }
    }

    let safe = is_valid_dot_atom(&address.local);
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
    use std::collections::HashMap;

    let mut result = DedupResult {
        addresses: Vec::new(),
        source_indices: Vec::new(),
        duplicate_of: vec![None; inputs.len()],
        errors: Vec::new(),
    };
    let mut first_by_key: HashMap<(String, String), usize> = HashMap::new();

    for (index, input) in inputs.iter().enumerate() {
        match normalize_email(input, policy) {
            Err(error) => result.errors.push((index, error)),
            Ok(address) => {
                let key = (address.local.clone(), address.domain.clone());
                if let Some(&first_input) = first_by_key.get(&key) {
                    result.duplicate_of[index] = Some(first_input);
                } else {
                    first_by_key.insert(key, index);
                    result.source_indices.push(index);
                    result.addresses.push(address);
                }
            }
        }
    }

    result
}