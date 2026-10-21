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

#[derive(Clone)]
struct Run {
    start: usize,
    end: usize,
}

fn horizontal(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

fn printable(b: u8) -> bool {
    (0x20..=0x7e).contains(&b)
}

fn fold_end(bytes: &[u8], i: usize) -> Option<usize> {
    if bytes.get(i) != Some(&b'\r') || bytes.get(i + 1) != Some(&b'\n') {
        return None;
    }
    let mut j = i + 2;
    if !bytes.get(j).copied().map(horizontal).unwrap_or(false) {
        return None;
    }
    while bytes.get(j).copied().map(horizontal).unwrap_or(false) {
        j += 1;
    }
    Some(j)
}

fn clean(input: &[u8]) -> Result<(Vec<u8>, Vec<usize>, Vec<Run>), EmailError> {
    let mut out = Vec::new();
    let mut positions = Vec::new();
    let mut runs = Vec::new();
    let mut i = 0;
    let mut quote = false;
    let mut comment_depth = 0usize;
    let mut outer_open = 0usize;
    let mut run_start: Option<usize> = None;

    while i < input.len() {
        if quote {
            let b = input[i];
            out.push(b);
            positions.push(i);
            i += 1;
            if b == b'\\' && i < input.len() {
                out.push(input[i]);
                positions.push(i);
                i += 1;
            } else if b == b'"' {
                quote = false;
            }
            continue;
        }

        if comment_depth > 0 {
            let b = input[i];
            if let Some(end) = fold_end(input, i) {
                i = end;
                continue;
            }
            if b == b'\r' || b == b'\n' {
                return Err(EmailError::BadFold { at: i });
            }
            if b == b'\\' {
                if i + 1 >= input.len() || !printable(input[i + 1]) {
                    return Err(EmailError::CommentEscape { at: i });
                }
                i += 2;
                continue;
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
            if !printable(b) && !horizontal(b) {
                return Err(EmailError::CommentChar { at: i });
            }
            i += 1;
            continue;
        }

        if quote {
            continue;
        }

        if out.is_empty() && input[i] == b'"' {
            if let Some(s) = run_start.take() {
                runs.push(Run { start: s, end: i });
            }
            quote = true;
            out.push(input[i]);
            positions.push(i);
            i += 1;
            continue;
        }

        let b = input[i];
        if let Some(end) = fold_end(input, i) {
            run_start.get_or_insert(i);
            i = end;
            continue;
        }
        if b == b'\r' || b == b'\n' {
            return Err(EmailError::BadFold { at: i });
        }
        if horizontal(b) {
            run_start.get_or_insert(i);
            i += 1;
            continue;
        }
        if b == b'(' {
            run_start.get_or_insert(i);
            outer_open = i;
            comment_depth = 1;
            i += 1;
            continue;
        }
        if b == b')' {
            return Err(EmailError::UnexpectedClose { at: i });
        }
        if let Some(s) = run_start.take() {
            runs.push(Run { start: s, end: i });
        }
        out.push(b);
        positions.push(i);
        i += 1;
    }

    if comment_depth > 0 {
        return Err(EmailError::UnclosedComment { at: outer_open });
    }
    if let Some(s) = run_start {
        runs.push(Run { start: s, end: input.len() });
    }

    Ok((out, positions, runs))
}

fn pos(positions: &[usize], input_len: usize, i: usize) -> usize {
    positions.get(i).copied().unwrap_or(input_len)
}

pub fn parse_email(input: &str) -> Result<Address, EmailError> {
    if input.is_empty() {
        return Err(EmailError::EmptyInput);
    }
    let bytes = input.as_bytes();
    if let Some(at) = bytes.iter().position(|&b| b >= 128) {
        return Err(EmailError::NonAscii { at });
    }

    let (cleaned, positions, runs) = clean(bytes)?;
    for run in runs {
        let left_at = run.start > 0 && bytes[run.start - 1] == b'@';
        let right_at = run.end < bytes.len() && bytes[run.end] == b'@';
        let at_boundary = run.start == 0 || run.end == bytes.len();
        if !at_boundary && !left_at && !right_at {
            return Err(EmailError::CfwsBoundary { at: run.start });
        }
    }
    if cleaned.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let n = cleaned.len();
    let mut quote_end = None;
    if cleaned[0] == b'"' {
        let mut i = 1;
        while i < n {
            if cleaned[i] == b'\\' {
                i += 2;
            } else if cleaned[i] == b'"' {
                quote_end = Some(i);
                break;
            } else {
                i += 1;
            }
        }
        if quote_end.is_none() {
            return Err(EmailError::UnclosedQuote {
                at: pos(&positions, bytes.len(), 0),
            });
        }
    }

    let protected_end = quote_end.map(|i| i + 1).unwrap_or(0);
    let ats: Vec<usize> = (protected_end..n).filter(|&i| cleaned[i] == b'@').collect();
    if ats.is_empty() {
        return Err(EmailError::MissingAt);
    }
    if ats.len() > 1 {
        return Err(EmailError::MultipleAt {
            at: pos(&positions, bytes.len(), ats[1]),
        });
    }
    let sep = ats[0];
    if sep == 0 {
        return Err(EmailError::EmptyLocal {
            at: pos(&positions, bytes.len(), sep),
        });
    }

    let mut local = String::new();
    let quoted = cleaned[0] == b'"';
    if !quoted {
        if sep > 64 {
            return Err(EmailError::LocalTooLong {
                at: pos(&positions, bytes.len(), 64),
            });
        }
        for i in 0..sep {
            let b = cleaned[i];
            if b == b'.' {
                if i == 0 || i + 1 == sep || cleaned[i - 1] == b'.' {
                    return Err(EmailError::LocalDot {
                        at: pos(&positions, bytes.len(), i),
                    });
                }
            } else if !b.is_ascii_alphanumeric()
                && !b"!#$%&'*+-/=?^_`{|}~".contains(&b)
            {
                return Err(EmailError::LocalChar {
                    at: pos(&positions, bytes.len(), i),
                });
            }
            local.push(b as char);
        }
    } else {
        let end = quote_end.unwrap();
        if end + 1 != sep {
            return Err(EmailError::TrailingLocal {
                at: pos(&positions, bytes.len(), end + 1),
            });
        }
        let mut origins = Vec::new();
        let mut i = 1;
        while i < end {
            let b = cleaned[i];
            if b == b'\\' {
                if i + 1 >= end || !matches!(cleaned[i + 1], b'"' | b'\\' | b'@') {
                    return Err(EmailError::BadEscape {
                        at: pos(&positions, bytes.len(), i),
                    });
                }
                local.push(cleaned[i + 1] as char);
                origins.push(pos(&positions, bytes.len(), i));
                i += 2;
            } else {
                if !printable(b) {
                    return Err(EmailError::QuotedChar {
                        at: pos(&positions, bytes.len(), i),
                    });
                }
                local.push(b as char);
                origins.push(pos(&positions, bytes.len(), i));
                i += 1;
            }
        }
        if local.is_empty() {
            return Err(EmailError::EmptyLocal {
                at: pos(&positions, bytes.len(), end),
            });
        }
        if local.len() > 64 {
            return Err(EmailError::LocalTooLong { at: origins[64] });
        }
    }

    let domain_start = sep + 1;
    let mut domain_end = n;
    let mut root_pos = None;
    if domain_end > domain_start && cleaned[domain_end - 1] == b'.' {
        domain_end -= 1;
        root_pos = Some(pos(&positions, bytes.len(), domain_end));
    }
    if domain_start == domain_end {
        return Err(EmailError::EmptyDomain {
            at: pos(&positions, bytes.len(), domain_start),
        });
    }
    if domain_end - domain_start > 253 {
        return Err(EmailError::DomainTooLong {
            at: pos(&positions, bytes.len(), domain_start + 253),
        });
    }

    let mut labels = Vec::new();
    let mut start = domain_start;
    for i in domain_start..=domain_end {
        if i == domain_end || cleaned[i] == b'.' {
            if start == i {
                let at = if i == domain_end {
                    root_pos.unwrap_or_else(|| pos(&positions, bytes.len(), i))
                } else {
                    pos(&positions, bytes.len(), i)
                };
                return Err(EmailError::EmptyLabel { at });
            }
            let len = i - start;
            if len > 63 {
                return Err(EmailError::LabelTooLong {
                    at: pos(&positions, bytes.len(), start + 63),
                });
            }
            if cleaned[start] == b'-' {
                return Err(EmailError::LabelHyphen {
                    at: pos(&positions, bytes.len(), start),
                });
            }
            if cleaned[i - 1] == b'-' {
                return Err(EmailError::LabelHyphen {
                    at: pos(&positions, bytes.len(), i - 1),
                });
            }
            for j in start..i {
                let b = cleaned[j];
                if !b.is_ascii_alphanumeric() && b != b'-' {
                    return Err(EmailError::DomainChar {
                        at: pos(&positions, bytes.len(), j),
                    });
                }
            }
            labels.push((start, i));
            start = i + 1;
        }
    }

    if labels.len() < 2 {
        return Err(EmailError::DomainNeedsDot {
            at: pos(&positions, bytes.len(), domain_start),
        });
    }
    let (last_start, last_end) = *labels.last().unwrap();
    if cleaned[last_start..last_end].iter().all(u8::is_ascii_digit) {
        return Err(EmailError::NumericTld {
            at: pos(&positions, bytes.len(), last_start),
        });
    }

    let domain = cleaned[domain_start..domain_end]
        .iter()
        .map(|b| b.to_ascii_lowercase() as char)
        .collect();
    Ok(Address {
        local,
        domain,
        quoted,
    })
}