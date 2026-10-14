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

    for (index, field) in core.iter_mut().enumerate() {
        let start = pos;
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            pos += 1;
        }

        if pos == start {
            return Err(if pos == bytes.len()
                || matches!(bytes[pos], b'.' | b'-' | b'+')
            {
                VersionError::MissingCore { at: pos }
            } else {
                VersionError::InvalidChar { at: pos }
            });
        }

        let digits = &bytes[start..pos];
        if digits.len() > 1 && digits[0] == b'0' {
            return Err(VersionError::LeadingZero { at: start });
        }

        let mut value = 0u64;
        for &digit in digits {
            value = match value
                .checked_mul(10)
                .and_then(|n| n.checked_add((digit - b'0') as u64))
            {
                Some(n) => n,
                None => return Err(VersionError::Overflow { at: start }),
            };
        }
        *field = value;

        if index < 2 {
            if pos >= bytes.len() || matches!(bytes[pos], b'-' | b'+') {
                return Err(VersionError::MissingCore { at: pos });
            }
            if bytes[pos] != b'.' {
                return Err(VersionError::InvalidChar { at: pos });
            }
            pos += 1;
        }
    }

    let (pre, build) = if pos == bytes.len() {
        (Vec::new(), Vec::new())
    } else {
        if !matches!(bytes[pos], b'-' | b'+') {
            return Err(VersionError::InvalidChar { at: pos });
        }

        let mut pre = Vec::new();
        let mut build = Vec::new();

        if bytes[pos] == b'-' {
            let pre_start = pos + 1;
            let plus = bytes[pre_start..]
                .iter()
                .position(|&b| b == b'+')
                .map(|offset| pre_start + offset);
            let pre_end = plus.unwrap_or(bytes.len());
            pre = parse_identifiers(bytes, pre_start, pre_end, true)?;

            if let Some(plus_pos) = plus {
                build = parse_identifiers(bytes, plus_pos + 1, bytes.len(), false)?;
            }
        } else {
            build = parse_identifiers(bytes, pos + 1, bytes.len(), false)?;
        }

        (pre, build)
    };

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
    let mut ident_start = start;

    loop {
        let ident_end = bytes[ident_start..end]
            .iter()
            .position(|&b| b == b'.')
            .map(|offset| ident_start + offset)
            .unwrap_or(end);

        if ident_start == ident_end {
            return Err(VersionError::EmptyIdentifier { at: ident_start });
        }

        for &byte in &bytes[ident_start..ident_end] {
            if !(byte.is_ascii_alphanumeric() || byte == b'-') {
                return Err(VersionError::InvalidChar { at: bytes
                    [ident_start..ident_end]
                    .iter()
                    .position(|&b| b == byte)
                    .map(|offset| ident_start + offset)
                    .unwrap_or(ident_start) });
            }
        }

        let ident_bytes = &bytes[ident_start..ident_end];
        let numeric = ident_bytes.iter().all(u8::is_ascii_digit);
        if check_numeric_leading_zero
            && numeric
            && ident_bytes.len() > 1
            && ident_bytes[0] == b'0'
        {
            return Err(VersionError::LeadingZero { at: ident_start });
        }

        result.push(String::from_utf8(ident_bytes.to_vec()).expect("ASCII identifier"));

        if ident_end == end {
            break;
        }
        ident_start = ident_end + 1;
    }

    Ok(result)
}

fn is_numeric_identifier(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn compare_numeric_identifiers(a: &str, b: &str) -> Ordering {
    a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes()))
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
        })
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
    let mut arms = Vec::new();
    let mut comparators = Vec::new();
    let mut pos = 0;

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

        let (op, operand_offset) = if token.starts_with(">=") {
            (Operator::GreaterEqual, 2)
        } else if token.starts_with("<=") {
            (Operator::LessEqual, 2)
        } else if token.starts_with('>') {
            (Operator::Greater, 1)
        } else if token.starts_with('<') {
            (Operator::Less, 1)
        } else if token.starts_with('=') {
            (Operator::Equal, 1)
        } else {
            (Operator::Equal, 0)
        };

        if operand_offset >= token.len()
            || !token.as_bytes()[operand_offset].is_ascii_digit()
        {
            return Err(RangeError::Syntax { at: token_start });
        }

        let operand_start = token_start + operand_offset;
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
    arms.push(comparators);

    Ok(Range { arms })
}

fn is_range_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

pub fn matches(v: &Version, r: &Range) -> bool {
    r.arms.iter().any(|arm| {
        let admitted = arm.iter().any(|comparator| {
            !comparator.version.pre.is_empty()
                && core_tuple(&comparator.version) == core_tuple(v)
        });

        if !v.pre.is_empty() && !admitted {
            return false;
        }

        arm.iter().all(|comparator| {
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

fn core_tuple(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
}