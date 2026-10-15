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

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return Err(VersionError::Empty);
    }

    let mut pos = 0;
    let mut core = [0u64; 3];

    for (field, value) in core.iter_mut().enumerate() {
        let start = pos;
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            pos += 1;
        }

        if pos == start {
            return Err(if pos == bytes.len()
                || matches!(bytes[pos], b'.' | b'-' | b'+')
            {
                VersionError::MissingCore { at: start }
            } else {
                VersionError::InvalidChar { at: start }
            });
        }

        if pos - start > 1 && bytes[start] == b'0' {
            return Err(VersionError::LeadingZero { at: start });
        }

        let mut number = 0u64;
        for &digit in &bytes[start..pos] {
            number = match number
                .checked_mul(10)
                .and_then(|n| n.checked_add((digit - b'0') as u64))
            {
                Some(n) => n,
                None => return Err(VersionError::Overflow { at: start }),
            };
        }
        *value = number;

        if field < 2 {
            if pos == bytes.len() || matches!(bytes[pos], b'-' | b'+') {
                return Err(VersionError::MissingCore { at: pos });
            }
            if bytes[pos] != b'.' {
                return Err(VersionError::InvalidChar { at: pos });
            }
            pos += 1;
        }
    }

    let mut pre = Vec::new();
    let mut build = Vec::new();

    if pos < bytes.len() {
        match bytes[pos] {
            b'-' => {
                pos += 1;
                let pre_end = bytes[pos..]
                    .iter()
                    .position(|&b| b == b'+')
                    .map(|offset| pos + offset)
                    .unwrap_or(bytes.len());
                pre = parse_identifiers(bytes, pos, pre_end, true)?;

                if pre_end < bytes.len() {
                    pos = pre_end + 1;
                    build = parse_identifiers(bytes, pos, bytes.len(), false)?;
                }
            }
            b'+' => {
                pos += 1;
                build = parse_identifiers(bytes, pos, bytes.len(), false)?;
            }
            _ => return Err(VersionError::InvalidChar { at: pos }),
        }
    }

    Ok(Version {
        major: core[0],
        minor: core[1],
        patch: core[2],
        pre,
        build,
    })
}

fn parse_identifiers(
    bytes: &[u8],
    start: usize,
    end: usize,
    check_numeric_leading_zero: bool,
) -> Result<Vec<String>, VersionError> {
    let mut result = Vec::new();
    let mut id_start = start;

    loop {
        let id_end = bytes[id_start..end]
            .iter()
            .position(|&b| b == b'.')
            .map(|offset| id_start + offset)
            .unwrap_or(end);

        if id_start == id_end {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }

        for (offset, &byte) in bytes[id_start..id_end].iter().enumerate() {
            if !(byte.is_ascii_alphanumeric() || byte == b'-') {
                return Err(VersionError::InvalidChar {
                    at: id_start + offset,
                });
            }
        }

        let identifier = &bytes[id_start..id_end];
        let is_numeric = identifier.iter().all(u8::is_ascii_digit);
        if check_numeric_leading_zero
            && is_numeric
            && identifier.len() > 1
            && identifier[0] == b'0'
        {
            return Err(VersionError::LeadingZero { at: id_start });
        }

        result.push(String::from_utf8(identifier.to_vec()).unwrap());

        if id_end == end {
            break;
        }
        id_start = id_end + 1;
    }

    Ok(result)
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    (a.major, a.minor, a.patch)
        .cmp(&(b.major, b.minor, b.patch))
        .then_with(|| compare_pre(&a.pre, &b.pre))
}

fn compare_pre(a: &[String], b: &[String]) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        (false, false) => {}
    }

    for (left, right) in a.iter().zip(b.iter()) {
        let left_numeric = left.bytes().all(|byte| byte.is_ascii_digit());
        let right_numeric = right.bytes().all(|byte| byte.is_ascii_digit());

        let ordering = match (left_numeric, right_numeric) {
            (true, true) => left
                .len()
                .cmp(&right.len())
                .then_with(|| left.as_bytes().cmp(right.as_bytes())),
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => left.as_bytes().cmp(right.as_bytes()),
        };

        if ordering != Ordering::Equal {
            return ordering;
        }
    }

    a.len().cmp(&b.len())
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

#[derive(Clone, Debug)]
struct Comparator {
    op: Operator,
    version: Version,
}

#[derive(Clone, Copy, Debug)]
enum Operator {
    Equal,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let bytes = s.as_bytes();
    let mut pos = 0;
    let mut comparators = Vec::new();

    while pos < bytes.len() {
        while pos < bytes.len() && is_range_whitespace(bytes[pos]) {
            pos += 1;
        }
        if pos == bytes.len() {
            break;
        }

        let token_start = pos;
        while pos < bytes.len() && !is_range_whitespace(bytes[pos]) {
            pos += 1;
        }
        let token_end = pos;
        let token = &bytes[token_start..token_end];

        let (op, operand_offset) = if token.starts_with(b">=") {
            (Operator::GreaterEqual, 2)
        } else if token.starts_with(b"<=") {
            (Operator::LessEqual, 2)
        } else if token.starts_with(b">") {
            (Operator::Greater, 1)
        } else if token.starts_with(b"<") {
            (Operator::Less, 1)
        } else if token.starts_with(b"=") {
            (Operator::Equal, 1)
        } else {
            (Operator::Equal, 0)
        };

        if operand_offset >= token.len() || !token[operand_offset].is_ascii_digit() {
            return Err(RangeError::Syntax { at: token_start });
        }

        let operand_start = token_start + operand_offset;
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

pub fn matches(v: &Version, r: &Range) -> bool {
    r.arms.iter().any(|arm| {
        let admitted_pre = v.pre.is_empty()
            || arm.iter().any(|comparator| {
                !comparator.version.pre.is_empty()
                    && same_core(v, &comparator.version)
            });

        admitted_pre
            && arm.iter().all(|comparator| {
                let ordering = compare_versions(v, &comparator.version);
                match comparator.op {
                    Operator::Equal => ordering == Ordering::Equal,
                    Operator::Less => ordering == Ordering::Less,
                    Operator::LessEqual => ordering != Ordering::Greater,
                    Operator::Greater => ordering == Ordering::Greater,
                    Operator::GreaterEqual => ordering != Ordering::Less,
                }
            })
    })
}

fn same_core(a: &Version, b: &Version) -> bool {
    a.major == b.major && a.minor == b.minor && a.patch == b.patch
}

fn is_range_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}