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

fn parse_core(s: &str, pos: &mut usize) -> Result<u64, VersionError> {
    let bytes = s.as_bytes();
    let start = *pos;
    while *pos < bytes.len() && bytes[*pos].is_ascii_digit() {
        *pos += 1;
    }
    if *pos == start {
        return Err(if *pos == bytes.len()
            || matches!(bytes[*pos], b'.' | b'-' | b'+')
        {
            VersionError::MissingCore { at: start }
        } else {
            VersionError::InvalidChar { at: start }
        });
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

fn valid_identifier_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-'
}

fn parse_identifiers(
    s: &str,
    start: usize,
    end: usize,
    prerelease: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut result = Vec::new();
    let mut id_start = start;
    loop {
        let mut i = id_start;
        while i < end && bytes[i] != b'.' {
            if !valid_identifier_byte(bytes[i]) {
                return Err(VersionError::InvalidChar { at: i });
            }
            i += 1;
        }
        if i == id_start {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }
        if prerelease {
            let id = &bytes[id_start..i];
            if id.iter().all(u8::is_ascii_digit) && id.len() > 1 && id[0] == b'0' {
                return Err(VersionError::LeadingZero { at: id_start });
            }
        }
        result.push(s[id_start..i].to_owned());
        if i == end {
            break;
        }
        id_start = i + 1;
    }
    Ok(result)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }
    let bytes = s.as_bytes();
    let mut pos = 0;
    let major = parse_core(s, &mut pos)?;
    if pos >= bytes.len() || bytes[pos] != b'.' {
        return Err(if pos >= bytes.len() || matches!(bytes[pos], b'-' | b'+') {
            VersionError::MissingCore { at: pos }
        } else {
            VersionError::InvalidChar { at: pos }
        });
    }
    pos += 1;
    let minor = parse_core(s, &mut pos)?;
    if pos >= bytes.len() || bytes[pos] != b'.' {
        return Err(if pos >= bytes.len() || matches!(bytes[pos], b'-' | b'+') {
            VersionError::MissingCore { at: pos }
        } else {
            VersionError::InvalidChar { at: pos }
        });
    }
    pos += 1;
    let patch = parse_core(s, &mut pos)?;

    let mut pre = Vec::new();
    let mut build = Vec::new();
    if pos < bytes.len() && bytes[pos] == b'-' {
        let pre_start = pos + 1;
        let pre_end = bytes[pre_start..]
            .iter()
            .position(|&b| b == b'+')
            .map(|i| pre_start + i)
            .unwrap_or(bytes.len());
        pre = parse_identifiers(s, pre_start, pre_end, true)?;
        pos = pre_end;
    }
    if pos < bytes.len() && bytes[pos] == b'+' {
        build = parse_identifiers(s, pos + 1, bytes.len(), false)?;
        pos = bytes.len();
    }
    if pos < bytes.len() {
        return Err(VersionError::InvalidChar { at: pos });
    }
    Ok(Version {
        major,
        minor,
        patch,
        pre,
        build,
    })
}

fn cmp_numeric_ident(a: &str, b: &str) -> Ordering {
    a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes()))
}

fn is_numeric_identifier(s: &str) -> bool {
    !s.is_empty() && s.as_bytes().iter().all(u8::is_ascii_digit)
}

fn cmp_pre(a: &[String], b: &[String]) -> Ordering {
    if a.is_empty() || b.is_empty() {
        return match (a.is_empty(), b.is_empty()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            _ => unreachable!(),
        };
    }
    for (left, right) in a.iter().zip(b) {
        let left_num = is_numeric_identifier(left);
        let right_num = is_numeric_identifier(right);
        let order = match (left_num, right_num) {
            (true, true) => cmp_numeric_ident(left, right),
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

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    (a.major, a.minor, a.patch)
        .cmp(&(b.major, b.minor, b.patch))
        .then_with(|| cmp_pre(&a.pre, &b.pre))
}

#[derive(Clone, Debug)]
struct Comparator {
    op: Op,
    version: Version,
}

#[derive(Clone, Copy, Debug)]
enum Op {
    Eq,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Debug)]
pub struct Range {
    comparators: Vec<Comparator>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RangeError {
    Empty { at: usize },
    Syntax { at: usize },
    Version { at: usize, error: VersionError },
}

fn is_range_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let bytes = s.as_bytes();
    let mut comparators = Vec::new();
    let mut pos = 0;

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
            (Op::Ge, 2)
        } else if token_bytes.starts_with(b"<=") {
            (Op::Le, 2)
        } else if token_bytes.starts_with(b">") {
            (Op::Gt, 1)
        } else if token_bytes.starts_with(b"<") {
            (Op::Lt, 1)
        } else if token_bytes.starts_with(b"=") {
            (Op::Eq, 1)
        } else {
            (Op::Eq, 0)
        };

        if operand_offset >= token_bytes.len()
            || !token_bytes[operand_offset].is_ascii_digit()
        {
            return Err(RangeError::Syntax { at: token_start });
        }

        let operand_start = token_start + operand_offset;
        let version = parse_version(&s[operand_start..token_end]).map_err(|error| {
            RangeError::Version {
                at: operand_start,
                error,
            }
        })?;
        comparators.push(Comparator { op, version });

        while pos < bytes.len() && is_range_space(bytes[pos]) {
            pos += 1;
        }
    }

    Ok(Range { comparators })
}

fn same_core(a: &Version, b: &Version) -> bool {
    (a.major, a.minor, a.patch) == (b.major, b.minor, b.patch)
}

pub fn matches(v: &Version, r: &Range) -> bool {
    let admits_prerelease = v.pre.is_empty()
        || r.comparators.iter().any(|c| {
            !c.version.pre.is_empty() && same_core(v, &c.version)
        });
    if !admits_prerelease {
        return false;
    }

    r.comparators.iter().all(|c| {
        let order = compare_versions(v, &c.version);
        match c.op {
            Op::Eq => order == Ordering::Equal,
            Op::Lt => order == Ordering::Less,
            Op::Le => order != Ordering::Greater,
            Op::Gt => order == Ordering::Greater,
            Op::Ge => order != Ordering::Less,
        }
    })
}

fn main() {}