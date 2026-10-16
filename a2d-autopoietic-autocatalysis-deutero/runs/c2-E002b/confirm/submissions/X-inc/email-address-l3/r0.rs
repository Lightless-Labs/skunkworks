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
    at: usize,
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

fn is_atom_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$%&'*+-/=?^_`{|}~".contains(&b)
}

pub fn parse_email(input: &str) -> Result<Address, EmailError> {
    if input.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let bytes = input.as_bytes();
    if let Some(at) = bytes.iter().position(|&b| b >= 128) {
        return Err(EmailError::NonAscii { at });
    }

    let mut cleaned: Vec<Byte> = Vec::with_capacity(bytes.len());
    let mut runs: Vec<Run> = Vec::new();
    let mut i = 0usize;

    // Quote protection begins only at the first retained byte.
    let mut protected_quote = false;
    let mut quote_closed = false;

    while i < bytes.len() {
        let b = bytes[i];

        if protected_quote && !quote_closed {
            cleaned.push(Byte { value: b, at: i });
            if b == b'\\' {
                if i + 1 < bytes.len() {
                    i += 1;
                    cleaned.push(Byte {
                        value: bytes[i],
                        at: i,
                    });
                }
            } else if b == b'"' {
                quote_closed = true;
            }
            i += 1;
            continue;
        }

        let is_run_start = b == b' '
            || b == b'\t'
            || b == b'('
            || b == b')'
            || b == b'\r'
            || b == b'\n';

        if is_run_start {
            let run_start = i;
            loop {
                if i >= bytes.len() {
                    break;
                }

                let c = bytes[i];
                if is_hws(c) {
                    i += 1;
                    continue;
                }

                if c == b'\r' || c == b'\n' {
                    if c == b'\r'
                        && i + 2 < bytes.len()
                        && bytes[i + 1] == b'\n'
                        && is_hws(bytes[i + 2])
                    {
                        i += 2;
                        while i < bytes.len() && is_hws(bytes[i]) {
                            i += 1;
                        }
                        continue;
                    }
                    return Err(EmailError::BadFold { at: i });
                }

                if c == b'(' {
                    let outer = i;
                    let mut depth = 1usize;
                    i += 1;

                    while i < bytes.len() {
                        let d = bytes[i];

                        if d == b'\\' {
                            let slash = i;
                            if i + 1 >= bytes.len() || !is_printable(bytes[i + 1]) {
                                return Err(EmailError::CommentEscape { at: slash });
                            }
                            i += 2;
                            continue;
                        }

                        if d == b'(' {
                            if depth == 3 {
                                return Err(EmailError::CommentDepth { at: i });
                            }
                            depth += 1;
                            i += 1;
                            continue;
                        }

                        if d == b')' {
                            depth -= 1;
                            i += 1;
                            if depth == 0 {
                                break;
                            }
                            continue;
                        }

                        if d == b'\r' || d == b'\n' {
                            if d == b'\r'
                                && i + 2 < bytes.len()
                                && bytes[i + 1] == b'\n'
                                && is_hws(bytes[i + 2])
                            {
                                i += 2;
                                while i < bytes.len() && is_hws(bytes[i]) {
                                    i += 1;
                                }
                                continue;
                            }
                            return Err(EmailError::BadFold { at: i });
                        }

                        if !is_printable(d) && d != b'\t' {
                            return Err(EmailError::CommentChar { at: i });
                        }
                        i += 1;
                    }

                    if depth != 0 {
                        return Err(EmailError::UnclosedComment { at: outer });
                    }
                    continue;
                }

                if c == b')' {
                    return Err(EmailError::UnexpectedClose { at: i });
                }

                break;
            }

            runs.push(Run {
                start: run_start,
                end: i,
            });
            continue;
        }

        if cleaned.is_empty() && b == b'"' {
            protected_quote = true;
            cleaned.push(Byte { value: b, at: i });
            i += 1;
            continue;
        }

        cleaned.push(Byte { value: b, at: i });
        i += 1;
    }

    // A run is permitted only at a cleaned boundary or next to an @.
    for run in &runs {
        let left = cleaned
            .iter()
            .rev()
            .find(|b| b.at < run.start)
            .map(|b| b.value);
        let right = cleaned
            .iter()
            .find(|b| b.at >= run.end)
            .map(|b| b.value);

        let at_boundary = left.is_none() || right.is_none();
        if !at_boundary && left != Some(b'@') && right != Some(b'@') {
            return Err(EmailError::CfwsBoundary { at: run.start });
        }
    }

    if cleaned.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let cleaned_bytes: Vec<u8> = cleaned.iter().map(|b| b.value).collect();

    let quoted_end = if cleaned_bytes.first() == Some(&b'"') {
        let mut j = 1usize;
        let mut found = None;
        while j < cleaned_bytes.len() {
            if cleaned_bytes[j] == b'\\' {
                j += 2;
            } else if cleaned_bytes[j] == b'"' {
                found = Some(j);
                break;
            } else {
                j += 1;
            }
        }
        match found {
            Some(end) => Some(end),
            None => return Err(EmailError::UnclosedQuote { at: cleaned[0].at }),
        }
    } else {
        None
    };

    let mut separators = Vec::new();
    let mut j = 0usize;
    while j < cleaned.len() {
        if let Some(end) = quoted_end {
            if j <= end {
                j += 1;
                continue;
            }
        }
        if cleaned[j].value == b'@' {
            separators.push(j);
            if separators.len() == 2 {
                return Err(EmailError::MultipleAt {
                    at: cleaned[j].at,
                });
            }
        }
        j += 1;
    }

    if separators.is_empty() {
        return Err(EmailError::MissingAt);
    }

    let sep = separators[0];
    if sep == 0 {
        return Err(EmailError::EmptyLocal { at: cleaned[sep].at });
    }

    let domain_start = if sep + 1 < cleaned.len() {
        cleaned[sep + 1].at
    } else {
        input.len()
    };

    let local_bytes = &cleaned[..sep];
    let (local, quoted) = if let Some(end) = quoted_end {
        if end + 1 != sep {
            let at = if end + 1 < cleaned.len() {
                cleaned[end + 1].at
            } else {
                cleaned[sep].at
            };
            return Err(EmailError::TrailingLocal { at });
        }

        let mut decoded = Vec::new();
        let mut origins = Vec::new();
        let mut k = 1usize;
        while k < end {
            let b = cleaned[k].value;
            if b == b'\\' {
                let slash_at = cleaned[k].at;
                if k + 1 >= end {
                    return Err(EmailError::BadEscape { at: slash_at });
                }
                let escaped = cleaned[k + 1].value;
                if escaped != b'"' && escaped != b'\\' && escaped != b'@' {
                    return Err(EmailError::BadEscape { at: slash_at });
                }
                decoded.push(escaped);
                origins.push(slash_at);
                k += 2;
            } else {
                if !is_printable(b) {
                    return Err(EmailError::QuotedChar { at: cleaned[k].at });
                }
                decoded.push(b);
                origins.push(cleaned[k].at);
                k += 1;
            }
        }

        if decoded.is_empty() {
            return Err(EmailError::EmptyLocal {
                at: cleaned[end].at,
            });
        }
        if decoded.len() > 64 {
            return Err(EmailError::LocalTooLong { at: origins[64] });
        }

        (String::from_utf8(decoded).unwrap(), true)
    } else {
        if local_bytes.len() > 64 {
            return Err(EmailError::LocalTooLong {
                at: local_bytes[64].at,
            });
        }

        for (idx, byte) in local_bytes.iter().enumerate() {
            if byte.value == b'.' {
                if idx == 0
                    || idx + 1 == local_bytes.len()
                    || (idx > 0 && local_bytes[idx - 1].value == b'.')
                {
                    return Err(EmailError::LocalDot { at: byte.at });
                }
            } else if !is_atom_char(byte.value) {
                return Err(EmailError::LocalChar { at: byte.at });
            }
        }

        (
            String::from_utf8(local_bytes.iter().map(|b| b.value).collect()).unwrap(),
            false,
        )
    };

    let mut domain: Vec<Byte> = cleaned[sep + 1..].to_vec();
    if domain.last().map(|b| b.value) == Some(b'.') {
        domain.pop();
    }

    if domain.is_empty() {
        return Err(EmailError::EmptyDomain { at: domain_start });
    }

    if domain.len() > 253 {
        return Err(EmailError::DomainTooLong {
            at: domain[253].at,
        });
    }

    let mut labels: Vec<&[Byte]> = Vec::new();
    let mut label_start = 0usize;
    for idx in 0..=domain.len() {
        if idx == domain.len() || domain[idx].value == b'.' {
            if idx == label_start {
                let at = if idx < domain.len() {
                    domain[idx].at
                } else if cleaned.last().map(|b| b.value) == Some(b'.') {
                    cleaned.last().unwrap().at
                } else {
                    input.len()
                };
                return Err(EmailError::EmptyLabel { at });
            }
            labels.push(&domain[label_start..idx]);
            label_start = idx + 1;
        }
    }

    for label in &labels {
        if label.len() > 63 {
            return Err(EmailError::LabelTooLong { at: label[63].at });
        }
        if label.first().unwrap().value == b'-' {
            return Err(EmailError::LabelHyphen {
                at: label.first().unwrap().at,
            });
        }
        if label.last().unwrap().value == b'-' {
            return Err(EmailError::LabelHyphen {
                at: label.last().unwrap().at,
            });
        }
        for byte in *label {
            if !byte.value.is_ascii_alphanumeric() && byte.value != b'-' {
                return Err(EmailError::DomainChar { at: byte.at });
            }
        }
    }

    if labels.len() < 2 {
        return Err(EmailError::DomainNeedsDot { at: domain_start });
    }

    let last_label = labels.last().unwrap();
    if last_label.iter().all(|b| b.value.is_ascii_digit()) {
        return Err(EmailError::NumericTld {
            at: last_label[0].at,
        });
    }

    let domain_string = domain
        .iter()
        .map(|b| (b.value as char).to_ascii_lowercase())
        .collect();

    Ok(Address {
        local,
        domain: domain_string,
        quoted,
    })
}