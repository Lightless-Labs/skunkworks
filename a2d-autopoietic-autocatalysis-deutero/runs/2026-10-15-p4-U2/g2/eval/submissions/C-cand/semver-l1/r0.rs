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

fn core_number(s: &str, pos: &mut usize) -> Result<u64, VersionError> {
    let bytes = s.as_bytes();
    let start = *pos;

    while *pos < bytes.len() && bytes[*pos].is_ascii_digit() {
        *pos += 1;
    }

    if *pos == start {
        return match bytes.get(start) {
            None | Some(b'.' | b'-' | b'+') => {
                Err(VersionError::MissingCore { at: start })
            }
            Some(_) => Err(VersionError::InvalidChar { at: start }),
        };
    }

    let digits = &s[start..*pos];
    if digits.len() > 1 && digits.as_bytes()[0] == b'0' {
        return Err(VersionError::LeadingZero { at: start });
    }

    digits
        .parse::<u64>()
        .map_err(|_| VersionError::Overflow { at: start })
}

fn valid_identifier_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-'
}

fn parse_identifiers(
    s: &str,
    start: usize,
    end: usize,
    is_pre: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut identifiers = Vec::new();
    let mut ident_start = start;

    loop {
        let mut i = ident_start;
        while i < end && bytes[i] != b'.' {
            if !valid_identifier_char(bytes[i]) {
                return Err(VersionError::InvalidChar { at: i });
            }
            i += 1;
        }

        if i == ident_start {
            return Err(VersionError::EmptyIdentifier { at: ident_start });
        }

        let ident = &s[ident_start..i];
        if is_pre
            && ident.len() > 1
            && ident.as_bytes().iter().all(u8::is_ascii_digit)
            && ident.as_bytes()[0] == b'0'
        {
            return Err(VersionError::LeadingZero { at: ident_start });
        }

        identifiers.push(ident.to_owned());

        if i == end {
            break;
        }
        ident_start = i + 1;
    }

    Ok(identifiers)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }

    let mut pos = 0;
    let major = core_number(s, &mut pos)?;

    if s.as_bytes().get(pos) != Some(&b'.') {
        return match s.as_bytes().get(pos) {
            None | Some(b'-' | b'+') => Err(VersionError::MissingCore { at: pos }),
            Some(_) => Err(VersionError::InvalidChar { at: pos }),
        };
    }
    pos += 1;

    let minor = core_number(s, &mut pos)?;

    if s.as_bytes().get(pos) != Some(&b'.') {
        return match s.as_bytes().get(pos) {
            None | Some(b'-' | b'+') => Err(VersionError::MissingCore { at: pos }),
            Some(_) => Err(VersionError::InvalidChar { at: pos }),
        };
    }
    pos += 1;

    let patch = core_number(s, &mut pos)?;

    match s.as_bytes().get(pos) {
        None => {
            return Ok(Version {
                major,
                minor,
                patch,
                pre: Vec::new(),
                build: Vec::new(),
            });
        }
        Some(b'-' | b'+') => {}
        Some(_) => return Err(VersionError::InvalidChar { at: pos }),
    }

    let bytes = s.as_bytes();
    let mut pre = Vec::new();
    let mut build = Vec::new();

    if bytes[pos] == b'-' {
        let pre_start = pos + 1;
        let build_marker = bytes[pre_start..]
            .iter()
            .position(|&b| b == b'+')
            .map(|offset| pre_start + offset);
        let pre_end = build_marker.unwrap_or(bytes.len());

        pre = parse_identifiers(s, pre_start, pre_end, true)?;

        if let Some(marker) = build_marker {
            build = parse_identifiers(s, marker + 1, bytes.len(), false)?;
        }
    } else {
        build = parse_identifiers(s, pos + 1, bytes.len(), false)?;
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

fn compare_pre_identifier(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    let a_numeric = is_numeric_identifier(a);
    let b_numeric = is_numeric_identifier(b);

    match (a_numeric, b_numeric) {
        (true, true) => a
            .len()
            .cmp(&b.len())
            .then_with(|| a.as_bytes().cmp(b.as_bytes())),
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => a.as_bytes().cmp(b.as_bytes()),
    }
}

pub fn compare_versions(a: &Version, b: &Version) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    let core_order = (a.major, a.minor, a.patch).cmp(&(b.major, b.minor, b.patch));
    if core_order != Ordering::Equal {
        return core_order;
    }

    match (a.pre.is_empty(), b.pre.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => {
            for (left, right) in a.pre.iter().zip(&b.pre) {
                let order = compare_pre_identifier(left, right);
                if order != Ordering::Equal {
                    return order;
                }
            }
            a.pre.len().cmp(&b.pre.len())
        }
    }
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

#[derive(Clone, Debug)]
pub struct Range {
    arms: Vec<Vec<Comparator>>,
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
        while pos < bytes.len() && !is_range_whitespace(bytes[pos]) {
            pos += 1;
        }
        let token_end = pos;

        let mut operand_start = token_start;
        let operator = match bytes[token_start] {
            b'=' => {
                operand_start += 1;
                Operator::Equal
            }
            b'<' => {
                if bytes.get(token_start + 1) == Some(&b'=') {
                    operand_start += 2;
                    Operator::LessEqual
                } else {
                    operand_start += 1;
                    Operator::Less
                }
            }
            b'>' => {
                if bytes.get(token_start + 1) == Some(&b'=') {
                    operand_start += 2;
                    Operator::GreaterEqual
                } else {
                    operand_start += 1;
                    Operator::Greater
                }
            }
            _ => Operator::Equal,
        };

        if operand_start == token_end || !bytes[operand_start].is_ascii_digit() {
            return Err(RangeError::Syntax { at: token_start });
        }

        let operand = &s[operand_start..token_end];
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
        let admitted_prerelease = v.pre.is_empty()
            || arm.iter().any(|comparator| {
                !comparator.version.pre.is_empty()
                    && (comparator.version.major, comparator.version.minor, comparator.version.patch)
                        == (v.major, v.minor, v.patch)
            });

        admitted_prerelease
            && arm.iter().all(|comparator| {
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

fn main() {}