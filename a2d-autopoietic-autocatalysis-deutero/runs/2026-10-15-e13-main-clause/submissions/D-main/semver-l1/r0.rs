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

fn is_identifier_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-'
}

fn parse_core_field(bytes: &[u8], pos: &mut usize) -> Result<u64, VersionError> {
    let start = *pos;
    while *pos < bytes.len() && is_digit(bytes[*pos]) {
        *pos += 1;
    }

    if *pos == start {
        return Err(if *pos == bytes.len() || matches!(bytes[*pos], b'.' | b'-' | b'+') {
            VersionError::MissingCore { at: start }
        } else {
            VersionError::InvalidChar { at: start }
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

fn separator_error(bytes: &[u8], pos: usize) -> VersionError {
    if pos >= bytes.len() || matches!(bytes[pos], b'-' | b'+') {
        VersionError::MissingCore { at: pos }
    } else {
        VersionError::InvalidChar { at: pos }
    }
}

fn parse_identifiers(
    bytes: &[u8],
    pos: &mut usize,
    check_numeric_leading_zero: bool,
    stop_at_plus: bool,
) -> Result<Vec<String>, VersionError> {
    let mut identifiers = Vec::new();

    loop {
        let start = *pos;
        while *pos < bytes.len()
            && bytes[*pos] != b'.'
            && !(stop_at_plus && bytes[*pos] == b'+')
        {
            if !is_identifier_char(bytes[*pos]) {
                return Err(VersionError::InvalidChar { at: *pos });
            }
            *pos += 1;
        }

        if *pos == start {
            return Err(VersionError::EmptyIdentifier { at: start });
        }

        let identifier = &bytes[start..*pos];
        if check_numeric_leading_zero
            && identifier.iter().all(|&b| is_digit(b))
            && identifier.len() > 1
            && identifier[0] == b'0'
        {
            return Err(VersionError::LeadingZero { at: start });
        }

        identifiers.push(String::from_utf8(identifier.to_vec()).unwrap());

        if *pos == bytes.len() || (stop_at_plus && bytes[*pos] == b'+') {
            break;
        }

        // The only remaining delimiter is a dot.
        *pos += 1;
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
        return Err(separator_error(bytes, pos));
    }
    pos += 1;

    let minor = parse_core_field(bytes, &mut pos)?;
    if pos >= bytes.len() || bytes[pos] != b'.' {
        return Err(separator_error(bytes, pos));
    }
    pos += 1;

    let patch = parse_core_field(bytes, &mut pos)?;
    let mut pre = Vec::new();
    let mut build = Vec::new();

    if pos < bytes.len() {
        match bytes[pos] {
            b'-' => {
                pos += 1;
                pre = parse_identifiers(bytes, &mut pos, true, true)?;
                if pos < bytes.len() {
                    // The pre parser stops at the plus delimiter.
                    pos += 1;
                    build = parse_identifiers(bytes, &mut pos, false, false)?;
                }
            }
            b'+' => {
                pos += 1;
                build = parse_identifiers(bytes, &mut pos, false, false)?;
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
    !s.is_empty() && s.bytes().all(is_digit)
}

fn compare_identifiers(a: &str, b: &str) -> Ordering {
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
                for (x, y) in a.pre.iter().zip(&b.pre) {
                    let ordering = compare_identifiers(x, y);
                    if ordering != Ordering::Equal {
                        return ordering;
                    }
                }
                a.pre.len().cmp(&b.pre.len())
            }
        })
}

#[derive(Clone, Debug, PartialEq, Eq)]
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

#[derive(Clone, Debug)]
pub struct Range {
    comparators: Vec<Comparator>,
    admits_pre_core: Vec<(u64, u64, u64)>,
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
    let mut admits_pre_core = Vec::new();

    while pos < bytes.len() {
        while pos < bytes.len() && is_range_whitespace(bytes[pos]) {
            pos += 1;
        }
        if pos == bytes.len() {
            break;
        }

        let token_start = pos;
        let (operator, operand_start) = match bytes[pos] {
            b'=' => {
                pos += 1;
                (Operator::Equal, pos)
            }
            b'<' => {
                pos += 1;
                if pos < bytes.len() && bytes[pos] == b'=' {
                    pos += 1;
                    (Operator::LessEqual, pos)
                } else {
                    (Operator::Less, pos)
                }
            }
            b'>' => {
                pos += 1;
                if pos < bytes.len() && bytes[pos] == b'=' {
                    pos += 1;
                    (Operator::GreaterEqual, pos)
                } else {
                    (Operator::Greater, pos)
                }
            }
            _ => (Operator::Equal, pos),
        };

        if operand_start >= bytes.len() || !is_digit(bytes[operand_start]) {
            return Err(RangeError::Syntax { at: token_start });
        }

        let operand_end = (operand_start..bytes.len())
            .find(|&i| is_range_whitespace(bytes[i]))
            .unwrap_or(bytes.len());
        let operand = &s[operand_start..operand_end];
        let version = parse_version(operand).map_err(|error| RangeError::Version {
            at: operand_start,
            error,
        })?;

        if !version.pre.is_empty() {
            let core = (version.major, version.minor, version.patch);
            if !admits_pre_core.contains(&core) {
                admits_pre_core.push(core);
            }
        }

        comparators.push(Comparator { operator, version });
        pos = operand_end;
    }

    if comparators.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }

    Ok(Range {
        comparators,
        admits_pre_core,
    })
}

pub fn matches(v: &Version, r: &Range) -> bool {
    if !v.pre.is_empty() && !r.admits_pre_core.contains(&(v.major, v.minor, v.patch)) {
        return false;
    }

    r.comparators.iter().all(|comparator| {
        let ordering = compare_versions(v, &comparator.version);
        match comparator.operator {
            Operator::Equal => ordering == Ordering::Equal,
            Operator::Less => ordering == Ordering::Less,
            Operator::LessEqual => ordering != Ordering::Greater,
            Operator::Greater => ordering == Ordering::Greater,
            Operator::GreaterEqual => ordering != Ordering::Less,
        }
    })
}

fn main() {}