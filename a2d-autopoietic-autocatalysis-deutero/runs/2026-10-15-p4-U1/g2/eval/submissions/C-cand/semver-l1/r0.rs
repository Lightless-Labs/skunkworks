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

fn parse_core_field(bytes: &[u8], pos: &mut usize) -> Result<u64, VersionError> {
    let start = *pos;

    while *pos < bytes.len() && bytes[*pos].is_ascii_digit() {
        *pos += 1;
    }

    if *pos == start {
        if *pos == bytes.len() || matches!(bytes[*pos], b'.' | b'-' | b'+') {
            return Err(VersionError::MissingCore { at: start });
        }
        return Err(VersionError::InvalidChar { at: start });
    }

    let digits = &bytes[start..*pos];
    if digits.len() > 1 && digits[0] == b'0' {
        return Err(VersionError::LeadingZero { at: start });
    }

    let mut value = 0u64;
    for &digit in digits {
        value = value
            .checked_mul(10)
            .and_then(|n| n.checked_add((digit - b'0') as u64))
            .ok_or(VersionError::Overflow { at: start })?;
    }

    Ok(value)
}

fn parse_identifiers(
    bytes: &[u8],
    pos: &mut usize,
    pre: bool,
) -> Result<Vec<String>, VersionError> {
    let mut identifiers = Vec::new();

    loop {
        let start = *pos;

        while *pos < bytes.len()
            && bytes[*pos] != b'.'
            && !(pre && bytes[*pos] == b'+')
        {
            let byte = bytes[*pos];
            if !(byte.is_ascii_alphanumeric() || byte == b'-') {
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
            && identifier[0] == b'0'
            && identifier.iter().all(u8::is_ascii_digit)
        {
            return Err(VersionError::LeadingZero { at: start });
        }

        identifiers.push(String::from_utf8(identifier.to_vec()).unwrap());

        if *pos >= bytes.len() || (pre && bytes[*pos] == b'+') {
            break;
        }

        *pos += 1;
        if *pos == bytes.len() || (pre && bytes[*pos] == b'+') {
            return Err(VersionError::EmptyIdentifier { at: *pos });
        }
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

    if pos >= bytes.len() || matches!(bytes[pos], b'-' | b'+') {
        return Err(VersionError::MissingCore { at: pos });
    }
    if bytes[pos] != b'.' {
        return Err(VersionError::InvalidChar { at: pos });
    }
    pos += 1;

    let minor = parse_core_field(bytes, &mut pos)?;

    if pos >= bytes.len() || matches!(bytes[pos], b'-' | b'+') {
        return Err(VersionError::MissingCore { at: pos });
    }
    if bytes[pos] != b'.' {
        return Err(VersionError::InvalidChar { at: pos });
    }
    pos += 1;

    let patch = parse_core_field(bytes, &mut pos)?;

    let mut pre = Vec::new();
    let mut build = Vec::new();

    if pos < bytes.len() {
        match bytes[pos] {
            b'-' => {
                pos += 1;
                pre = parse_identifiers(bytes, &mut pos, true)?;
                if pos < bytes.len() && bytes[pos] == b'+' {
                    pos += 1;
                    build = parse_identifiers(bytes, &mut pos, false)?;
                }
            }
            b'+' => {
                pos += 1;
                build = parse_identifiers(bytes, &mut pos, false)?;
            }
            _ => return Err(VersionError::InvalidChar { at: pos }),
        }
    }

    Ok(Version {
        major,
        minor,
        patch,
        pre,
        build,
    })
}

fn is_numeric_identifier(identifier: &str) -> bool {
    !identifier.is_empty() && identifier.bytes().all(|b| b.is_ascii_digit())
}

fn compare_pre_identifier(a: &str, b: &str) -> Ordering {
    let a_numeric = is_numeric_identifier(a);
    let b_numeric = is_numeric_identifier(b);

    match (a_numeric, b_numeric) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (true, true) => a
            .len()
            .cmp(&b.len())
            .then_with(|| a.as_bytes().cmp(b.as_bytes())),
        (false, false) => a.as_bytes().cmp(b.as_bytes()),
    }
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    (a.major, a.minor, a.patch)
        .cmp(&(b.major, b.minor, b.patch))
        .then_with(|| match (a.pre.is_empty(), b.pre.is_empty()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            (false, false) => {
                for (left, right) in a.pre.iter().zip(&b.pre) {
                    let ordering = compare_pre_identifier(left, right);
                    if ordering != Ordering::Equal {
                        return ordering;
                    }
                }
                a.pre.len().cmp(&b.pre.len())
            }
        })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RangeError {
    Empty { at: usize },
    Syntax { at: usize },
    Version { at: usize, error: VersionError },
}

#[derive(Clone, Copy, Debug)]
enum Operator {
    Equal,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

#[derive(Clone, Debug)]
struct Comparator {
    op: Operator,
    version: Version,
}

#[derive(Clone, Debug)]
pub struct Range {
    arms: Vec<Vec<Comparator>>,
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let bytes = s.as_bytes();
    let mut pos = 0;
    let mut comparators = Vec::new();

    while pos < bytes.len() {
        while pos < bytes.len() && matches!(bytes[pos], b' ' | b'\t' | b'\r' | b'\n') {
            pos += 1;
        }
        if pos == bytes.len() {
            break;
        }

        let token_start = pos;
        while pos < bytes.len() && !matches!(bytes[pos], b' ' | b'\t' | b'\r' | b'\n') {
            pos += 1;
        }
        let token_end = pos;

        let (op, op_len) = match bytes[token_start] {
            b'=' => (Operator::Equal, 1),
            b'<' if token_start + 1 < token_end && bytes[token_start + 1] == b'=' => {
                (Operator::LessEqual, 2)
            }
            b'<' => (Operator::Less, 1),
            b'>' if token_start + 1 < token_end && bytes[token_start + 1] == b'=' => {
                (Operator::GreaterEqual, 2)
            }
            b'>' => (Operator::Greater, 1),
            _ => (Operator::Equal, 0),
        };

        let operand_start = token_start + op_len;
        if operand_start == token_end || !bytes[operand_start].is_ascii_digit() {
            return Err(RangeError::Syntax { at: token_start });
        }

        let operand = std::str::from_utf8(&bytes[operand_start..token_end]).unwrap();
        let version = parse_version(operand).map_err(|error| RangeError::Version {
            at: operand_start,
            error,
        })?;

        comparators.push(Comparator { op, version });
    }

    if comparators.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }

    Ok(Range {
        arms: vec![comparators],
    })
}

fn comparator_matches(candidate: &Version, comparator: &Comparator) -> bool {
    let ordering = compare_versions(candidate, &comparator.version);

    match comparator.op {
        Operator::Equal => ordering == Ordering::Equal,
        Operator::Less => ordering == Ordering::Less,
        Operator::LessEqual => ordering != Ordering::Greater,
        Operator::Greater => ordering == Ordering::Greater,
        Operator::GreaterEqual => ordering != Ordering::Less,
    }
}

pub fn matches(v: &Version, r: &Range) -> bool {
    r.arms.iter().any(|arm| {
        let admitted = v.pre.is_empty()
            || arm.iter().any(|comparator| {
                !comparator.version.pre.is_empty()
                    && (v.major, v.minor, v.patch)
                        == (
                            comparator.version.major,
                            comparator.version.minor,
                            comparator.version.patch,
                        )
            });

        admitted && arm.iter().all(|comparator| comparator_matches(v, comparator))
    })
}

fn main() {}