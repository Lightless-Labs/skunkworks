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

    for (field, value) in core.iter_mut().enumerate() {
        let start = pos;
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            pos += 1;
        }

        if pos == start {
            if pos == bytes.len() || matches!(bytes[pos], b'.' | b'-' | b'+') {
                return Err(VersionError::MissingCore { at: start });
            }
            return Err(VersionError::InvalidChar { at: pos });
        }

        if pos - start > 1 && bytes[start] == b'0' {
            return Err(VersionError::LeadingZero { at: start });
        }

        let mut n = 0u64;
        for &digit in &bytes[start..pos] {
            let d = (digit - b'0') as u64;
            n = n
                .checked_mul(10)
                .and_then(|x| x.checked_add(d))
                .ok_or(VersionError::Overflow { at: start })?;
        }
        *value = n;

        if field < 2 {
            if pos < bytes.len() && bytes[pos] == b'.' {
                pos += 1;
            } else if pos == bytes.len() || matches!(bytes[pos], b'-' | b'+') {
                return Err(VersionError::MissingCore { at: pos });
            } else {
                return Err(VersionError::InvalidChar { at: pos });
            }
        }
    }

    let mut pre = Vec::new();
    let mut build = Vec::new();

    if pos < bytes.len() && bytes[pos] != b'-' && bytes[pos] != b'+' {
        return Err(VersionError::InvalidChar { at: pos });
    }

    if pos < bytes.len() && bytes[pos] == b'-' {
        pos += 1;
        let start = pos;
        while pos < bytes.len() && bytes[pos] != b'+' {
            pos += 1;
        }
        parse_identifiers(&s[start..pos], start, true, &mut pre)?;
    }

    if pos < bytes.len() && bytes[pos] == b'+' {
        pos += 1;
        let start = pos;
        parse_identifiers(&s[start..], start, false, &mut build)?;
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
    out: &mut Vec<String>,
) -> Result<(), VersionError> {
    let bytes = text.as_bytes();

    if bytes.is_empty() {
        return Err(VersionError::EmptyIdentifier { at: base });
    }

    let mut start = 0usize;
    loop {
        let end = bytes[start..]
            .iter()
            .position(|&b| b == b'.')
            .map(|offset| start + offset)
            .unwrap_or(bytes.len());

        if end == start {
            return Err(VersionError::EmptyIdentifier { at: base + start });
        }

        let id = &bytes[start..end];
        for (offset, &b) in id.iter().enumerate() {
            if !(b.is_ascii_alphanumeric() || b == b'-') {
                return Err(VersionError::InvalidChar {
                    at: base + start + offset,
                });
            }
        }

        if check_numeric_leading_zero
            && id.len() > 1
            && id[0] == b'0'
            && id.iter().all(u8::is_ascii_digit)
        {
            return Err(VersionError::LeadingZero { at: base + start });
        }

        out.push(text[start..end].to_owned());

        if end == bytes.len() {
            break;
        }
        start = end + 1;
        if start == bytes.len() {
            return Err(VersionError::EmptyIdentifier { at: base + start });
        }
    }

    Ok(())
}

fn is_numeric_identifier(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
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
        let left_numeric = is_numeric_identifier(left);
        let right_numeric = is_numeric_identifier(right);

        let order = match (left_numeric, right_numeric) {
            (true, true) => left
                .len()
                .cmp(&right.len())
                .then_with(|| left.as_bytes().cmp(right.as_bytes())),
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => left.as_bytes().cmp(right.as_bytes()),
        };

        if order != Ordering::Equal {
            return order;
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
    let mut pos = 0usize;
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

        let token = &s[token_start..token_end];
        let token_bytes = token.as_bytes();
        let (op, op_len) = if token_bytes.starts_with(b">=") {
            (Operator::GreaterEqual, 2)
        } else if token_bytes.starts_with(b"<=") {
            (Operator::LessEqual, 2)
        } else if token_bytes.starts_with(b">") {
            (Operator::Greater, 1)
        } else if token_bytes.starts_with(b"<") {
            (Operator::Less, 1)
        } else if token_bytes.starts_with(b"=") {
            (Operator::Equal, 1)
        } else if token_bytes.first().is_some_and(u8::is_ascii_digit) {
            (Operator::Equal, 0)
        } else {
            return Err(RangeError::Syntax { at: token_start });
        };

        let operand_start = token_start + op_len;
        if op_len > token_bytes.len()
            || operand_start == token_end
            || !bytes[operand_start].is_ascii_digit()
        {
            return Err(RangeError::Syntax { at: token_start });
        }

        let operand = &s[operand_start..token_end];
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

fn is_range_whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

pub fn matches(v: &Version, r: &Range) -> bool {
    r.arms.iter().any(|arm| {
        let prerelease_admitted = arm.iter().any(|comparator| {
            !comparator.version.pre.is_empty()
                && (comparator.version.major, comparator.version.minor, comparator.version.patch)
                    == (v.major, v.minor, v.patch)
        });

        if !v.pre.is_empty() && !prerelease_admitted {
            return false;
        }

        arm.iter().all(|comparator| {
            let order = compare_versions(v, &comparator.version);
            match comparator.op {
                Operator::Equal => order == Ordering::Equal,
                Operator::Less => order == Ordering::Less,
                Operator::LessEqual => order != Ordering::Greater,
                Operator::Greater => order == Ordering::Greater,
                Operator::GreaterEqual => order != Ordering::Less,
            }
        })
    })
}