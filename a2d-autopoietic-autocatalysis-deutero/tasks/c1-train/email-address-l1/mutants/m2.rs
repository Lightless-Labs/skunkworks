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
}

fn offset(map: &[usize], i: usize, end: usize) -> usize {
    map.get(i).copied().unwrap_or(end)
}

fn atext(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$%&'*+-/=?^_`{|}~".contains(&b)
}

fn atom_ok(bytes: &[u8]) -> bool {
    !bytes.is_empty() && bytes.len() <= 64 &&
    bytes.iter().enumerate().all(|(i, &b)| {
        if b == b'.' { i != 0 && i + 1 != bytes.len() && bytes[i - 1] != b'.' }
        else { atext(b) }
    })
}

fn parse_core(input: &str, allow_single: bool) -> Result<(Address, Vec<usize>), EmailError> {
    use EmailError::*;
    let raw = input.as_bytes();
    if raw.is_empty() { return Err(EmptyInput); }
    if let Some(at) = raw.iter().position(|b| !b.is_ascii()) {
        return Err(NonAscii { at });
    }
    let bytes = raw.to_vec();
    let map: Vec<usize> = (0..raw.len()).collect();
    let at_pos = |i| offset(&map, i, input.len());
    if bytes.is_empty() { return Err(EmptyInput); }
    
    let ats: Vec<usize> = bytes.iter().enumerate()
        .filter_map(|(i, &b)| (b == b'@' && true).then_some(i)).collect();
    if ats.is_empty() { return Err(MissingAt); }
    if ats.len() > 1 { return Err(MultipleAt { at: at_pos(ats[1]) }); }
    let sep = ats[0];
    if sep == 0 { return Err(EmptyLocal { at: at_pos(0) }); }
    let local_bytes = &bytes[..sep];
    let mut local_map = Vec::new();
    
    let (local, quoted) = {
        if local_bytes.len() > 64 { return Err(LocalTooLong { at: at_pos(64) }); }
        for (i, &b) in local_bytes.iter().enumerate() {
            if b == b'.' {
                if i == 0 || i + 1 == local_bytes.len() || local_bytes[i - 1] == b'.' {
                    return Err(LocalDot { at: at_pos(i) });
                }
            } else if !atext(b) { return Err(LocalChar { at: at_pos(i) }); }
            local_map.push(at_pos(i));
        }
        (String::from_utf8(local_bytes.to_vec()).unwrap(), false)
    };
    
    let domain_start = sep + 1;
    let mut domain_end = bytes.len();
    if domain_end > domain_start && bytes[domain_end - 1] == b'.' { domain_end -= 1; }
    if domain_start == domain_end { return Err(EmptyDomain { at: at_pos(domain_start) }); }
    if domain_end - domain_start > 253 {
        return Err(DomainTooLong { at: at_pos(domain_start + 253) });
    }
    let mut start = domain_start;
    let mut labels = 0;
    let mut last_start = start;
    loop {
        let end = (start..domain_end).find(|&i| bytes[i] == b'.').unwrap_or(domain_end);
        if end == start { return Err(EmptyLabel { at: at_pos(start) }); }
        if end - start > 63 { return Err(LabelTooLong { at: at_pos(start + 63) }); }
        if bytes[end - 1] == b'-' { return Err(LabelHyphen { at: at_pos(end - 1) }); }
        if bytes[start] == b'-' { return Err(LabelHyphen { at: at_pos(start) }); }
        for i in start..end {
            if !bytes[i].is_ascii_alphanumeric() && bytes[i] != b'-' {
                return Err(DomainChar { at: at_pos(i) });
            }
        }
        labels += 1;
        last_start = start;
        if end == domain_end { break; }
        start = end + 1;
    }
    if labels < 2 && !allow_single { return Err(DomainNeedsDot { at: at_pos(domain_start) }); }
    if bytes[last_start..domain_end].iter().all(|b| b.is_ascii_digit()) {
        return Err(NumericTld { at: at_pos(last_start) });
    }
    let domain = String::from_utf8(bytes[domain_start..domain_end].to_vec()).unwrap().to_ascii_lowercase();
    Ok((Address { local, domain, quoted }, local_map))
}

pub fn parse_email(input: &str) -> Result<Address, EmailError> {
    parse_core(input, false).map(|(address, _)| address)
}
