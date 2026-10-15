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
    clean_index: usize,
}

fn is_hws(b: u8) -> bool {
    b == b' ' || b == b'\t'
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

fn cleanup(input: &[u8]) -> Result<(Vec<u8>, Vec<usize>, Vec<Run>), EmailError> {
    let mut clean = Vec::with_capacity(input.len());
    let mut positions = Vec::with_capacity(input.len());
    let mut runs = Vec::new();
    let mut i = 0;
    let mut first_retained = true;

    while i < input.len() {
        if first_retained && input[i] == b'"' {
            loop {
                if i >= input.len() {
                    break;
                }
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
                    break;
                }
            }
            first_retained = false;
            continue;
        }

        let start = i;
        let clean_index = clean.len();
        let mut depth = 0usize;
        let mut outer = None;
        let mut consumed = false;

        loop {
            if i >= input.len() {
                break;
            }
            let b = input[i];

            if depth == 0 {
                if is_hws(b) {
                    consumed = true;
                    i += 1;
                    continue;
                }
                if b == b'\r' || b == b'\n' {
                    consumed = true;
                    if let Some(end) = fold_end(input, i) {
                        i = end;
                        continue;
                    }
                    return Err(EmailError::BadFold { at: i });
                }
                if b == b'(' {
                    consumed = true;
                    outer = Some(i);
                    depth = 1;
                    i += 1;
                    continue;
                }
                if b == b')' {
                    return Err(EmailError::UnexpectedClose { at: i });
                }
                break;
            }

            match b {
                b'\\' => {
                    let slash = i;
                    if i + 1 >= input.len() || !(0x20..=0x7e).contains(&input[i + 1]) {
                        return Err(EmailError::CommentEscape { at: slash });
                    }
                    i += 2;
                }
                b'\r' | b'\n' => {
                    if let Some(end) = fold_end(input, i) {
                        i = end;
                    } else {
                        return Err(EmailError::BadFold { at: i });
                    }
                }
                b'(' => {
                    if depth == 3 {
                        return Err(EmailError::CommentDepth { at: i });
                    }
                    depth += 1;
                    i += 1;
                }
                b')' => {
                    depth -= 1;
                    i += 1;
                }
                b'\t' | 0x20..=0x7e => i += 1,
                _ => return Err(EmailError::CommentChar { at: i }),
            }
        }

        if depth != 0 {
            return Err(EmailError::UnclosedComment {
                at: outer.unwrap_or(start),
            });
        }

        if consumed {
            runs.push(Run { start, clean_index });
        } else {
            clean.push(input[i]);
            positions.push(i);
            i += 1;
            first_retained = false;
        }
    }

    Ok((clean, positions, runs))
}

fn mapped(positions: &[usize], input_len: usize, index: usize) -> usize {
    positions.get(index).copied().unwrap_or(input_len)
}

pub fn parse_email(input: &str) -> Result<Address, EmailError> {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return Err(EmailError::EmptyInput);
    }
    if let Some(at) = bytes.iter().position(|&b| b >= 128) {
        return Err(EmailError::NonAscii { at });
    }

    let (clean, positions, runs) = cleanup(bytes)?;

    for run in runs {
        let at_boundary = run.clean_index == 0 || run.clean_index == clean.len();
        let adjacent_at = (run.clean_index > 0 && clean[run.clean_index - 1] == b'@')
            || clean.get(run.clean_index) == Some(&b'@');
        if !at_boundary && !adjacent_at {
            return Err(EmailError::CfwsBoundary { at: run.start });
        }
    }

    if clean.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let mut quote_close = None;
    if clean.first() == Some(&b'"') {
        let mut i = 1;
        while i < clean.len() {
            if clean[i] == b'\\' {
                i += 2;
            } else if clean[i] == b'"' {
                quote_close = Some(i);
                break;
            } else {
                i += 1;
            }
        }
        if quote_close.is_none() {
            return Err(EmailError::UnclosedQuote {
                at: mapped(&positions, bytes.len(), 0),
            });
        }
    }

    let mut separators = Vec::new();
    let mut i = 0;
    while i < clean.len() {
        if let Some(close) = quote_close {
            if i <= close {
                i = close + 1;
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
    if separators.len() > 1 {
        return Err(EmailError::MultipleAt {
            at: mapped(&positions, bytes.len(), separators[1]),
        });
    }

    let sep = separators[0];
    if sep == 0 {
        return Err(EmailError::EmptyLocal {
            at: mapped(&positions, bytes.len(), sep),
        });
    }

    let local_bytes = &clean[..sep];
    let (local, quoted) = if local_bytes.first() == Some(&b'"') {
        let close = quote_close.unwrap();
        if close + 1 < sep {
            return Err(EmailError::TrailingLocal {
                at: mapped(&positions, bytes.len(), close + 1),
            });
        }

        let mut decoded = Vec::new();
        let mut origins = Vec::new();
        let mut j = 1;
        while j < close {
            let b = clean[j];
            if b == b'\\' {
                let next = clean.get(j + 1).copied().unwrap_or(0);
                if next != b'"' && next != b'\\' && next != b'@' {
                    return Err(EmailError::BadEscape {
                        at: mapped(&positions, bytes.len(), j),
                    });
                }
                decoded.push(next);
                origins.push(mapped(&positions, bytes.len(), j));
                j += 2;
            } else {
                if !(0x20..=0x7e).contains(&b) {
                    return Err(EmailError::QuotedChar {
                        at: mapped(&positions, bytes.len(), j),
                    });
                }
                decoded.push(b);
                origins.push(mapped(&positions, bytes.len(), j));
                j += 1;
            }
        }

        if decoded.is_empty() {
            return Err(EmailError::EmptyLocal {
                at: mapped(&positions, bytes.len(), close),
            });
        }
        if decoded.len() > 64 {
            return Err(EmailError::LocalTooLong { at: origins[64] });
        }
        (String::from_utf8(decoded).unwrap(), true)
    } else {
        if local_bytes.len() > 64 {
            return Err(EmailError::LocalTooLong {
                at: mapped(&positions, bytes.len(), 64),
            });
        }
        for (idx, &b) in local_bytes.iter().enumerate() {
            if b == b'.' {
                if idx == 0 || idx + 1 == local_bytes.len() || local_bytes[idx - 1] == b'.' {
                    return Err(EmailError::LocalDot {
                        at: mapped(&positions, bytes.len(), idx),
                    });
                }
            } else if !b.is_ascii_alphanumeric() && !b"!#$%&'*+-/=?^_`{|}~".contains(&b) {
                return Err(EmailError::LocalChar {
                    at: mapped(&positions, bytes.len(), idx),
                });
            }
        }
        (String::from_utf8(local_bytes.to_vec()).unwrap(), false)
    };

    let domain_start = sep + 1;
    let mut domain_end = clean.len();
    let removed_root = domain_end > domain_start && clean[domain_end - 1] == b'.';
    if removed_root {
        domain_end -= 1;
    }

    if domain_start == domain_end {
        return Err(EmailError::EmptyDomain {
            at: mapped(&positions, bytes.len(), domain_start),
        });
    }

    if domain_end - domain_start > 253 {
        return Err(EmailError::DomainTooLong {
            at: mapped(&positions, bytes.len(), domain_start + 253),
        });
    }

    let mut labels = Vec::new();
    let mut label_start = domain_start;
    let mut k = domain_start;

    while k <= domain_end {
        if k == domain_end || clean[k] == b'.' {
            let label_end = k;
            if label_start == label_end {
                let empty_at = if k == domain_end && removed_root {
                    clean.len() - 1
                } else {
                    label_start
                };
                return Err(EmailError::EmptyLabel {
                    at: mapped(&positions, bytes.len(), empty_at),
                });
            }

            let len = label_end - label_start;
            if len > 63 {
                return Err(EmailError::LabelTooLong {
                    at: mapped(&positions, bytes.len(), label_start + 63),
                });
            }
            if clean[label_start] == b'-' {
                return Err(EmailError::LabelHyphen {
                    at: mapped(&positions, bytes.len(), label_start),
                });
            }
            if clean[label_end - 1] == b'-' {
                return Err(EmailError::LabelHyphen {
                    at: mapped(&positions, bytes.len(), label_end - 1),
                });
            }
            for p in label_start..label_end {
                if !clean[p].is_ascii_alphanumeric() && clean[p] != b'-' {
                    return Err(EmailError::DomainChar {
                        at: mapped(&positions, bytes.len(), p),
                    });
                }
            }
            labels.push((label_start, label_end));
            label_start = k + 1;
        }
        k += 1;
    }

    if labels.len() < 2 {
        return Err(EmailError::DomainNeedsDot {
            at: mapped(&positions, bytes.len(), domain_start),
        });
    }

    let (last_start, last_end) = *labels.last().unwrap();
    if clean[last_start..last_end].iter().all(u8::is_ascii_digit) {
        return Err(EmailError::NumericTld {
            at: mapped(&positions, bytes.len(), last_start),
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

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_atom_address() {
        assert_eq!(
            parse_email("Ada.Lovelace@Mail.Example"),
            Ok(Address {
                local: "Ada.Lovelace".into(),
                domain: "mail.example".into(),
                quoted: false,
            })
        );
    }

    #[test]
    fn public_quoted_local() {
        assert_eq!(
            parse_email("\"Ada Lovelace\\@home\"@Example.COM"),
            Ok(Address {
                local: "Ada Lovelace@home".into(),
                domain: "example.com".into(),
                quoted: true,
            })
        );
        assert_eq!(
            parse_email("\"Ada Lovelace@home\"@example.com"),
            Ok(Address {
                local: "Ada Lovelace@home".into(),
                domain: "example.com".into(),
                quoted: true,
            })
        );
    }

    #[test]
    fn public_comments_and_fold() {
        assert_eq!(
            parse_email("(name) User (note) @\r\n\tMail.Example (end)"),
            Ok(Address {
                local: "User".into(),
                domain: "mail.example".into(),
                quoted: false,
            })
        );
    }

    #[test]
    fn public_boundary_error() {
        assert_eq!(
            parse_email("first last@mail.example"),
            Err(EmailError::CfwsBoundary { at: 5 })
        );
    }

    #[test]
    fn terminal_root_dot_preserves_empty_label_position() {
        assert_eq!(
            parse_email("a@example.."),
            Err(EmailError::EmptyLabel { at: 10 })
        );
    }
}