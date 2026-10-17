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
    if !bytes.get(j).is_some_and(|b| is_hws(*b)) {
        return None;
    }
    while bytes.get(j).is_some_and(|b| is_hws(*b)) {
        j += 1;
    }
    Some(j)
}

fn cleanup(input: &[u8]) -> Result<(Vec<u8>, Vec<usize>, Vec<CfwsRun>), EmailError> {
    let mut kept = Vec::with_capacity(input.len());
    let mut map = Vec::with_capacity(input.len());
    let mut runs = Vec::new();

    let mut i = 0;
    let mut comment_depth = 0usize;
    let mut outer_open = 0usize;
    let mut run_start: Option<usize> = None;
    let mut protected_quote = false;

    while i < input.len() {
        let b = input[i];

        if protected_quote {
            kept.push(b);
            map.push(i);
            if b == b'\\' {
                i += 1;
                if i < input.len() {
                    kept.push(input[i]);
                    map.push(i);
                    i += 1;
                }
                continue;
            }
            if b == b'"' {
                protected_quote = false;
            }
            i += 1;
            continue;
        }

        if comment_depth > 0 {
            if let Some(end) = fold_end(input, i) {
                if run_start.is_none() {
                    run_start = Some(i);
                }
                i = end;
                continue;
            }
            if b == b'\r' || b == b'\n' {
                return Err(EmailError::BadFold { at: i });
            }
            if is_hws(b) {
                if run_start.is_none() {
                    run_start = Some(i);
                }
                i += 1;
                continue;
            }
            if b == b'\\' {
                if !input.get(i + 1).is_some_and(|c| is_printable(*c)) {
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
                if comment_depth == 0 {
                    // The comment and any adjacent whitespace/comments form one run.
                }
                continue;
            }
            if is_printable(b) {
                i += 1;
                continue;
            }
            return Err(EmailError::CommentChar { at: i });
        }

        if let Some(end) = fold_end(input, i) {
            if run_start.is_none() {
                run_start = Some(i);
            }
            i = end;
            continue;
        }
        if b == b'\r' || b == b'\n' {
            return Err(EmailError::BadFold { at: i });
        }
        if is_hws(b) {
            if run_start.is_none() {
                run_start = Some(i);
            }
            i += 1;
            continue;
        }
        if b == b'(' {
            if run_start.is_none() {
                run_start = Some(i);
            }
            comment_depth = 1;
            outer_open = i;
            i += 1;
            continue;
        }
        if b == b')' {
            return Err(EmailError::UnexpectedClose { at: i });
        }

        if let Some(start) = run_start.take() {
            runs.push(CfwsRun { start, end: i });
        }

        let first_retained = kept.is_empty();
        if first_retained && b == b'"' {
            protected_quote = true;
        }
        kept.push(b);
        map.push(i);
        i += 1;
    }

    if comment_depth > 0 {
        return Err(EmailError::UnclosedComment { at: outer_open });
    }
    if let Some(start) = run_start {
        runs.push(CfwsRun {
            start,
            end: input.len(),
        });
    }

    Ok((kept, map, runs))
}

fn pos(map: &[usize], index: usize, input_len: usize) -> usize {
    map.get(index).copied().unwrap_or(input_len)
}

pub fn parse_email(input: &str) -> Result<Address, EmailError> {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return Err(EmailError::EmptyInput);
    }
    if let Some(at) = bytes.iter().position(|b| *b >= 128) {
        return Err(EmailError::NonAscii { at });
    }

    let (clean, map, runs) = cleanup(bytes)?;
    for run in &runs {
        let left_ok = run.start == 0
            || map.iter().position(|p| *p >= run.end).is_some_and(|idx| clean.get(idx) == Some(&b'@'));
        let right_ok = run.end == bytes.len()
            || map.iter().rposition(|p| *p < run.start).is_some_and(|idx| clean.get(idx) == Some(&b'@'));
        if !left_ok && !right_ok {
            return Err(EmailError::CfwsBoundary { at: run.start });
        }
    }
    if clean.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let mut quote_end = None;
    if clean.first() == Some(&b'"') {
        let mut i = 1;
        while i < clean.len() {
            if clean[i] == b'\\' {
                i += 2;
            } else if clean[i] == b'"' {
                quote_end = Some(i);
                break;
            } else {
                i += 1;
            }
        }
        if quote_end.is_none() {
            return Err(EmailError::UnclosedQuote {
                at: pos(&map, 0, bytes.len()),
            });
        }
    }

    let mut separators = Vec::new();
    let mut i = 0;
    while i < clean.len() {
        if let Some(end) = quote_end {
            if i <= end {
                i += 1;
                continue;
            }
        }
        if clean[i] == b'@' {
            separators.push(i);
        }
        i += 1;
    }
    if separators.is_empty() {
        return Err(EmailError::MissingAt);
    }
    if separators.len() >= 2 {
        return Err(EmailError::MultipleAt {
            at: pos(&map, separators[1], bytes.len()),
        });
    }

    let sep = separators[0];
    if sep == 0 {
        return Err(EmailError::EmptyLocal {
            at: pos(&map, sep, bytes.len()),
        });
    }

    let mut local = String::new();
    let quoted = clean.first() == Some(&b'"');
    if !quoted {
        if sep > 64 {
            return Err(EmailError::LocalTooLong {
                at: pos(&map, 64, bytes.len()),
            });
        }
        for j in 0..sep {
            let b = clean[j];
            if b == b'.' {
                if j == 0 || j + 1 == sep || clean[j - 1] == b'.' {
                    return Err(EmailError::LocalDot {
                        at: pos(&map, j, bytes.len()),
                    });
                }
            } else if !(b.is_ascii_alphanumeric()
                || b"!#$%&'*+-/=?^_`{|}~".contains(&b))
            {
                return Err(EmailError::LocalChar {
                    at: pos(&map, j, bytes.len()),
                });
            }
        }
        local = String::from_utf8(clean[..sep].to_vec()).unwrap();
    } else {
        let end = quote_end.unwrap();
        if end + 1 != sep {
            return Err(EmailError::TrailingLocal {
                at: pos(&map, end + 1, bytes.len()),
            });
        }
        let mut origins = Vec::new();
        let mut j = 1;
        while j < end {
            let b = clean[j];
            if b == b'\\' {
                let next = clean[j + 1];
                if !matches!(next, b'"' | b'\\' | b'@') {
                    return Err(EmailError::BadEscape {
                        at: pos(&map, j, bytes.len()),
                    });
                }
                local.push(next as char);
                origins.push(map[j]);
                j += 2;
            } else {
                if !is_printable(b) {
                    return Err(EmailError::QuotedChar {
                        at: pos(&map, j, bytes.len()),
                    });
                }
                local.push(b as char);
                origins.push(map[j]);
                j += 1;
            }
        }
        if local.is_empty() {
            return Err(EmailError::EmptyLocal {
                at: pos(&map, end, bytes.len()),
            });
        }
        if local.len() > 64 {
            return Err(EmailError::LocalTooLong {
                at: origins[64],
            });
        }
    }

    let domain_start = sep + 1;
    let mut domain_end = clean.len();
    let mut removed_dot = None;
    if domain_end > domain_start && clean[domain_end - 1] == b'.' {
        domain_end -= 1;
        removed_dot = Some(domain_end);
    }
    if domain_end == domain_start {
        return Err(EmailError::EmptyDomain {
            at: pos(&map, domain_start, bytes.len()),
        });
    }

    let domain_len = domain_end - domain_start;
    if domain_len > 253 {
        return Err(EmailError::DomainTooLong {
            at: pos(&map, domain_start + 253, bytes.len()),
        });
    }

    let mut labels: Vec<(usize, usize)> = Vec::new();
    let mut label_start = domain_start;
    let mut j = domain_start;
    while j <= domain_end {
        if j == domain_end || clean[j] == b'.' {
            if j == label_start {
                let at = if j == domain_end {
                    removed_dot.map(|idx| pos(&map, idx, bytes.len()))
                        .unwrap_or_else(|| pos(&map, j, bytes.len()))
                } else {
                    pos(&map, j, bytes.len())
                };
                return Err(EmailError::EmptyLabel { at });
            }
            labels.push((label_start, j));
            label_start = j + 1;
        }
        j += 1;
    }

    for &(start, end) in &labels {
        let len = end - start;
        if len > 63 {
            return Err(EmailError::LabelTooLong {
                at: pos(&map, start + 63, bytes.len()),
            });
        }
        if clean[start] == b'-' {
            return Err(EmailError::LabelHyphen {
                at: pos(&map, start, bytes.len()),
            });
        }
        if clean[end - 1] == b'-' {
            return Err(EmailError::LabelHyphen {
                at: pos(&map, end - 1, bytes.len()),
            });
        }
        for k in start..end {
            let b = clean[k];
            if !(b.is_ascii_alphanumeric() || b == b'-') {
                return Err(EmailError::DomainChar {
                    at: pos(&map, k, bytes.len()),
                });
            }
        }
    }

    if labels.len() < 2 {
        return Err(EmailError::DomainNeedsDot {
            at: pos(&map, domain_start, bytes.len()),
        });
    }
    let (last_start, last_end) = *labels.last().unwrap();
    if clean[last_start..last_end].iter().all(u8::is_ascii_digit) {
        return Err(EmailError::NumericTld {
            at: pos(&map, last_start, bytes.len()),
        });
    }

    let domain = clean[domain_start..domain_end]
        .iter()
        .map(|b| b.to_ascii_lowercase() as char)
        .collect();

    Ok(Address {
        local,
        domain,
        quoted,
    })
}