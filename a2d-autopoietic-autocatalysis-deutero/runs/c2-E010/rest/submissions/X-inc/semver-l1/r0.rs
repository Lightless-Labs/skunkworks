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

fn is_digit(b: u8) -> bool {
    b.is_ascii_digit()
}

fn parse_core_field(bytes: &[u8], pos: &mut usize) -> Result<u64, VersionError> {
    let start = *pos;
    while *pos < bytes.len() && is_digit(bytes[*pos]) {
        *pos += 1;
    }

    if *pos == start {
        return Err(match bytes.get(start) {
            None | Some(b'.' | b'-' | b'+') => VersionError::MissingCore { at: start },
            _ => VersionError::InvalidChar { at: start },
        });
    }

    if *pos - start > 1 && bytes[start] == b'0' {
        return Err(VersionError::LeadingZero { at: start });
    }

    let mut value = 0u64;
    for &b in &bytes[start..*pos] {
        value = value
            .checked_mul(10)
            .and_then(|n| n.checked_add((b - b'0') as u64))
            .ok_or(VersionError::Overflow { at: start })?;
    }
    Ok(value)
}

fn parse_identifiers(
    bytes: &[u8],
    pos: &mut usize,
    stop_at_plus: bool,
    pre: bool,
) -> Result<Vec<String>, VersionError> {
    let mut identifiers = Vec::new();

    loop {
        let start = *pos;
        while *pos < bytes.len()
            && bytes[*pos] != b'.'
            && !(stop_at_plus && bytes[*pos] == b'+')
        {
            let b = bytes[*pos];
            if !(b.is_ascii_alphanumeric() || b == b'-') {
                return Err(VersionError::InvalidChar { at: *pos });
            }
            *pos += 1;
        }

        if *pos == start {
            return Err(VersionError::EmptyIdentifier { at: start });
        }

        let identifier = &bytes[start..*pos];
        if pre
            && identifier.len() > 1
            && identifier.iter().all(u8::is_ascii_digit)
            && identifier[0] == b'0'
        {
            return Err(VersionError::LeadingZero { at: start });
        }

        identifiers.push(String::from_utf8(identifier.to_vec()).expect("ASCII identifier"));

        if *pos < bytes.len() && bytes[*pos] == b'.' {
            *pos += 1;
            continue;
        }
        break;
    }

    Ok(identifiers)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }

    let bytes = s.as_bytes();
    let mut pos = 0;

    let major = parse_core_field(bytes, &mut pos)?;
    if pos >= bytes.len() || bytes[pos] != b'.' {
        return Err(match bytes.get(pos) {
            None | Some(b'-' | b'+') => VersionError::MissingCore { at: pos },
            _ => VersionError::InvalidChar { at: pos },
        });
    }
    pos += 1;

    let minor = parse_core_field(bytes, &mut pos)?;
    if pos >= bytes.len() || bytes[pos] != b'.' {
        return Err(match bytes.get(pos) {
            None | Some(b'-' | b'+') => VersionError::MissingCore { at: pos },
            _ => VersionError::InvalidChar { at: pos },
        });
    }
    pos += 1;

    let patch = parse_core_field(bytes, &mut pos)?;
    let mut pre = Vec::new();
    let mut build = Vec::new();

    if pos < bytes.len() && bytes[pos] == b'-' {
        pos += 1;
        pre = parse_identifiers(bytes, &mut pos, true, true)?;
    }

    if pos < bytes.len() && bytes[pos] == b'+' {
        pos += 1;
        build = parse_identifiers(bytes, &mut pos, false, false)?;
    }

    if pos != bytes.len() {
        return Err(VersionError::InvalidChar { at: pos });
    }

    Ok(Version {
        major,
        minor,
        patch,
        pre,
        build,
    })
}

fn is_numeric_identifier(s: &str) -> bool {
    !s.is_empty() && s.as_bytes().iter().all(u8::is_ascii_digit)
}

fn compare_numeric_identifiers(a: &str, b: &str) -> Ordering {
    a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes()))
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    let core = (a.major, a.minor, a.patch).cmp(&(b.major, b.minor, b.patch));
    if core != Ordering::Equal {
        return core;
    }

    match (a.pre.is_empty(), b.pre.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => {
            for (left, right) in a.pre.iter().zip(&b.pre) {
                let left_numeric = is_numeric_identifier(left);
                let right_numeric = is_numeric_identifier(right);
                let ordering = match (left_numeric, right_numeric) {
                    (true, true) => compare_numeric_identifiers(left, right),
                    (true, false) => Ordering::Less,
                    (false, true) => Ordering::Greater,
                    (false, false) => left.as_bytes().cmp(right.as_bytes()),
                };
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
            a.pre.len().cmp(&b.pre.len())
        }
    }
}

#[derive(Clone, Debug)]
struct Comparator {
    op: CompareOp,
    version: Version,
}

#[derive(Clone, Debug)]
enum CompareOp {
    Eq,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Debug)]
pub struct Range {
    arms: Vec<Vec<Comparator>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RangeError {
    Empty { at: usize },
    Syntax { at: usize },
    Version { at: usize, error: VersionError },
}

fn is_range_whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let bytes = s.as_bytes();
    let mut pos = 0;
    let mut comparators = Vec::new();

    while pos < bytes.len() && is_range_whitespace(bytes[pos]) {
        pos += 1;
    }
    if pos == bytes.len() {
        return Err(RangeError::Empty { at: 0 });
    }

    while pos < bytes.len() {
        let token_start = pos;
        while pos < bytes.len() && !is_range_whitespace(bytes[pos]) {
            pos += 1;
        }
        let token_end = pos;

        let (op, operand_offset) = match bytes[token_start] {
            b'=' => (CompareOp::Eq, 1),
            b'<' if bytes.get(token_start + 1) == Some(&b'=') => (CompareOp::Le, 2),
            b'<' => (CompareOp::Lt, 1),
            b'>' if bytes.get(token_start + 1) == Some(&b'=') => (CompareOp::Ge, 2),
            b'>' => (CompareOp::Gt, 1),
            _ => (CompareOp::Eq, 0),
        };

        let operand_start = token_start + operand_offset;
        if operand_start >= token_end || !bytes[operand_start].is_ascii_digit() {
            return Err(RangeError::Syntax { at: token_start });
        }

        let operand = &s[operand_start..token_end];
        let version = parse_version(operand).map_err(|error| RangeError::Version {
            at: operand_start,
            error,
        })?;
        comparators.push(Comparator { op, version });

        while pos < bytes.len() && is_range_whitespace(bytes[pos]) {
            pos += 1;
        }
    }

    Ok(Range {
        arms: vec![comparators],
    })
}

pub fn matches(v: &Version, r: &Range) -> bool {
    let candidate_core = (v.major, v.minor, v.patch);

    r.arms.iter().any(|arm| {
        if !v.pre.is_empty()
            && !arm.iter().any(|comparator| {
                !comparator.version.pre.is_empty()
                    && (
                        comparator.version.major,
                        comparator.version.minor,
                        comparator.version.patch,
                    ) == candidate_core
            })
        {
            return false;
        }

        arm.iter().all(|comparator| {
            let ordering = compare_versions(v, &comparator.version);
            match comparator.op {
                CompareOp::Eq => ordering == Ordering::Equal,
                CompareOp::Lt => ordering == Ordering::Less,
                CompareOp::Le => ordering != Ordering::Greater,
                CompareOp::Gt => ordering == Ordering::Greater,
                CompareOp::Ge => ordering != Ordering::Less,
            }
        })
    })
}