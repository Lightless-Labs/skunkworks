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

#[derive(Debug)]
struct Cleaned {
    bytes: Vec<u8>,
    positions: Vec<usize>,
    runs: Vec<usize>,
}

fn is_hws(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

fn printable(b: u8) -> bool {
    (0x20..=0x7e).contains(&b)
}

fn clean(input: &[u8]) -> Result<Cleaned, EmailError> {
    let mut bytes = Vec::with_capacity(input.len());
    let mut positions = Vec::with_capacity(input.len());
    let mut runs = Vec::new();
    let mut i = 0usize;
    let mut comment_stack: Vec<usize> = Vec::new();
    let mut protected_quote = false;
    let mut quote_escape = false;
    let mut in_run = false;

    while i < input.len() {
        let b = input[i];

        if protected_quote {
            bytes.push(b);
            positions.push(i);
            if quote_escape {
                quote_escape = false;
            } else if b == b'\\' {
                quote_escape = true;
            } else if b == b'"' {
                protected_quote = false;
            }
            i += 1;
            continue;
        }

        if !comment_stack.is_empty() {
            if b == b'\\' {
                if i + 1 >= input.len() || !printable(input[i + 1]) {
                    return Err(EmailError::CommentEscape { at: i });
                }
                in_run = true;
                i += 2;
                continue;
            }
            if b == b'\r' || b == b'\n' {
                if b != b'\r'
                    || i + 2 >= input.len()
                    || input[i + 1] != b'\n'
                    || !is_hws(input[i + 2])
                {
                    return Err(EmailError::BadFold { at: i });
                }
                in_run = true;
                i += 2;
                while i < input.len() && is_hws(input[i]) {
                    i += 1;
                }
                continue;
            }
            if b == b'(' {
                if comment_stack.len() == 3 {
                    return Err(EmailError::CommentDepth { at: i });
                }
                comment_stack.push(i);
                in_run = true;
                i += 1;
                continue;
            }
            if b == b')' {
                comment_stack.pop();
                in_run = true;
                i += 1;
                continue;
            }
            if !printable(b) && b != b'\t' {
                return Err(EmailError::CommentChar { at: i });
            }
            in_run = true;
            i += 1;
            continue;
        }

        if b == b'\r' || b == b'\n' {
            if b != b'\r'
                || i + 2 >= input.len()
                || input[i + 1] != b'\n'
                || !is_hws(input[i + 2])
            {
                return Err(EmailError::BadFold { at: i });
            }
            if !in_run {
                runs.push(i);
            }
            in_run = true;
            i += 2;
            while i < input.len() && is_hws(input[i]) {
                i += 1;
            }
            continue;
        }

        if is_hws(b) {
            if !in_run {
                runs.push(i);
            }
            in_run = true;
            i += 1;
            continue;
        }

        if b == b'(' {
            if !in_run {
                runs.push(i);
            }
            in_run = true;
            comment_stack.push(i);
            i += 1;
            continue;
        }

        if b == b')' {
            return Err(EmailError::UnexpectedClose { at: i });
        }

        if b == b'"' && bytes.is_empty() {
            bytes.push(b);
            positions.push(i);
            protected_quote = true;
            quote_escape = false;
            in_run = false;
            i += 1;
            continue;
        }

        bytes.push(b);
        positions.push(i);
        in_run = false;
        i += 1;
    }

    if let Some(&outermost) = comment_stack.first() {
        return Err(EmailError::UnclosedComment { at: outermost });
    }

    Ok(Cleaned {
        bytes,
        positions,
        runs,
    })
}

fn origin(positions: &[usize], index: usize, input_len: usize) -> usize {
    positions.get(index).copied().unwrap_or(input_len)
}

fn parse_email(input: &str) -> Result<Address, EmailError> {
    let raw = input.as_bytes();
    if raw.is_empty() {
        return Err(EmailError::EmptyInput);
    }
    if let Some(at) = raw.iter().position(|&b| b >= 128) {
        return Err(EmailError::NonAscii { at });
    }

    let cleaned = clean(raw)?;
    for &run_at in &cleaned.runs {
        let before = cleaned
            .positions
            .iter()
            .position(|&p| p >= run_at)
            .unwrap_or(cleaned.bytes.len());
        let after = before;
        let legal = before == 0
            || before == cleaned.bytes.len()
            || cleaned.bytes.get(before.wrapping_sub(1)) == Some(&b'@')
            || cleaned.bytes.get(after) == Some(&b'@');
        if !legal {
            return Err(EmailError::CfwsBoundary { at: run_at });
        }
    }
    if cleaned.bytes.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let b = &cleaned.bytes;
    let p = &cleaned.positions;
    let len = b.len();
    let mut quoted_end = None;

    if b[0] == b'"' {
        let mut i = 1;
        let mut closed = None;
        while i < len {
            if b[i] == b'\\' {
                i += 2;
            } else if b[i] == b'"' {
                closed = Some(i);
                break;
            } else {
                i += 1;
            }
        }
        match closed {
            Some(end) => quoted_end = Some(end),
            None => return Err(EmailError::UnclosedQuote { at: p[0] }),
        }
    }

    let mut separators = Vec::new();
    let mut i = 0;
    while i < len {
        if let Some(end) = quoted_end {
            if i <= end {
                i = end + 1;
                continue;
            }
        }
        if b[i] == b'@' {
            separators.push(i);
        }
        i += 1;
    }

    if separators.is_empty() {
        return Err(EmailError::MissingAt);
    }
    if separators.len() >= 2 {
        return Err(EmailError::MultipleAt {
            at: p[separators[1]],
        });
    }

    let sep = separators[0];
    if sep == 0 {
        return Err(EmailError::EmptyLocal { at: p[sep] });
    }

    let local: String;
    let is_quoted = b[0] == b'"';
    if !is_quoted {
        if sep > 64 {
            return Err(EmailError::LocalTooLong { at: p[64] });
        }
        for i in 0..sep {
            let ch = b[i];
            if ch == b'.' {
                if i == 0 || i + 1 == sep || b[i - 1] == b'.' {
                    return Err(EmailError::LocalDot { at: p[i] });
                }
            } else if !(ch.is_ascii_alphanumeric()
                || b"!#$%&'*+-/=?^_`{|}~".contains(&ch))
            {
                return Err(EmailError::LocalChar { at: p[i] });
            }
        }
        local = String::from_utf8(b[..sep].to_vec()).unwrap();
    } else {
        let end = quoted_end.unwrap();
        if end + 1 != sep {
            return Err(EmailError::TrailingLocal {
                at: origin(p, end + 1, raw.len()),
            });
        }
        let mut decoded = Vec::new();
        let mut origins = Vec::new();
        let mut i = 1;
        while i < end {
            if b[i] == b'\\' {
                if i + 1 >= end || !matches!(b[i + 1], b'"' | b'\\' | b'@') {
                    return Err(EmailError::BadEscape { at: p[i] });
                }
                decoded.push(b[i + 1]);
                origins.push(p[i]);
                i += 2;
            } else {
                if !printable(b[i]) {
                    return Err(EmailError::QuotedChar { at: p[i] });
                }
                decoded.push(b[i]);
                origins.push(p[i]);
                i += 1;
            }
        }
        if decoded.is_empty() {
            return Err(EmailError::EmptyLocal { at: p[end] });
        }
        if decoded.len() > 64 {
            return Err(EmailError::LocalTooLong { at: origins[64] });
        }
        local = String::from_utf8(decoded).unwrap();
    }

    let domain_start = sep + 1;
    let mut domain_end = len;
    let root_dot = if domain_end > domain_start && b[domain_end - 1] == b'.' {
        domain_end -= 1;
        Some(domain_end)
    } else {
        None
    };

    if domain_start == domain_end {
        return Err(EmailError::EmptyDomain {
            at: origin(p, domain_start, raw.len()),
        });
    }
    let domain_len = domain_end - domain_start;
    if domain_len > 253 {
        return Err(EmailError::DomainTooLong { at: p[domain_start + 253] });
    }

    let mut labels: Vec<(usize, usize)> = Vec::new();
    let mut start = domain_start;
    for i in domain_start..=domain_end {
        if i == domain_end || b[i] == b'.' {
            if start == i {
                let at = if i == domain_end {
                    root_dot.map(|d| p[d]).unwrap_or_else(|| origin(p, start, raw.len()))
                } else {
                    p[i]
                };
                return Err(EmailError::EmptyLabel { at });
            }
            let size = i - start;
            if size > 63 {
                return Err(EmailError::LabelTooLong { at: p[start + 63] });
            }
            if b[start] == b'-' {
                return Err(EmailError::LabelHyphen { at: p[start] });
            }
            if b[i - 1] == b'-' {
                return Err(EmailError::LabelHyphen { at: p[i - 1] });
            }
            for j in start..i {
                if !(b[j].is_ascii_alphanumeric() || b[j] == b'-') {
                    return Err(EmailError::DomainChar { at: p[j] });
                }
            }
            labels.push((start, i));
            start = i + 1;
        }
    }

    if labels.len() < 2 {
        return Err(EmailError::DomainNeedsDot {
            at: origin(p, domain_start, raw.len()),
        });
    }
    let (last_start, last_end) = *labels.last().unwrap();
    if b[last_start..last_end].iter().all(u8::is_ascii_digit) {
        return Err(EmailError::NumericTld { at: p[last_start] });
    }

    let domain = String::from_utf8(
        b[domain_start..domain_end]
            .iter()
            .map(u8::to_ascii_lowercase)
            .collect(),
    )
    .unwrap();

    Ok(Address {
        local,
        domain,
        quoted: is_quoted,
    })
}

pub fn main() {}