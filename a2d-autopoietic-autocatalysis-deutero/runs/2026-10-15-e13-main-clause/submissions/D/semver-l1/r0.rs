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

fn missing_core_byte(b: u8) -> bool {
    matches!(b, b'.' | b'-' | b'+')
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }

    let bytes = s.as_bytes();
    let mut pos = 0;

    let mut core = [0u64; 3];
    for (field, value) in core.iter_mut().enumerate() {
        let start = pos;
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            pos += 1;
        }

        if pos == start {
            if pos == bytes.len() || missing_core_byte(bytes[pos]) {
                return Err(VersionError::MissingCore { at: pos });
            }
            return Err(VersionError::InvalidChar { at: pos });
        }

        if pos - start > 1 && bytes[start] == b'0' {
            return Err(VersionError::LeadingZero { at: start });
        }

        let mut n = 0u64;
        for &digit in &bytes[start..pos] {
            n = n
                .checked_mul(10)
                .and_then(|x| x.checked_add((digit - b'0') as u64))
                .ok_or(VersionError::Overflow { at: start })?;
        }
        *value = n;

        if field < 2 {
            if pos >= bytes.len() || bytes[pos] != b'.' {
                if pos == bytes.len() || matches!(bytes[pos], b'-' | b'+') {
                    return Err(VersionError::MissingCore { at: pos });
                }
                return Err(VersionError::InvalidChar { at: pos });
            }
            pos += 1;
        } else if pos < bytes.len() && !matches!(bytes[pos], b'-' | b'+') {
            return Err(VersionError::InvalidChar { at: pos });
        }
    }

    let mut pre = Vec::new();
    let mut build = Vec::new();

    if pos < bytes.len() && bytes[pos] == b'-' {
        pos += 1;
        let start = pos;
        let end = bytes[pos..]
            .iter()
            .position(|&b| b == b'+')
            .map_or(bytes.len(), |i| pos + i);
        pre = parse_identifiers(s, start, end, true)?;
        pos = end;
    }

    if pos < bytes.len() && bytes[pos] == b'+' {
        pos += 1;
        build = parse_identifiers(s, pos, bytes.len(), false)?;
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
    s: &str,
    start: usize,
    end: usize,
    prerelease: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut identifiers = Vec::new();
    let mut id_start = start;

    loop {
        let mut i = id_start;
        while i < end && bytes[i] != b'.' {
            if !bytes[i].is_ascii_alphanumeric() && bytes[i] != b'-' {
                return Err(VersionError::InvalidChar { at: i });
            }
            i += 1;
        }

        if i == id_start {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }

        if prerelease
            && bytes[id_start..i].iter().all(u8::is_ascii_digit)
            && i - id_start > 1
            && bytes[id_start] == b'0'
        {
            return Err(VersionError::LeadingZero { at: id_start });
        }

        identifiers.push(s[id_start..i].to_owned());

        if i == end {
            break;
        }
        id_start = i + 1;
    }

    Ok(identifiers)
}

fn is_numeric_identifier(s: &str) -> bool {
    !s.is_empty() && s.as_bytes().iter().all(u8::is_ascii_digit)
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
                for (x, y) in a.pre.iter().zip(&b.pre) {
                    let xn = is_numeric_identifier(x);
                    let yn = is_numeric_identifier(y);
                    let order = match (xn, yn) {
                        (true, true) => compare_numeric_identifiers(x, y),
                        (true, false) => Ordering::Less,
                        (false, true) => Ordering::Greater,
                        (false, false) => x.as_bytes().cmp(y.as_bytes()),
                    };
                    if order != Ordering::Equal {
                        return order;
                    }
                }
                a.pre.len().cmp(&b.pre.len())
            }
        })
}

#[derive(Clone, Debug)]
pub struct Range {
    comparators: Vec<Comparator>,
    admits_prerelease: Option<(u64, u64, u64)>,
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
    let mut admits_prerelease = None;

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

        let mut operand_start = token_start;
        let operator = match bytes[token_start] {
            b'=' => {
                operand_start += 1;
                Operator::Equal
            }
            b'<' if token_start + 1 < token_end && bytes[token_start + 1] == b'=' => {
                operand_start += 2;
                Operator::LessEqual
            }
            b'<' => {
                operand_start += 1;
                Operator::Less
            }
            b'>' if token_start + 1 < token_end && bytes[token_start + 1] == b'=' => {
                operand_start += 2;
                Operator::GreaterEqual
            }
            b'>' => {
                operand_start += 1;
                Operator::Greater
            }
            _ => Operator::Equal,
        };

        if operand_start >= token_end || !bytes[operand_start].is_ascii_digit() {
            return Err(RangeError::Syntax { at: token_start });
        }

        let operand = &s[operand_start..token_end];
        let version = parse_version(operand).map_err(|error| RangeError::Version {
            at: operand_start,
            error,
        })?;

        if !version.pre.is_empty() {
            admits_prerelease.get_or_insert((version.major, version.minor, version.patch));
        }

        comparators.push(Comparator { operator, version });

        while pos < bytes.len() && is_range_whitespace(bytes[pos]) {
            pos += 1;
        }
    }

    Ok(Range {
        comparators,
        admits_prerelease,
    })
}

pub fn matches(v: &Version, r: &Range) -> bool {
    if !v.pre.is_empty()
        && r.admits_prerelease != Some((v.major, v.minor, v.patch))
    {
        return false;
    }

    r.comparators.iter().all(|comparator| {
        let order = compare_versions(v, &comparator.version);
        match comparator.operator {
            Operator::Equal => order == Ordering::Equal,
            Operator::Less => order == Ordering::Less,
            Operator::LessEqual => order != Ordering::Greater,
            Operator::Greater => order == Ordering::Greater,
            Operator::GreaterEqual => order != Ordering::Less,
        }
    })
}