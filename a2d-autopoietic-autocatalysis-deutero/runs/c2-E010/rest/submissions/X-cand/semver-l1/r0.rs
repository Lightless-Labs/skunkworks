use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub pre: Vec<String>,
    pub build: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VersionError {
    Empty,
    MissingCore { at: usize },
    LeadingZero { at: usize },
    Overflow { at: usize },
    EmptyIdentifier { at: usize },
    InvalidChar { at: usize },
}

fn missing_or_invalid(s: &str, at: usize) -> VersionError {
    match s.as_bytes().get(at) {
        None | Some(b'.' | b'-' | b'+') => VersionError::MissingCore { at },
        _ => VersionError::InvalidChar { at },
    }
}

fn core(s: &str, pos: &mut usize) -> Result<u64, VersionError> {
    let bytes = s.as_bytes();
    let start = *pos;
    while bytes.get(*pos).is_some_and(u8::is_ascii_digit) {
        *pos += 1;
    }
    if start == *pos {
        return Err(missing_or_invalid(s, *pos));
    }
    let digits = &bytes[start..*pos];
    if digits.len() > 1 && digits[0] == b'0' {
        return Err(VersionError::LeadingZero { at: start });
    }
    let mut value = 0u64;
    for digit in digits {
        value = value.checked_mul(10)
            .and_then(|n| n.checked_add((digit - b'0') as u64))
            .ok_or(VersionError::Overflow { at: start })?;
    }
    Ok(value)
}

fn identifiers(
    s: &str,
    pos: &mut usize,
    stop_plus: bool,
    pre: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut result = Vec::new();
    loop {
        let start = *pos;
        while let Some(&b) = bytes.get(*pos) {
            if b == b'.' || (stop_plus && b == b'+') {
                break;
            }
            if !(b.is_ascii_alphanumeric() || b == b'-') {
                return Err(VersionError::InvalidChar { at: *pos });
            }
            *pos += 1;
        }
        if start == *pos {
            return Err(VersionError::EmptyIdentifier { at: start });
        }
        let id = &s[start..*pos];
        if pre && id.bytes().all(|b| b.is_ascii_digit())
            && id.len() > 1 && id.as_bytes()[0] == b'0'
        {
            return Err(VersionError::LeadingZero { at: start });
        }
        result.push(id.to_string());

        match bytes.get(*pos) {
            None => break,
            Some(b'+') if stop_plus => break,
            Some(b'.') => {
                *pos += 1;
                if *pos == bytes.len() || (stop_plus && bytes[*pos] == b'+') {
                    return Err(VersionError::EmptyIdentifier { at: *pos });
                }
            }
            _ => break,
        }
    }
    Ok(result)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }
    let mut pos = 0;
    let major = core(s, &mut pos)?;
    if s.as_bytes().get(pos) != Some(&b'.') {
        return Err(missing_or_invalid(s, pos));
    }
    pos += 1;
    let minor = core(s, &mut pos)?;
    if s.as_bytes().get(pos) != Some(&b'.') {
        return Err(missing_or_invalid(s, pos));
    }
    pos += 1;
    let patch = core(s, &mut pos)?;

    let mut pre = Vec::new();
    let mut build = Vec::new();
    match s.as_bytes().get(pos) {
        None => {}
        Some(b'-') => {
            pos += 1;
            pre = identifiers(s, &mut pos, true, true)?;
            if s.as_bytes().get(pos) == Some(&b'+') {
                pos += 1;
                build = identifiers(s, &mut pos, false, false)?;
            }
        }
        Some(b'+') => {
            pos += 1;
            build = identifiers(s, &mut pos, false, false)?;
        }
        _ => return Err(VersionError::InvalidChar { at: pos }),
    }

    Ok(Version { major, minor, patch, pre, build })
}

fn numeric(id: &str) -> bool {
    !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit())
}

fn pre_cmp(a: &str, b: &str) -> Ordering {
    match (numeric(a), numeric(b)) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (true, true) => a.len().cmp(&b.len()).then(a.as_bytes().cmp(b.as_bytes())),
        (false, false) => a.as_bytes().cmp(b.as_bytes()),
    }
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    let core_order = (a.major, a.minor, a.patch).cmp(&(b.major, b.minor, b.patch));
    if core_order != Ordering::Equal {
        return core_order;
    }
    match (a.pre.is_empty(), b.pre.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => {
            for (x, y) in a.pre.iter().zip(&b.pre) {
                let ord = pre_cmp(x, y);
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            a.pre.len().cmp(&b.pre.len())
        }
    }
}

#[derive(Clone, Debug)]
pub struct Range {
    comparators: Vec<Comparator>,
}

#[derive(Clone, Debug)]
struct Comparator {
    op: Op,
    version: Version,
}

#[derive(Clone, Copy, Debug)]
enum Op { Eq, Lt, Le, Gt, Ge }

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RangeError {
    Empty { at: usize },
    Syntax { at: usize },
    Version { at: usize, error: VersionError },
}

fn ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let bytes = s.as_bytes();
    let mut comparators = Vec::new();
    let mut pos = 0;
    while pos < bytes.len() {
        while pos < bytes.len() && ws(bytes[pos]) { pos += 1; }
        if pos == bytes.len() { break; }
        let start = pos;
        while pos < bytes.len() && !ws(bytes[pos]) { pos += 1; }
        let end = pos;

        let (op, operand_start) = match bytes[start] {
            b'=' => (Op::Eq, start + 1),
            b'<' if start + 1 < end && bytes[start + 1] == b'=' => (Op::Le, start + 2),
            b'<' => (Op::Lt, start + 1),
            b'>' if start + 1 < end && bytes[start + 1] == b'=' => (Op::Ge, start + 2),
            b'>' => (Op::Gt, start + 1),
            _ => (Op::Eq, start),
        };
        if operand_start >= end || !bytes[operand_start].is_ascii_digit() {
            return Err(RangeError::Syntax { at: start });
        }
        let version = parse_version(&s[operand_start..end]).map_err(|error| {
            RangeError::Version { at: operand_start, error }
        })?;
        comparators.push(Comparator { op, version });
    }
    if comparators.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }
    Ok(Range { comparators })
}

pub fn matches(v: &Version, r: &Range) -> bool {
    let admitted = v.pre.is_empty() || r.comparators.iter().any(|c| {
        !c.version.pre.is_empty()
            && (c.version.major, c.version.minor, c.version.patch)
                == (v.major, v.minor, v.patch)
    });
    admitted && r.comparators.iter().all(|c| {
        let ord = compare_versions(v, &c.version);
        match c.op {
            Op::Eq => ord == Ordering::Equal,
            Op::Lt => ord == Ordering::Less,
            Op::Le => ord != Ordering::Greater,
            Op::Gt => ord == Ordering::Greater,
            Op::Ge => ord != Ordering::Less,
        }
    })
}
