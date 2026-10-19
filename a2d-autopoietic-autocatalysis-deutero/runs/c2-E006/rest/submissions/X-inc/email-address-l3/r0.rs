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

fn is_printable(b: u8) -> bool {
    (0x20..=0x7e).contains(&b)
}

fn lexical_cleanup(input: &[u8]) -> Result<(Vec<u8>, Vec<usize>, Vec<Run>), EmailError> {
    let mut clean = Vec::with_capacity(input.len());
    let mut map = Vec::with_capacity(input.len());
    let mut runs = Vec::new();
    let mut i = 0;
    let mut first_retained = true;
    let mut protected_quote = false;

    while i < input.len() {
        if protected_quote {
            let b = input[i];
            clean.push(b);
            map.push(i);
            i += 1;
            if b == b'\\' {
                if i < input.len() {
                    clean.push(input[i]);
                    map.push(i);
                    i += 1;
                }
            } else if b == b'"' {
                protected_quote = false;
            }
            continue;
        }

        let b = input[i];
        if first_retained && b == b'"' {
            first_retained = false;
            protected_quote = true;
            clean.push(b);
            map.push(i);
            i += 1;
            continue;
        }

        if b == b' ' || b == b'\t' || b == b'\r' || b == b'\n' || b == b'(' {
            let run_start = i;
            let mut depth = 0usize;
            let mut outer_open = None;

            loop {
                if i >= input.len() {
                    break;
                }
                let c = input[i];

                if depth == 0 {
                    if c == b' ' || c == b'\t' {
                        i += 1;
                    } else if c == b'\r' || c == b'\n' {
                        if c == b'\r'
                            && i + 2 < input.len()
                            && input[i + 1] == b'\n'
                            && is_hws(input[i + 2])
                        {
                            i += 2;
                            while i < input.len() && is_hws(input[i]) {
                                i += 1;
                            }
                        } else {
                            return Err(EmailError::BadFold { at: i });
                        }
                    } else if c == b'(' {
                        depth = 1;
                        outer_open = Some(i);
                        i += 1;
                    } else {
                        break;
                    }
                } else {
                    if c == b'\\' {
                        let slash = i;
                        if i + 1 >= input.len() || !is_printable(input[i + 1]) {
                            return Err(EmailError::CommentEscape { at: slash });
                        }
                        i += 2;
                    } else if c == b'(' {
                        if depth == 3 {
                            return Err(EmailError::CommentDepth { at: i });
                        }
                        depth += 1;
                        i += 1;
                    } else if c == b')' {
                        depth -= 1;
                        i += 1;
                    } else if c == b'\r' || c == b'\n' {
                        if c == b'\r'
                            && i + 2 < input.len()
                            && input[i + 1] == b'\n'
                            && is_hws(input[i + 2])
                        {
                            i += 2;
                            while i < input.len() && is_hws(input[i]) {
                                i += 1;
                            }
                        } else {
                            return Err(EmailError::BadFold { at: i });
                        }
                    } else if is_printable(c) || c == b'\t' {
                        i += 1;
                    } else {
                        return Err(EmailError::CommentChar { at: i });
                    }
                }
            }

            if depth != 0 {
                return Err(EmailError::UnclosedComment {
                    at: outer_open.unwrap_or(run_start),
                });
            }
            runs.push(Run { start: run_start, end: i });
            continue;
        }

        if b == b')' {
            return Err(EmailError::UnexpectedClose { at: i });
        }

        first_retained = false;
        clean.push(b);
        map.push(i);
        i += 1;
    }

    Ok((clean, map, runs))
}

fn original_pos(map: &[usize], index: usize, input_len: usize) -> usize {
    map.get(index).copied().unwrap_or(input_len)
}

pub fn parse_email(input: &str) -> Result<Address, EmailError> {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return Err(EmailError::EmptyInput);
    }
    if let Some(at) = bytes.iter().position(|&b| b >= 128) {
        return Err(EmailError::NonAscii { at });
    }

    let (clean, map, runs) = lexical_cleanup(bytes)?;
    let input_len = bytes.len();

    for run in runs {
        let left = map.iter().position(|&p| p >= run.end);
        let right = map.iter().rposition(|&p| p < run.start);

        let at_left = left.map_or(false, |idx| clean[idx] == b'@');
        let at_right = right.map_or(false, |idx| clean[idx] == b'@');

        if right.is_none() || left.is_none() || at_left || at_right {
            continue;
        }
        return Err(EmailError::CfwsBoundary { at: run.start });
    }

    if clean.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let mut quote_end = None;
    if clean[0] == b'"' {
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
                at: original_pos(&map, 0, input_len),
            });
        }
    }

    let mut separators = Vec::new();
    let mut i = 0;
    if let Some(end) = quote_end {
        i = end + 1;
    }
    while i < clean.len() {
        if clean[i] == b'@' {
            separators.push(i);
        }
        i += 1;
    }

    if separators.is_empty() {
        return Err(EmailError::MissingAt);
    }
    if separators.len() > 1 {
        return Err(EmailError::MultipleAt {
            at: original_pos(&map, separators[1], input_len),
        });
    }

    let sep = separators[0];
    if sep == 0 {
        return Err(EmailError::EmptyLocal {
            at: original_pos(&map, sep, input_len),
        });
    }

    let mut local = String::new();
    let quoted = clean[0] == b'"';

    if !quoted {
        let len = sep;
        if len > 64 {
            return Err(EmailError::LocalTooLong {
                at: original_pos(&map, 64, input_len),
            });
        }
        for j in 0..len {
            let b = clean[j];
            if b == b'.' {
                if j == 0 || j + 1 == len || clean[j - 1] == b'.' {
                    return Err(EmailError::LocalDot {
                        at: original_pos(&map, j, input_len),
                    });
                }
            } else if !(b.is_ascii_alphanumeric()
                || b"!#$%&'*+-/=?^_`{|}~".contains(&b))
            {
                return Err(EmailError::LocalChar {
                    at: original_pos(&map, j, input_len),
                });
            }
        }
        local = String::from_utf8(clean[..sep].to_vec()).unwrap();
    } else {
        let end = quote_end.unwrap();
        if end + 1 != sep {
            return Err(EmailError::TrailingLocal {
                at: original_pos(&map, end + 1, input_len),
            });
        }

        let mut decoded = Vec::new();
        let mut origins = Vec::new();
        let mut j = 1;
        while j < end {
            let b = clean[j];
            if b == b'\\' {
                if j + 1 >= end
                    || !(clean[j + 1] == b'"' || clean[j + 1] == b'\\' || clean[j + 1] == b'@')
                {
                    return Err(EmailError::BadEscape {
                        at: original_pos(&map, j, input_len),
                    });
                }
                decoded.push(clean[j + 1]);
                origins.push(map[j]);
                j += 2;
            } else {
                if !is_printable(b) {
                    return Err(EmailError::QuotedChar {
                        at: original_pos(&map, j, input_len),
                    });
                }
                decoded.push(b);
                origins.push(map[j]);
                j += 1;
            }
        }
        if decoded.is_empty() {
            return Err(EmailError::EmptyLocal {
                at: original_pos(&map, end, input_len),
            });
        }
        if decoded.len() > 64 {
            return Err(EmailError::LocalTooLong { at: origins[64] });
        }
        local = String::from_utf8(decoded).unwrap();
    }

    let domain_start = sep + 1;
    let mut domain_end = clean.len();
    let mut root_dot = None;
    if domain_end > domain_start && clean[domain_end - 1] == b'.' {
        domain_end -= 1;
        root_dot = Some(domain_end);
    }
    if domain_end == domain_start {
        return Err(EmailError::EmptyDomain {
            at: original_pos(&map, domain_start, input_len),
        });
    }
    let domain_len = domain_end - domain_start;
    if domain_len > 253 {
        return Err(EmailError::DomainTooLong {
            at: original_pos(&map, domain_start + 253, input_len),
        });
    }

    let mut labels: Vec<(usize, usize)> = Vec::new();
    let mut label_start = domain_start;
    for j in domain_start..domain_end {
        if clean[j] == b'.' {
            labels.push((label_start, j));
            label_start = j + 1;
        }
    }
    labels.push((label_start, domain_end));

    for (idx, &(start, end)) in labels.iter().enumerate() {
        if start == end {
            let pos = if idx + 1 == labels.len() {
                root_dot.map(|p| original_pos(&map, p, input_len))
                    .unwrap_or_else(|| original_pos(&map, start, input_len))
            } else {
                original_pos(&map, start, input_len)
            };
            return Err(EmailError::EmptyLabel { at: pos });
        }
        if end - start > 63 {
            return Err(EmailError::LabelTooLong {
                at: original_pos(&map, start + 63, input_len),
            });
        }
        if clean[start] == b'-' {
            return Err(EmailError::LabelHyphen {
                at: original_pos(&map, start, input_len),
            });
        }
        if clean[end - 1] == b'-' {
            return Err(EmailError::LabelHyphen {
                at: original_pos(&map, end - 1, input_len),
            });
        }
        for j in start..end {
            let b = clean[j];
            if !(b.is_ascii_alphanumeric() || b == b'-') {
                return Err(EmailError::DomainChar {
                    at: original_pos(&map, j, input_len),
                });
            }
        }
    }

    if labels.len() < 2 {
        return Err(EmailError::DomainNeedsDot {
            at: original_pos(&map, domain_start, input_len),
        });
    }

    let (last_start, last_end) = labels[labels.len() - 1];
    if clean[last_start..last_end].iter().all(u8::is_ascii_digit) {
        return Err(EmailError::NumericTld {
            at: original_pos(&map, last_start, input_len),
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