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
            if pos == bytes.len() || matches!(bytes[pos], b'.' | b'-' | b'+') {
                return Err(VersionError::MissingCore { at: start });
            }
            return Err(VersionError::InvalidChar { at: start });
        }

        if pos - start > 1 && bytes[start] == b'0' {
            return Err(VersionError::LeadingZero { at: start });
        }

        let mut number = 0u64;
        for &digit in &bytes[start..pos] {
            number = number
                .checked_mul(10)
                .and_then(|n| n.checked_add((digit - b'0') as u64))
                .ok_or(VersionError::Overflow { at: start })?;
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

    let (pre, build) = if pos == bytes.len() {
        (Vec::new(), Vec::new())
    } else {
        match bytes[pos] {
            b'-' => {
                let pre_start = pos + 1;
                let plus = bytes[pre_start..]
                    .iter()
                    .position(|&b| b == b'+')
                    .map(|offset| pre_start + offset);
                let pre_end = plus.unwrap_or(bytes.len());
                let pre = parse_identifiers(bytes, pre_start, pre_end, true)?;
                let build = if let Some(plus_pos) = plus {
                    parse_identifiers(bytes, plus_pos + 1, bytes.len(), false)?
                } else {
                    Vec::new()
                };
                (pre, build)
            }
            b'+' => {
                let build = parse_identifiers(bytes, pos + 1, bytes.len(), false)?;
                (Vec::new(), build)
            }
            _ => return Err(VersionError::InvalidChar { at: pos }),
        }
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
    if start == end {
        return Err(VersionError::EmptyIdentifier { at: start });
    }

    let mut identifiers = Vec::new();
    let mut identifier_start = start;

    for pos in start..=end {
        if pos == end || bytes[pos] == b'.' {
            if identifier_start == pos {
                return Err(VersionError::EmptyIdentifier { at: identifier_start });
            }

            for (offset, &byte) in bytes[identifier_start..pos].iter().enumerate() {
                if !(byte.is_ascii_alphanumeric() || byte == b'-') {
                    return Err(VersionError::InvalidChar {
                        at: identifier_start + offset,
                    });
                }
            }

            let identifier_bytes = &bytes[identifier_start..pos];
            if check_numeric_leading_zero
                && identifier_bytes.len() > 1
                && identifier_bytes[0] == b'0'
                && identifier_bytes.iter().all(u8::is_ascii_digit)
            {
                return Err(VersionError::LeadingZero {
                    at: identifier_start,
                });
            }

            identifiers.push(
                std::str::from_utf8(identifier_bytes)
                    .expect("validated identifier is ASCII")
                    .to_owned(),
            );
            identifier_start = pos + 1;
        }
    }

    Ok(identifiers)
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

    for (left, right) in a.iter().zip(b) {
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
    operator: Operator,
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
        let token = &s[token_start..token_end];

        let (operator, operand_offset) = if token.starts_with(">=") {
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

        let operand = &token[operand_offset..];
        if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
            return Err(RangeError::Syntax { at: token_start });
        }

        let version = parse_version(operand).map_err(|error| RangeError::Version {
            at: token_start + operand_offset,
            error,
        })?;
        comparators.push(Comparator { operator, version });
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
        let admitted = v.pre.is_empty()
            || arm.iter().any(|comparator| {
                !comparator.version.pre.is_empty()
                    && (comparator.version.major, comparator.version.minor, comparator.version.patch)
                        == (v.major, v.minor, v.patch)
            });

        admitted
            && arm.iter().all(|comparator| {
                let ordering = compare_versions(v, &comparator.version);
                match comparator.operator {
                    Operator::Equal => ordering == Ordering::Equal,
                    Operator::Less => ordering == Ordering::Less,
                    Operator::LessEqual => ordering != Ordering::Greater,
                    Operator::Greater => ordering == Ordering::Greater,
                    Operator::GreaterEqual => ordering != Ordering::Less,
                }
            })
    })
}

fn is_range_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}