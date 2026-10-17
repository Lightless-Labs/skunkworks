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

    let mut pos = 0usize;
    let mut core = [0u64; 3];

    for field in 0..3 {
        let start = pos;
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            pos += 1;
        }

        if pos == start {
            if pos == bytes.len() || matches!(bytes[pos], b'.' | b'-' | b'+') {
                return Err(VersionError::MissingCore { at: start });
            }
            return Err(VersionError::InvalidChar { at: start });
        }

        let digits = &bytes[start..pos];
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
        core[field] = value;

        if field < 2 {
            if pos >= bytes.len() || matches!(bytes[pos], b'-' | b'+') {
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

    if pos < bytes.len() && bytes[pos] != b'-' && bytes[pos] != b'+' {
        return Err(VersionError::InvalidChar { at: pos });
    }

    if pos < bytes.len() && bytes[pos] == b'-' {
        pos += 1;
        let pre_start = pos;
        while pos < bytes.len() && bytes[pos] != b'+' {
            pos += 1;
        }
        pre = parse_identifiers(&s[pre_start..pos], pre_start, true)?;
    }

    if pos < bytes.len() && bytes[pos] == b'+' {
        pos += 1;
        build = parse_identifiers(&s[pos..], pos, false)?;
        pos = bytes.len();
    }

    if pos != bytes.len() {
        return Err(VersionError::InvalidChar { at: pos });
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
    text: &str,
    base: usize,
    check_numeric_leading_zero: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = text.as_bytes();
    let mut identifiers = Vec::new();
    let mut start = 0usize;

    loop {
        let mut end = start;
        while end < bytes.len() && bytes[end] != b'.' {
            let b = bytes[end];
            if !(b.is_ascii_alphanumeric() || b == b'-') {
                return Err(VersionError::InvalidChar { at: base + end });
            }
            end += 1;
        }

        if end == start {
            return Err(VersionError::EmptyIdentifier { at: base + start });
        }

        let identifier = &text[start..end];
        if check_numeric_leading_zero
            && identifier.bytes().all(|b| b.is_ascii_digit())
            && identifier.len() > 1
            && identifier.as_bytes()[0] == b'0'
        {
            return Err(VersionError::LeadingZero { at: base + start });
        }

        identifiers.push(identifier.to_owned());

        if end == bytes.len() {
            break;
        }
        start = end + 1;
        if start == bytes.len() {
            return Err(VersionError::EmptyIdentifier { at: base + start });
        }
    }

    Ok(identifiers)
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
        (false, false) => compare_pre(&a.pre, &b.pre),
    }
}

fn compare_pre(a: &[String], b: &[String]) -> Ordering {
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
    comparators: Vec<Comparator>,
    prerelease_tuples: Vec<(u64, u64, u64)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RangeError {
    Empty { at: usize },
    Syntax { at: usize },
    Version { at: usize, error: VersionError },
}

#[derive(Clone, Debug)]
enum Op {
    Equal,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

#[derive(Clone, Debug)]
struct Comparator {
    op: Op,
    version: Version,
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let bytes = s.as_bytes();
    let mut comparators = Vec::new();
    let mut prerelease_tuples = Vec::new();
    let mut pos = 0usize;

    while pos < bytes.len() && is_range_space(bytes[pos]) {
        pos += 1;
    }
    if pos == bytes.len() {
        return Err(RangeError::Empty { at: 0 });
    }

    while pos < bytes.len() {
        let token_start = pos;
        while pos < bytes.len() && !is_range_space(bytes[pos]) {
            pos += 1;
        }
        let token_end = pos;
        let token = &s[token_start..token_end];
        let token_bytes = token.as_bytes();

        let (op, operand_offset) = if token_bytes.starts_with(b">=") {
            (Op::GreaterEqual, 2)
        } else if token_bytes.starts_with(b"<=") {
            (Op::LessEqual, 2)
        } else if token_bytes.starts_with(b">") {
            (Op::Greater, 1)
        } else if token_bytes.starts_with(b"<") {
            (Op::Less, 1)
        } else if token_bytes.starts_with(b"=") {
            (Op::Equal, 1)
        } else if token_bytes.first().is_some_and(u8::is_ascii_digit) {
            (Op::Equal, 0)
        } else {
            return Err(RangeError::Syntax { at: token_start });
        };

        let operand_start = token_start + operand_offset;
        if operand_offset >= token_bytes.len()
            || !token_bytes[operand_offset].is_ascii_digit()
        {
            return Err(RangeError::Syntax { at: token_start });
        }

        let version = parse_version(&s[operand_start..token_end]).map_err(|error| {
            RangeError::Version {
                at: operand_start,
                error,
            }
        })?;

        if !version.pre.is_empty() {
            prerelease_tuples.push((version.major, version.minor, version.patch));
        }
        comparators.push(Comparator { op, version });

        while pos < bytes.len() && is_range_space(bytes[pos]) {
            pos += 1;
        }
    }

    Ok(Range {
        comparators,
        prerelease_tuples,
    })
}

pub fn matches(v: &Version, r: &Range) -> bool {
    if !v.pre.is_empty()
        && !r
            .prerelease_tuples
            .contains(&(v.major, v.minor, v.patch))
    {
        return false;
    }

    r.comparators.iter().all(|comparator| {
        let ordering = compare_versions(v, &comparator.version);
        match comparator.op {
            Op::Equal => ordering == Ordering::Equal,
            Op::Less => ordering == Ordering::Less,
            Op::LessEqual => ordering != Ordering::Greater,
            Op::Greater => ordering == Ordering::Greater,
            Op::GreaterEqual => ordering != Ordering::Less,
        }
    })
}

fn is_range_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}