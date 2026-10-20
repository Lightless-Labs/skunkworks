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
struct Gap {
    start: usize,
    end: usize,
}

fn horizontal(b: u8) -> bool {
    matches!(b, b' ' | b'\t')
}

fn folded_end(b: &[u8], p: usize) -> Option<usize> {
    if b.get(p) != Some(&b'\r') || b.get(p + 1) != Some(&b'\n') {
        return None;
    }
    let mut q = p + 2;
    if !b.get(q).copied().map(horizontal).unwrap_or(false) {
        return None;
    }
    while b.get(q).copied().map(horizontal).unwrap_or(false) {
        q += 1;
    }
    Some(q)
}

fn append_gap(gaps: &mut Vec<Gap>, start: usize, end: usize) {
    if let Some(g) = gaps.last_mut() {
        if g.end == start {
            g.end = end;
            return;
        }
    }
    gaps.push(Gap { start, end });
}

fn strip_cfws(src: &[u8]) -> Result<(Vec<u8>, Vec<usize>, Vec<Gap>), EmailError> {
    let mut kept = Vec::with_capacity(src.len());
    let mut offsets = Vec::with_capacity(src.len());
    let mut gaps = Vec::new();
    let mut p = 0usize;
    let mut first_is_quote = false;
    let mut quote_done = false;
    let mut escaped = false;

    while p < src.len() {
        if first_is_quote && !quote_done {
            kept.push(src[p]);
            offsets.push(p);
            if escaped {
                escaped = false;
            } else if src[p] == b'\\' {
                escaped = true;
            } else if src[p] == b'"' {
                quote_done = true;
            }
            p += 1;
            continue;
        }

        let c = src[p];
        if kept.is_empty() && !first_is_quote && c == b'"' {
            first_is_quote = true;
            kept.push(c);
            offsets.push(p);
            p += 1;
            continue;
        }

        if horizontal(c) {
            let begin = p;
            while p < src.len() && horizontal(src[p]) {
                p += 1;
            }
            append_gap(&mut gaps, begin, p);
            continue;
        }
        if c == b'\r' || c == b'\n' {
            let begin = p;
            p = folded_end(src, p).ok_or(EmailError::BadFold { at: p })?;
            append_gap(&mut gaps, begin, p);
            continue;
        }
        if c == b'(' {
            let begin = p;
            let outer = p;
            let mut stack = vec![p];
            p += 1;
            while p < src.len() {
                let x = src[p];
                if x == b'\\' {
                    let at = p;
                    match src.get(p + 1) {
                        Some(v) if (0x20..=0x7e).contains(v) => p += 2,
                        _ => return Err(EmailError::CommentEscape { at }),
                    }
                    continue;
                }
                if x == b'\r' || x == b'\n' {
                    p = folded_end(src, p).ok_or(EmailError::BadFold { at: p })?;
                    continue;
                }
                if x == b'(' {
                    if stack.len() == 3 {
                        return Err(EmailError::CommentDepth { at: p });
                    }
                    stack.push(p);
                    p += 1;
                    continue;
                }
                if x == b')' {
                    stack.pop();
                    p += 1;
                    if stack.is_empty() {
                        break;
                    }
                    continue;
                }
                if !(horizontal(x) || (0x20..=0x7e).contains(&x)) {
                    return Err(EmailError::CommentChar { at: p });
                }
                p += 1;
            }
            if !stack.is_empty() {
                return Err(EmailError::UnclosedComment { at: outer });
            }
            append_gap(&mut gaps, begin, p);
            continue;
        }
        if c == b')' {
            return Err(EmailError::UnexpectedClose { at: p });
        }
        kept.push(c);
        offsets.push(p);
        p += 1;
    }
    Ok((kept, offsets, gaps))
}

fn pos(offsets: &[usize], end: usize, i: usize) -> usize {
    offsets.get(i).copied().unwrap_or(end)
}

pub fn parse_email(input: &str) -> Result<Address, EmailError> {
    let src = input.as_bytes();
    if src.is_empty() {
        return Err(EmailError::EmptyInput);
    }
    if let Some(p) = src.iter().position(|x| *x >= 128) {
        return Err(EmailError::NonAscii { at: p });
    }

    let (b, offsets, gaps) = strip_cfws(src)?;
    for g in gaps {
        let at_edge = g.start == 0 || g.end == src.len();
        let adjacent_left_at = g.start > 0 && src[g.start - 1] == b'@';
        let adjacent_right_at = g.end < src.len() && src[g.end] == b'@';
        if !(at_edge || adjacent_left_at || adjacent_right_at) {
            return Err(EmailError::CfwsBoundary { at: g.start });
        }
    }
    if b.is_empty() {
        return Err(EmailError::EmptyInput);
    }

    let close = if b[0] == b'"' {
        let mut q = 1usize;
        let mut found = None;
        while q < b.len() {
            match b[q] {
                b'\\' => q = (q + 2).min(b.len()),
                b'"' => {
                    found = Some(q);
                    break;
                }
                _ => q += 1,
            }
        }
        match found {
            Some(q) => Some(q),
            None => return Err(EmailError::UnclosedQuote { at: pos(&offsets, input.len(), 0) }),
        }
    } else {
        None
    };

    let mut separator = None;
    let mut second = None;
    let mut i = 0usize;
    while i < b.len() {
        if let Some(q) = close {
            if i <= q {
                i = q + 1;
                continue;
            }
        }
        if b[i] == b'@' {
            if separator.is_none() {
                separator = Some(i);
            } else if second.is_none() {
                second = Some(i);
                break;
            }
        }
        i += 1;
    }
    let at = match separator {
        None => return Err(EmailError::MissingAt),
        Some(v) => v,
    };
    if let Some(v) = second {
        return Err(EmailError::MultipleAt { at: pos(&offsets, input.len(), v) });
    }
    if at == 0 {
        return Err(EmailError::EmptyLocal { at: pos(&offsets, input.len(), at) });
    }

    let is_quoted = b[0] == b'"';
    let local;
    if !is_quoted {
        if at > 64 {
            return Err(EmailError::LocalTooLong { at: pos(&offsets, input.len(), 64) });
        }
        let mut prev_period = false;
        for k in 0..at {
            let c = b[k];
            if c == b'.' {
                if k == 0 || k + 1 == at || prev_period {
                    return Err(EmailError::LocalDot { at: pos(&offsets, input.len(), k) });
                }
                prev_period = true;
            } else {
                prev_period = false;
                if !(c.is_ascii_alphanumeric() || b"!#$%&'*+-/=?^_`{|}~".contains(&c)) {
                    return Err(EmailError::LocalChar { at: pos(&offsets, input.len(), k) });
                }
            }
        }
        local = String::from_utf8(b[..at].to_vec()).unwrap();
    } else {
        let q = close.unwrap();
        if q + 1 != at {
            return Err(EmailError::TrailingLocal { at: pos(&offsets, input.len(), q + 1) });
        }
        let mut value = Vec::new();
        let mut origin = Vec::new();
        let mut k = 1usize;
        while k < q {
            if b[k] == b'\\' {
                if k + 1 >= q || !matches!(b[k + 1], b'"' | b'\\' | b'@') {
                    return Err(EmailError::BadEscape { at: offsets[k] });
                }
                value.push(b[k + 1]);
                origin.push(offsets[k]);
                k += 2;
            } else {
                if !(0x20..=0x7e).contains(&b[k]) {
                    return Err(EmailError::QuotedChar { at: offsets[k] });
                }
                value.push(b[k]);
                origin.push(offsets[k]);
                k += 1;
            }
        }
        if value.is_empty() {
            return Err(EmailError::EmptyLocal { at: offsets[q] });
        }
        if value.len() > 64 {
            return Err(EmailError::LocalTooLong { at: origin[64] });
        }
        local = String::from_utf8(value).unwrap();
    }

    let ds = at + 1;
    let mut de = b.len();
    let removed_root = if de > ds && b[de - 1] == b'.' {
        de -= 1;
        Some(de)
    } else {
        None
    };
    if ds == de {
        return Err(EmailError::EmptyDomain { at: pos(&offsets, input.len(), ds) });
    }
    if de - ds > 253 {
        return Err(EmailError::DomainTooLong { at: pos(&offsets, input.len(), ds + 253) });
    }

    let mut bounds = Vec::new();
    let mut start = ds;
    let mut k = ds;
    while k <= de {
        if k == de || b[k] == b'.' {
            if start == k {
                let error_at = if start == de {
                    removed_root.map(|r| pos(&offsets, input.len(), r)).unwrap_or_else(|| pos(&offsets, input.len(), start))
                } else {
                    pos(&offsets, input.len(), start)
                };
                return Err(EmailError::EmptyLabel { at: error_at });
            }
            if k - start > 63 {
                return Err(EmailError::LabelTooLong { at: pos(&offsets, input.len(), start + 63) });
            }
            if b[start] == b'-' {
                return Err(EmailError::LabelHyphen { at: pos(&offsets, input.len(), start) });
            }
            if b[k - 1] == b'-' {
                return Err(EmailError::LabelHyphen { at: pos(&offsets, input.len(), k - 1) });
            }
            for z in start..k {
                if !(b[z].is_ascii_alphanumeric() || b[z] == b'-') {
                    return Err(EmailError::DomainChar { at: pos(&offsets, input.len(), z) });
                }
            }
            bounds.push((start, k));
            start = k + 1;
        }
        k += 1;
    }
    if bounds.len() < 2 {
        return Err(EmailError::DomainNeedsDot { at: pos(&offsets, input.len(), ds) });
    }
    let (last_s, last_e) = bounds[bounds.len() - 1];
    if b[last_s..last_e].iter().all(u8::is_ascii_digit) {
        return Err(EmailError::NumericTld { at: pos(&offsets, input.len(), last_s) });
    }

    let mut domain = String::with_capacity(de - ds);
    for &x in &b[ds..de] {
        domain.push(x.to_ascii_lowercase() as char);
    }
    Ok(Address { local, domain, quoted: is_quoted })
}
