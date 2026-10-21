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
}

#[derive(Clone, Copy)]
struct Run {
    start: usize,
    end: usize,
}

fn is_hws(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

fn is_fold(bytes: &[u8], i: usize) -> Option<usize> {
    if bytes.get(i) != Some(&b'\r') || bytes.get(i + 1) != Some(&b'\n') {
        return None;
    }
    let mut j = i + 2;
    if !bytes.get(j).copied().map(is_hws).unwrap_or(false) {
        return None;
    }
    while bytes.get(j).copied().map(is_hws).unwrap_or(false) {
        j += 1;
    }
    Some(j)
}

fn cleanup(input: &[u8]) -> Result<(Vec<u8>, Vec<usize>, Vec<Run>), EmailError> {
    let mut clean = Vec::with_capacity(input.len());
    let mut positions = Vec::with_capacity(input.len());
    let mut runs = Vec::new();
    let mut i = 0;
    let mut quote_protected = false;
    let mut quote_closed = false;

    while i < input.len() {
        if quote_protected && !quote_closed {
            let b = input[i];
            clean.push(b);
            positions.push(i);
            i += 1;
            if b == b'\\' {
                if i < input.len() {
                    clean.push(input[i]);
                    positions.push(i);
                    i += 1;
                }
            } else if b == b'"' {
                quote_closed = true;
            }
            continue;
        }

        if !quote_protected && clean.is_empty() && input[i] == b'"' {
            quote_protected = true;
            clean.push(input[i]);
            positions.push(i);
            i += 1;
            continue;
        }

        let b = input[i];
        if b == b'(' {
            let run_start = i;
            let outer = i;
            let mut depth = 0usize;
            while i < input.len() {
                let c = input[i];
                if c == b'\\' {
                    let slash = i;
                    if i + 1 >= input.len()
                        || !(0x20..=0x7e).contains(&input[i + 1])
                    {
                        return Err(EmailError::CommentEscape { at: slash });
                    }
                    i += 2;
                    continue;
                }
                if c == b'\r' || c == b'\n' {
                    if let Some(end) = is_fold(input, i) {
                        i = end;
                        continue;
                    }
                    return Err(EmailError::BadFold { at: i });
                }
                if c == b'(' {
                    depth += 1;
                    if depth > 3 {
                        return Err(EmailError::CommentDepth { at: i });
                    }
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
                if c == b'\t' || (0x20..=0x7e).contains(&c) {
                    i += 1;
                    continue;
                }
                return Err(EmailError::CommentChar { at: i });
            }
            if depth != 0 {
                return Err(EmailError::UnclosedComment { at: outer });
            }
            while i < input.len() {
                let c = input[i];
                if is_hws(c) {
                    i += 1;
                    continue;
                }
                if let Some(end) = is_fold(input, i) {
                    i = end;
                    continue;
                }
                if c == b'(' {
                    // Adjacent comments are part of the same CFWS run.
                    // Scan them in the outer loop, preserving one run.
                    break;
                }
                break;
            }
            runs.push(Run { start: run_start, end: i });
            continue;
        }

        if b == b')' {
            return Err(EmailError::UnexpectedClose { at: i });
        }
        if b == b'\r' || b == b'\n' {
            let Some(end) = is_fold(input, i) else {
                return Err(EmailError::BadFold { at: i });
            };
            let start = i;
            i = end;
            while i < input.len() {
                if is_hws(input[i]) {
                    i += 1;
                } else if let Some(end) = is_fold(input, i) {
                    i = end;
                } else if input[i] == b'(' {
                    // The following comment is absorbed on the next pass.
                    break;
                } else {
                    break;
                }
            }
            runs.push(Run { start, end: i });
            continue;
        }
        if is_hws(b) {
            let start = i;
            while i < input.len() {
                if is_hws(input[i]) {
                    i += 1;
                } else if let Some(end) = is_fold(input, i) {
                    i = end;
                } else {
                    break;
                }
            }
            runs.push(Run { start, end: i });
            continue;
        }

        clean.push(b);
        positions.push(i);
        i += 1;

        if quote_protected && quote_closed {
            // Protection is limited to the initial quoted local.
            quote_protected = false;
        }
    }

    // Merge adjacent CFWS constructs into maximal runs.
    runs.sort_by_key(|r| r.start);
    let mut merged: Vec<Run> = Vec::new();
    for run in runs {
        if let Some(last) = merged.last_mut() {
            if run.start <= last.end {
                last.end = last.end.max(run.end);
                continue;
            }
        }
        merged.push(run);
    }
    Ok((clean, positions, merged))
}

fn original_pos(positions: &[usize], idx: usize, input_len: usize) -> usize {
    positions.get(idx).copied().unwrap_or(input_len)
}

fn is_local_atom(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$%&'*+-/=?^_`{|}~".contains(&b)
}

pub fn parse_email(input: &str) -> Result<Address, EmailError> {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return Err(EmailError::EmptyInput);
    }
    if let Some(i) = bytes.iter().position(|&b| b >= 128) {
        return Err(EmailError::NonAscii { at: i });
    }

    let (clean, positions, runs) = cleanup(bytes)?;
    for run in &runs {
        let left_edge = run.start == 0;
        let right_edge = run.end == bytes.len();
        let left_at = positions.last().copied().map(|p| p < run.start);
        let right_at = positions.iter().copied().find(|&p| p >= run.end);
        let legal_left = left_edge
            || left_at
                .map(|_| {
                    clean
                        .iter()
                        .enumerate()
                        .find(|(idx, _)| positions[*idx] >= run.start)
                        .and_then(|(idx, _)| idx.checked_sub(1).map(|j| clean[j]))
                        == Some(b'@')
                })
                .unwrap_or(false);
        let legal_right = right_edge
            || right_at
                .and_then(|p| positions.iter().position(|&x| x == p))
                .map(|idx| clean[idx] == b'@')
                .unwrap_or(false);
        if !legal_left && !legal_right {
            return Err(EmailError::CfwsBoundary { at: run.start });
        }
    }

    if clean.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let mut quoted_end = None;
    if clean[0] == b'"' {
        let mut i = 1;
        while i < clean.len() {
            if clean[i] == b'\\' {
                i += 2;
            } else if clean[i] == b'"' {
                quoted_end = Some(i);
                break;
            } else {
                i += 1;
            }
        }
        if quoted_end.is_none() {
            return Err(EmailError::UnclosedQuote {
                at: original_pos(&positions, 0, bytes.len()),
            });
        }
    }

    let mut ats = Vec::new();
    let mut i = 0;
    while i < clean.len() {
        if let Some(end) = quoted_end {
            if i <= end {
                i = end + 1;
                continue;
            }
        }
        if clean[i] == b'@' {
            ats.push(i);
        }
        i += 1;
    }
    if ats.is_empty() {
        return Err(EmailError::MissingAt);
    }
    if ats.len() > 1 {
        return Err(EmailError::MultipleAt {
            at: original_pos(&positions, ats[1], bytes.len()),
        });
    }

    let at_idx = ats[0];
    if at_idx == 0 {
        return Err(EmailError::EmptyLocal {
            at: original_pos(&positions, at_idx, bytes.len()),
        });
    }

    let mut local = String::new();
    let quoted = clean[0] == b'"';
    if !quoted {
        if at_idx > 64 {
            return Err(EmailError::LocalTooLong {
                at: original_pos(&positions, 64, bytes.len()),
            });
        }
        for i in 0..at_idx {
            let b = clean[i];
            if b == b'.' {
                if i == 0 || i + 1 == at_idx || clean[i - 1] == b'.' {
                    return Err(EmailError::LocalDot {
                        at: original_pos(&positions, i, bytes.len()),
                    });
                }
            } else if !is_local_atom(b) {
                return Err(EmailError::LocalChar {
                    at: original_pos(&positions, i, bytes.len()),
                });
            }
        }
        local.push_str(std::str::from_utf8(&clean[..at_idx]).unwrap());
    } else {
        let end = quoted_end.unwrap();
        if end + 1 != at_idx {
            return Err(EmailError::TrailingLocal {
                at: original_pos(&positions, end + 1, bytes.len()),
            });
        }
        let mut decoded = Vec::new();
        let mut origins = Vec::new();
        let mut i = 1;
        while i < end {
            let b = clean[i];
            if b == b'\\' {
                if i + 1 >= end || !matches!(clean[i + 1], b'"' | b'\\' | b'@') {
                    return Err(EmailError::BadEscape {
                        at: original_pos(&positions, i, bytes.len()),
                    });
                }
                decoded.push(clean[i + 1]);
                origins.push(positions[i]);
                i += 2;
            } else {
                if !(0x20..=0x7e).contains(&b) {
                    return Err(EmailError::QuotedChar {
                        at: original_pos(&positions, i, bytes.len()),
                    });
                }
                decoded.push(b);
                origins.push(positions[i]);
                i += 1;
            }
        }
        if decoded.is_empty() {
            return Err(EmailError::EmptyLocal {
                at: original_pos(&positions, end, bytes.len()),
            });
        }
        if decoded.len() > 64 {
            return Err(EmailError::LocalTooLong { at: origins[64] });
        }
        local = String::from_utf8(decoded).unwrap();
    }

    let domain_start_idx = at_idx + 1;
    let domain_start = original_pos(&positions, domain_start_idx, bytes.len());
    if domain_start_idx >= clean.len() {
        return Err(EmailError::EmptyDomain { at: domain_start });
    }

    let mut domain_end = clean.len();
    let root_pos;
    if clean[domain_end - 1] == b'.' {
        root_pos = Some(positions[domain_end - 1]);
        domain_end -= 1;
    } else {
        root_pos = None;
    }
    if domain_start_idx >= domain_end {
        return Err(EmailError::EmptyDomain { at: domain_start });
    }
    let domain_len = domain_end - domain_start_idx;
    if domain_len > 253 {
        return Err(EmailError::DomainTooLong {
            at: original_pos(&positions, domain_start_idx + 253, bytes.len()),
        });
    }

    let mut label_starts = Vec::new();
    let mut label_ends = Vec::new();
    let mut start = domain_start_idx;
    for i in domain_start_idx..=domain_end {
        if i == domain_end || clean[i] == b'.' {
            if start == i {
                let pos = if i == domain_end {
                    root_pos.unwrap_or_else(|| original_pos(&positions, i, bytes.len()))
                } else {
                    original_pos(&positions, i, bytes.len())
                };
                return Err(EmailError::EmptyLabel { at: pos });
            }
            let len = i - start;
            if len > 63 {
                return Err(EmailError::LabelTooLong {
                    at: original_pos(&positions, start + 63, bytes.len()),
                });
            }
            if clean[start] == b'-' {
                return Err(EmailError::LabelHyphen {
                    at: original_pos(&positions, start, bytes.len()),
                });
            }
            if clean[i - 1] == b'-' {
                return Err(EmailError::LabelHyphen {
                    at: original_pos(&positions, i - 1, bytes.len()),
                });
            }
            for j in start..i {
                if !clean[j].is_ascii_alphanumeric() && clean[j] != b'-' {
                    return Err(EmailError::DomainChar {
                        at: original_pos(&positions, j, bytes.len()),
                    });
                }
            }
            label_starts.push(start);
            label_ends.push(i);
            start = i + 1;
        }
    }

    if label_starts.len() < 2 {
        return Err(EmailError::DomainNeedsDot { at: domain_start });
    }
    let last_start = *label_starts.last().unwrap();
    let last_end = *label_ends.last().unwrap();
    if clean[last_start..last_end].iter().all(u8::is_ascii_digit) {
        return Err(EmailError::NumericTld {
            at: original_pos(&positions, last_start, bytes.len()),
        });
    }

    let domain = clean[domain_start_idx..domain_end]
        .iter()
        .map(|&b| b.to_ascii_lowercase() as char)
        .collect();
    Ok(Address {
        local,
        domain,
        quoted,
    })
}