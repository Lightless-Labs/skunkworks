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

fn is_core_missing_byte(b: u8) -> bool {
    b == b'.' || b == b'-' || b == b'+'
}

fn is_identifier_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-'
}

fn parse_core(s: &str, pos: &mut usize) -> Result<u64, VersionError> {
    let bytes = s.as_bytes();
    let start = *pos;

    if start >= bytes.len() || is_core_missing_byte(bytes[start]) {
        return Err(VersionError::MissingCore { at: start });
    }
    if !bytes[start].is_ascii_digit() {
        return Err(VersionError::InvalidChar { at: start });
    }

    while *pos < bytes.len() && bytes[*pos].is_ascii_digit() {
        *pos += 1;
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
    s: &str,
    pos: &mut usize,
    pre: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut identifiers = Vec::new();

    loop {
        let start = *pos;
        while *pos < bytes.len()
            && bytes[*pos] != b'.'
            && !(pre && bytes[*pos] == b'+')
        {
            if !is_identifier_char(bytes[*pos]) {
                return Err(VersionError::InvalidChar { at: *pos });
            }
            *pos += 1;
        }

        if *pos == start {
            return Err(VersionError::EmptyIdentifier { at: start });
        }

        let identifier = &s[start..*pos];
        if pre
            && identifier.len() > 1
            && identifier.as_bytes().iter().all(u8::is_ascii_digit)
            && identifier.as_bytes()[0] == b'0'
        {
            return Err(VersionError::LeadingZero { at: start });
        }
        identifiers.push(identifier.to_owned());

        if *pos >= bytes.len() || (pre && bytes[*pos] == b'+') {
            break;
        }

        // The current byte is a dot.
        *pos += 1;
        if *pos >= bytes.len() || (pre && bytes[*pos] == b'+') {
            return Err(VersionError::EmptyIdentifier { at: *pos });
        }
    }

    Ok(identifiers)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }

    let mut pos = 0;
    let major = parse_core(s, &mut pos)?;
    if pos >= s.len() || s.as_bytes()[pos] != b'.' {
        if pos >= s.len() || is_core_missing_byte(s.as_bytes()[pos]) {
            return Err(VersionError::MissingCore { at: pos });
        }
        return Err(VersionError::InvalidChar { at: pos });
    }
    pos += 1;

    let minor = parse_core(s, &mut pos)?;
    if pos >= s.len() || s.as_bytes()[pos] != b'.' {
        if pos >= s.len() || is_core_missing_byte(s.as_bytes()[pos]) {
            return Err(VersionError::MissingCore { at: pos });
        }
        return Err(VersionError::InvalidChar { at: pos });
    }
    pos += 1;

    let patch = parse_core(s, &mut pos)?;

    let mut pre = Vec::new();
    let mut build = Vec::new();

    if pos < s.len() {
        match s.as_bytes()[pos] {
            b'-' => {
                pos += 1;
                pre = parse_identifiers(s, &mut pos, true)?;
                if pos < s.len() {
                    // Pre parsing stops only at the first plus.
                    pos += 1;
                    build = parse_identifiers(s, &mut pos, false)?;
                }
            }
            b'+' => {
                pos += 1;
                build = parse_identifiers(s, &mut pos, false)?;
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

fn is_numeric_identifier(s: &str) -> bool {
    !s.is_empty() && s.as_bytes().iter().all(u8::is_ascii_digit)
}

fn compare_identifiers(a: &str, b: &str) -> std::cmp::Ordering {
    let a_numeric = is_numeric_identifier(a);
    let b_numeric = is_numeric_identifier(b);
    match (a_numeric, b_numeric) {
        (true, true) => a
            .len()
            .cmp(&b.len())
            .then_with(|| a.as_bytes().cmp(b.as_bytes())),
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        (false, false) => a.as_bytes().cmp(b.as_bytes()),
    }
}

pub fn compare_versions(a: &Version, b: &Version) -> std::cmp::Ordering {
    use std::cmp::Ordering;

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
                let ordering = compare_identifiers(left, right);
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
            a.pre.len().cmp(&b.pre.len())
        }
    }
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
enum Operator {
    Equal,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

#[derive(Clone, Debug)]
struct Comparator {
    operator: Operator,
    version: Version,
}

fn is_range_whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
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
        let operator = match bytes[pos] {
            b'=' => {
                pos += 1;
                Operator::Equal
            }
            b'<' => {
                pos += 1;
                if pos < bytes.len() && bytes[pos] == b'=' {
                    pos += 1;
                    Operator::LessEqual
                } else {
                    Operator::Less
                }
            }
            b'>' => {
                pos += 1;
                if pos < bytes.len() && bytes[pos] == b'=' {
                    pos += 1;
                    Operator::GreaterEqual
                } else {
                    Operator::Greater
                }
            }
            _ => Operator::Equal,
        };

        let operand_start = pos;
        if pos >= bytes.len()
            || !bytes[pos].is_ascii_digit()
            || (pos > token_start && is_range_whitespace(bytes[pos]))
        {
            return Err(RangeError::Syntax { at: token_start });
        }

        while pos < bytes.len() && !is_range_whitespace(bytes[pos]) {
            pos += 1;
        }

        let operand = &s[operand_start..pos];
        let version = parse_version(operand).map_err(|error| RangeError::Version {
            at: operand_start,
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
        let has_matching_prerelease_operand = arm.iter().any(|comparator| {
            !comparator.version.pre.is_empty()
                && (v.major, v.minor, v.patch)
                    == (
                        comparator.version.major,
                        comparator.version.minor,
                        comparator.version.patch,
                    )
        });

        if !v.pre.is_empty() && !has_matching_prerelease_operand {
            return false;
        }

        arm.iter().all(|comparator| {
            let ordering = compare_versions(v, &comparator.version);
            match comparator.operator {
                Operator::Equal => ordering == std::cmp::Ordering::Equal,
                Operator::Less => ordering == std::cmp::Ordering::Less,
                Operator::LessEqual => ordering != std::cmp::Ordering::Greater,
                Operator::Greater => ordering == std::cmp::Ordering::Greater,
                Operator::GreaterEqual => ordering != std::cmp::Ordering::Less,
            }
        })
    })
}