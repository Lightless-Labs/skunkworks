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
struct Byte {
    value: u8,
    original: usize,
}

struct Cleaned {
    bytes: Vec<Byte>,
    runs: Vec<usize>,
}

fn is_hws(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

fn fold_end(input: &[u8], i: usize) -> Option<usize> {
    if input.get(i) != Some(&b'\r') || input.get(i + 1) != Some(&b'\n') {
        return None;
    }
    let mut end = i + 2;
    if !input.get(end).copied().is_some_and(is_hws) {
        return None;
    }
    while input.get(end).copied().is_some_and(is_hws) {
        end += 1;
    }
    Some(end)
}

fn clean_cfws(input: &[u8]) -> Result<Cleaned, EmailError> {
    let mut bytes = Vec::new();
    let mut runs = Vec::new();
    let mut i = 0;
    let mut quote_protected = false;
    let mut quoted = false;
    let mut comment_depth = 0usize;
    let mut outer_comment = 0usize;
    let mut in_run = false;

    while i < input.len() {
        let b = input[i];

        if quote_protected && quoted {
            bytes.push(Byte { value: b, original: i });
            if b == b'\\' {
                if i + 1 < input.len() {
                    i += 1;
                    bytes.push(Byte { value: input[i], original: i });
                }
            } else if b == b'"' {
                quoted = false;
            }
            i += 1;
            continue;
        }

        if comment_depth > 0 {
            if let Some(end) = fold_end(input, i) {
                i = end;
                continue;
            }
            if b == b'\r' || b == b'\n' {
                return Err(EmailError::BadFold { at: i });
            }
            if b == b'\\' {
                let Some(&next) = input.get(i + 1) else {
                    return Err(EmailError::CommentEscape { at: i });
                };
                if !(0x20..=0x7e).contains(&next) {
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
            if !(b == b'\t' || (0x20..=0x7e).contains(&b)) {
                return Err(EmailError::CommentChar { at: i });
            }
            i += 1;
            continue;
        }

        if let Some(end) = fold_end(input, i) {
            if !in_run {
                runs.push(i);
                in_run = true;
            }
            i = end;
            continue;
        }
        if b == b'\r' || b == b'\n' {
            return Err(EmailError::BadFold { at: i });
        }
        if is_hws(b) {
            if !in_run {
                runs.push(i);
                in_run = true;
            }
            i += 1;
            continue;
        }
        if b == b'(' {
            if !in_run {
                runs.push(i);
                in_run = true;
            }
            comment_depth = 1;
            outer_comment = i;
            i += 1;
            continue;
        }
        if b == b')' {
            return Err(EmailError::UnexpectedClose { at: i });
        }

        in_run = false;
        if !quote_protected && bytes.is_empty() && b == b'"' {
            quote_protected = true;
            quoted = true;
        }
        bytes.push(Byte { value: b, original: i });
        i += 1;
    }

    if comment_depth > 0 {
        return Err(EmailError::UnclosedComment { at: outer_comment });
    }

    Ok(Cleaned { bytes, runs })
}

fn pos(bytes: &[Byte], i: usize, input_len: usize) -> usize {
    bytes.get(i).map_or(input_len, |b| b.original)
}

pub fn parse_email(input: &str) -> Result<Address, EmailError> {
    if input.is_empty() {
        return Err(EmailError::EmptyInput);
    }
    let raw = input.as_bytes();
    if let Some(at) = raw.iter().position(|&b| b >= 128) {
        return Err(EmailError::NonAscii { at });
    }

    let cleaned = clean_cfws(raw)?;
    let bytes = &cleaned.bytes;

    let mut separators = Vec::new();
    let mut quote_end = None;
    if bytes.first().is_some_and(|b| b.value == b'"') {
        let mut i = 1;
        let mut end = None;
        while i < bytes.len() {
            match bytes[i].value {
                b'\\' => i += 2,
                b'"' => {
                    end = Some(i);
                    break;
                }
                _ => i += 1,
            }
        }
        let Some(end) = end else {
            return Err(EmailError::UnclosedQuote { at: pos(bytes, 0, raw.len()) });
        };
        quote_end = Some(end);
    }

    let start = quote_end.map_or(0, |e| e + 1);
    for i in start..bytes.len() {
        if bytes[i].value == b'@' {
            separators.push(i);
        }
    }
    if separators.is_empty() {
        return Err(EmailError::MissingAt);
    }
    if separators.len() > 1 {
        return Err(EmailError::MultipleAt {
            at: pos(bytes, separators[1], raw.len()),
        });
    }

    for &run in &cleaned.runs {
        let left = bytes.iter().rposition(|b| b.original < run);
        let right = bytes.iter().position(|b| b.original > run);
        let legal = left.is_none()
            || right.is_none()
            || left.is_some_and(|i| bytes[i].value == b'@')
            || right.is_some_and(|i| bytes[i].value == b'@');
        if !legal {
            return Err(EmailError::CfwsBoundary { at: run });
        }
    }

    if bytes.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let sep = separators[0];
    if sep == 0 {
        return Err(EmailError::EmptyLocal { at: pos(bytes, sep, raw.len()) });
    }
    let local_bytes = &bytes[..sep];
    let domain_start = pos(bytes, sep + 1, raw.len());
    let domain_bytes = &bytes[sep + 1..];

    let (local, quoted) = if local_bytes[0].value != b'"' {
        if local_bytes.len() > 64 {
            return Err(EmailError::LocalTooLong {
                at: pos(local_bytes, 64, raw.len()),
            });
        }
        for i in 0..local_bytes.len() {
            let b = local_bytes[i].value;
            if b == b'.' {
                if i == 0 || i + 1 == local_bytes.len() || local_bytes[i - 1].value == b'.' {
                    return Err(EmailError::LocalDot {
                        at: pos(local_bytes, i, raw.len()),
                    });
                }
            } else if !b.is_ascii_alphanumeric()
                && !b"!#$%&'*+-/=?^_`{|}~".contains(&b)
            {
                return Err(EmailError::LocalChar {
                    at: pos(local_bytes, i, raw.len()),
                });
            }
        }
        (
            local_bytes.iter().map(|b| b.value as char).collect::<String>(),
            false,
        )
    } else {
        let close = quote_end.unwrap_or(0);
        if close + 1 != sep {
            return Err(EmailError::TrailingLocal {
                at: pos(bytes, close + 1, raw.len()),
            });
        }
        let mut decoded = Vec::new();
        let mut origins = Vec::new();
        let mut i = 1;
        while i < close {
            let b = bytes[i].value;
            if b == b'\\' {
                let Some(next) = bytes.get(i + 1).filter(|_| i + 1 < close) else {
                    return Err(EmailError::BadEscape {
                        at: pos(bytes, i, raw.len()),
                    });
                };
                if !matches!(next.value, b'"' | b'\\' | b'@') {
                    return Err(EmailError::BadEscape {
                        at: pos(bytes, i, raw.len()),
                    });
                }
                decoded.push(next.value);
                origins.push(pos(bytes, i, raw.len()));
                i += 2;
            } else {
                if !(0x20..=0x7e).contains(&b) {
                    return Err(EmailError::QuotedChar {
                        at: pos(bytes, i, raw.len()),
                    });
                }
                decoded.push(b);
                origins.push(pos(bytes, i, raw.len()));
                i += 1;
            }
        }
        if decoded.is_empty() {
            return Err(EmailError::EmptyLocal {
                at: pos(bytes, close, raw.len()),
            });
        }
        if decoded.len() > 64 {
            return Err(EmailError::LocalTooLong { at: origins[64] });
        }
        (decoded.into_iter().map(char::from).collect::<String>(), true)
    };

    let mut domain = domain_bytes.to_vec();
    let mut removed_root = None;
    if domain.last().is_some_and(|b| b.value == b'.') {
        removed_root = domain.pop().map(|b| b.original);
    }
    if domain.is_empty() {
        return Err(EmailError::EmptyDomain { at: domain_start });
    }
    if domain.len() > 253 {
        return Err(EmailError::DomainTooLong {
            at: pos(&domain, 253, raw.len()),
        });
    }

    let mut labels: Vec<(usize, usize)> = Vec::new();
    let mut label_start = 0;
    for i in 0..=domain.len() {
        if i == domain.len() || domain[i].value == b'.' {
            labels.push((label_start, i));
            label_start = i + 1;
        }
    }

    for (start, end) in &labels {
        if start == end {
            let at = if *start < domain.len() {
                pos(&domain, *start, raw.len())
            } else {
                removed_root.unwrap_or(raw.len())
            };
            return Err(EmailError::EmptyLabel { at });
        }
        if end - start > 63 {
            return Err(EmailError::LabelTooLong {
                at: pos(&domain, start + 63, raw.len()),
            });
        }
        if domain[*start].value == b'-' {
            return Err(EmailError::LabelHyphen {
                at: pos(&domain, *start, raw.len()),
            });
        }
        if domain[end - 1].value == b'-' {
            return Err(EmailError::LabelHyphen {
                at: pos(&domain, end - 1, raw.len()),
            });
        }
        for i in *start..*end {
            let b = domain[i].value;
            if !b.is_ascii_alphanumeric() && b != b'-' {
                return Err(EmailError::DomainChar {
                    at: pos(&domain, i, raw.len()),
                });
            }
        }
    }

    if labels.len() < 2 {
        return Err(EmailError::DomainNeedsDot { at: domain_start });
    }
    let (last_start, last_end) = labels[labels.len() - 1];
    if domain[last_start..last_end]
        .iter()
        .all(|b| b.value.is_ascii_digit())
    {
        return Err(EmailError::NumericTld {
            at: pos(&domain, last_start, raw.len()),
        });
    }

    let domain = domain
        .iter()
        .map(|b| (b.value as char).to_ascii_lowercase())
        .collect::<String>();
    Ok(Address { local, domain, quoted })
}

fn main() {}