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

fn is_hws(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

fn is_atext(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$%&'*+-/=?^_`{|}~".contains(&b)
}

fn is_atom(local: &[u8]) -> bool {
    if local.is_empty() || local.len() > 64 {
        return false;
    }
    let mut previous_dot = true;
    for (i, &b) in local.iter().enumerate() {
        if b == b'.' {
            if previous_dot || i + 1 == local.len() {
                return false;
            }
            previous_dot = true;
        } else if is_atext(b) {
            previous_dot = false;
        } else {
            return false;
        }
    }
    true
}

fn cleanup(input: &[u8]) -> Result<(Vec<u8>, Vec<usize>, Vec<CfwsRun>), EmailError> {
    let mut kept = Vec::with_capacity(input.len());
    let mut positions = Vec::with_capacity(input.len());
    let mut runs = Vec::new();
    let mut i = 0;
    let mut first_retained = true;
    let mut in_quote = false;

    while i < input.len() {
        let b = input[i];

        if in_quote {
            kept.push(b);
            positions.push(i);
            if b == b'\\' {
                if i + 1 < input.len() {
                    kept.push(input[i + 1]);
                    positions.push(i + 1);
                    i += 2;
                    continue;
                }
            } else if b == b'"' {
                in_quote = false;
            }
            i += 1;
            continue;
        }

        if first_retained && b == b'"' {
            in_quote = true;
            kept.push(b);
            positions.push(i);
            first_retained = false;
            i += 1;
            continue;
        }

        let run_start = i;
        let mut consumed = false;
        if is_hws(b) {
            i += 1;
            while i < input.len() && is_hws(input[i]) {
                i += 1;
            }
            consumed = true;
        } else if b == b'\r' || b == b'\n' {
            if b == b'\r'
                && i + 2 < input.len()
                && input[i + 1] == b'\n'
                && is_hws(input[i + 2])
            {
                i += 3;
                while i < input.len() && is_hws(input[i]) {
                    i += 1;
                }
                consumed = true;
            } else {
                return Err(EmailError::BadFold { at: i });
            }
        } else if b == b'(' {
            let outer = i;
            let mut opens = [0usize; 3];
            opens[0] = i;
            let mut depth = 1usize;
            i += 1;
            while i < input.len() {
                let c = input[i];
                if c == b'\\' {
                    if i + 1 >= input.len()
                        || !(0x20..=0x7e).contains(&input[i + 1])
                    {
                        return Err(EmailError::CommentEscape { at: i });
                    }
                    i += 2;
                    continue;
                }
                if c == b'\r' || c == b'\n' {
                    if c == b'\r'
                        && i + 2 < input.len()
                        && input[i + 1] == b'\n'
                        && is_hws(input[i + 2])
                    {
                        i += 3;
                        while i < input.len() && is_hws(input[i]) {
                            i += 1;
                        }
                        continue;
                    }
                    return Err(EmailError::BadFold { at: i });
                }
                if c == b'(' {
                    if depth == 3 {
                        return Err(EmailError::CommentDepth { at: i });
                    }
                    opens[depth] = i;
                    depth += 1;
                    i += 1;
                    continue;
                }
                if c == b')' {
                    depth -= 1;
                    i += 1;
                    if depth == 0 {
                        break;
                    }
                    continue;
                }
                if !(0x20..=0x7e).contains(&c) && c != b'\t' {
                    return Err(EmailError::CommentChar { at: i });
                }
                i += 1;
            }
            if depth != 0 {
                return Err(EmailError::UnclosedComment { at: outer });
            }
            consumed = true;
        } else if b == b')' {
            return Err(EmailError::UnexpectedClose { at: i });
        }

        if consumed {
            runs.push(CfwsRun {
                start: run_start,
                end: i,
            });
        } else {
            kept.push(b);
            positions.push(i);
            first_retained = false;
            i += 1;
        }
    }

    Ok((kept, positions, runs))
}

fn mapped(positions: &[usize], index: usize, input_len: usize) -> usize {
    positions.get(index).copied().unwrap_or(input_len)
}

fn parse_cleaned(
    bytes: &[u8],
    positions: &[usize],
    input_len: usize,
    allow_single_label: bool,
) -> Result<(Address, Vec<usize>), EmailError> {
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
                at: mapped(positions, 0, input_len),
            });
        }
    }

    let mut ats = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if let Some(end) = quote_end {
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
    if ats.len() > 1 {
        return Err(EmailError::MultipleAt {
            at: mapped(positions, ats[1], input_len),
        });
    }

    let at = ats[0];
    if at == 0 {
        return Err(EmailError::EmptyLocal {
            at: mapped(positions, at, input_len),
        });
    }

    let local_bytes = &bytes[..at];
    let mut local = Vec::new();
    let mut local_origins = Vec::new();
    let quoted;

    if local_bytes[0] == b'"' {
        quoted = true;
        let end = quote_end.unwrap();
        if end + 1 != at {
            return Err(EmailError::TrailingLocal {
                at: mapped(positions, end + 1, input_len),
            });
        }

        let mut j = 1;
        while j < end {
            let b = local_bytes[j];
            if b == b'\\' {
                if j + 1 >= end {
                    return Err(EmailError::BadEscape {
                        at: mapped(positions, j, input_len),
                    });
                }
                let next = local_bytes[j + 1];
                if next != b'"' && next != b'\\' && next != b'@' {
                    return Err(EmailError::BadEscape {
                        at: mapped(positions, j, input_len),
                    });
                }
                local.push(next);
                local_origins.push(mapped(positions, j, input_len));
                j += 2;
            } else {
                if !(0x20..=0x7e).contains(&b) {
                    return Err(EmailError::QuotedChar {
                        at: mapped(positions, j, input_len),
                    });
                }
                local.push(b);
                local_origins.push(mapped(positions, j, input_len));
                j += 1;
            }
        }
        if local.is_empty() {
            return Err(EmailError::EmptyLocal {
                at: mapped(positions, end, input_len),
            });
        }
        if local.len() > 64 {
            return Err(EmailError::LocalTooLong {
                at: local_origins[64],
            });
        }
    } else {
        quoted = false;
        if local_bytes.len() > 64 {
            return Err(EmailError::LocalTooLong {
                at: mapped(positions, 64, input_len),
            });
        }
        let mut previous_dot = false;
        for (j, &b) in local_bytes.iter().enumerate() {
            if b == b'.' {
                if j == 0 || j + 1 == local_bytes.len() || previous_dot {
                    return Err(EmailError::LocalDot {
                        at: mapped(positions, j, input_len),
                    });
                }
                previous_dot = true;
            } else if is_atext(b) {
                previous_dot = false;
            } else {
                return Err(EmailError::LocalChar {
                    at: mapped(positions, j, input_len),
                });
            }
            local.push(b);
            local_origins.push(mapped(positions, j, input_len));
        }
    }

    let domain_start = at + 1;
    let mut domain_end = bytes.len();
    let mut root_pos = None;
    if domain_end > domain_start && bytes[domain_end - 1] == b'.' {
        domain_end -= 1;
        root_pos = Some(mapped(positions, domain_end, input_len));
    }
    if domain_start == domain_end {
        return Err(EmailError::EmptyDomain {
            at: mapped(positions, domain_start, input_len),
        });
    }
    let domain_bytes = &bytes[domain_start..domain_end];
    if domain_bytes.len() > 253 {
        return Err(EmailError::DomainTooLong {
            at: mapped(positions, domain_start + 253, input_len),
        });
    }

    let mut labels: Vec<(usize, usize)> = Vec::new();
    let mut label_start = domain_start;
    let mut j = domain_start;
    while j <= domain_end {
        if j == domain_end || bytes[j] == b'.' {
            labels.push((label_start, j));
            label_start = j + 1;
        }
        j += 1;
    }

    for (idx, &(start, end)) in labels.iter().enumerate() {
        if start == end {
            let pos = if idx + 1 == labels.len() {
                root_pos.unwrap_or_else(|| mapped(positions, start, input_len))
            } else {
                mapped(positions, start, input_len)
            };
            return Err(EmailError::EmptyLabel { at: pos });
        }
        let len = end - start;
        if len > 63 {
            return Err(EmailError::LabelTooLong {
                at: mapped(positions, start + 63, input_len),
            });
        }
        if bytes[start] == b'-' {
            return Err(EmailError::LabelHyphen {
                at: mapped(positions, start, input_len),
            });
        }
        if bytes[end - 1] == b'-' {
            return Err(EmailError::LabelHyphen {
                at: mapped(positions, end - 1, input_len),
            });
        }
        for k in start..end {
            let b = bytes[k];
            if !b.is_ascii_alphanumeric() && b != b'-' {
                return Err(EmailError::DomainChar {
                    at: mapped(positions, k, input_len),
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
    if bytes[last_start..last_end].iter().all(u8::is_ascii_digit) {
        return Err(EmailError::NumericTld {
            at: mapped(positions, last_start, input_len),
        });
    }

    let local_string = String::from_utf8(local).expect("ASCII local");
    let domain_string = String::from_utf8(domain_bytes.to_vec())
        .expect("ASCII domain")
        .to_ascii_lowercase();

    Ok((
        Address {
            local: local_string,
            domain: domain_string,
            quoted,
        },
        local_origins,
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

    let (cleaned, positions, runs) = cleanup(raw)?;
    for run in runs {
        let left_at = cleaned.last().copied();
        let _ = left_at;
        let start_idx = positions.partition_point(|&p| p < run.start);
        let end_idx = positions.partition_point(|&p| p < run.end);
        let left_is_at = start_idx > 0 && cleaned[start_idx - 1] == b'@';
        let right_is_at = end_idx < cleaned.len() && cleaned[end_idx] == b'@';
        if start_idx != 0
            && end_idx != cleaned.len()
            && !left_is_at
            && !right_is_at
        {
            return Err(EmailError::CfwsBoundary { at: run.start });
        }
    }

    if cleaned.is_empty() {
        return Err(EmailError::EmptyInput);
    }
    parse_cleaned(&cleaned, &positions, input.len(), false).map(|v| v.0)
}

pub fn normalize_email(input: &str, policy: &Policy) -> Result<Address, EmailError> {
    if input.is_empty() {
        return Err(EmailError::EmptyInput);
    }
    let raw = input.as_bytes();
    if let Some(at) = raw.iter().position(|&b| b >= 128) {
        return Err(EmailError::NonAscii { at });
    }

    let (cleaned, positions, runs) = cleanup(raw)?;
    for run in runs {
        let start_idx = positions.partition_point(|&p| p < run.start);
        let end_idx = positions.partition_point(|&p| p < run.end);
        let left_is_at = start_idx > 0 && cleaned[start_idx - 1] == b'@';
        let right_is_at = end_idx < cleaned.len() && cleaned[end_idx] == b'@';
        if start_idx != 0
            && end_idx != cleaned.len()
            && !left_is_at
            && !right_is_at
        {
            return Err(EmailError::CfwsBoundary { at: run.start });
        }
    }
    if cleaned.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let (mut address, mut origins) =
        parse_cleaned(&cleaned, &positions, input.len(), policy.allow_single_label)?;

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
            origins.truncate(plus);
        }
    }

    if local.is_empty() {
        return Err(EmailError::EmptyNormalizedLocal {
            at: origins.first().copied().unwrap_or(input.len()),
        });
    }

    let safe = is_atom(&local);
    address.quoted = if safe {
        address.quoted && !policy.unquote_safe
    } else {
        true
    };
    address.local = String::from_utf8(local).expect("ASCII local");
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
    let mut keys: Vec<(String, String, usize)> = Vec::new();

    for (index, input) in inputs.iter().enumerate() {
        match normalize_email(input, policy) {
            Err(error) => result.errors.push((index, error)),
            Ok(address) => {
                if let Some((_, _, first_index)) = keys
                    .iter()
                    .find(|(local, domain, _)| {
                        local == &address.local && domain == &address.domain
                    })
                {
                    result.duplicate_of[index] = Some(*first_index);
                } else {
                    keys.push((
                        address.local.clone(),
                        address.domain.clone(),
                        index,
                    ));
                    result.source_indices.push(index);
                    result.addresses.push(address);
                }
            }
        }
    }

    result
}