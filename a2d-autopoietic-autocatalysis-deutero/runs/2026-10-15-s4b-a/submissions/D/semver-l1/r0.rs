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

fn parse_identifiers(
    s: &str,
    base: usize,
    numeric_pre_rules: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut result = Vec::new();
    let mut start = 0;

    loop {
        let end = bytes[start..]
            .iter()
            .position(|&b| b == b'.')
            .map(|n| start + n)
            .unwrap_or(bytes.len());

        if end == start {
            return Err(VersionError::EmptyIdentifier { at: base + start });
        }

        for i in start..end {
            let b = bytes[i];
            if !(b.is_ascii_alphanumeric() || b == b'-') {
                return Err(VersionError::InvalidChar { at: base + i });
            }
        }

        if numeric_pre_rules
            && bytes[start..end].iter().all(|b| is_digit(*b))
            && end - start > 1
            && bytes[start] == b'0'
        {
            return Err(VersionError::LeadingZero { at: base + start });
        }

        result.push(s[start..end].to_owned());

        if end == bytes.len() {
            break;
        }
        start = end + 1;
    }

    Ok(result)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }

    let bytes = s.as_bytes();
    let mut pos = 0;

    let major = parse_core_field(bytes, &mut pos)?;
    if pos >= bytes.len() || bytes[pos] != b'.' {
        return Err(if pos >= bytes.len() || matches!(bytes[pos], b'-' | b'+') {
            VersionError::MissingCore { at: pos }
        } else {
            VersionError::InvalidChar { at: pos }
        });
    }
    pos += 1;

    let minor = parse_core_field(bytes, &mut pos)?;
    if pos >= bytes.len() || bytes[pos] != b'.' {
        return Err(if pos >= bytes.len() || matches!(bytes[pos], b'-' | b'+') {
            VersionError::MissingCore { at: pos }
        } else {
            VersionError::InvalidChar { at: pos }
        });
    }
    pos += 1;

    let patch = parse_core_field(bytes, &mut pos)?;
    if pos < bytes.len() && !matches!(bytes[pos], b'-' | b'+') {
        return Err(VersionError::InvalidChar { at: pos });
    }

    let mut pre = Vec::new();
    let mut build = Vec::new();

    if pos < bytes.len() && bytes[pos] == b'-' {
        let pre_start = pos + 1;
        let pre_end = bytes[pre_start..]
            .iter()
            .position(|&b| b == b'+')
            .map(|n| pre_start + n)
            .unwrap_or(bytes.len());

        pre = parse_identifiers(&s[pre_start..pre_end], pre_start, true)?;
        pos = pre_end;
    }

    if pos < bytes.len() && bytes[pos] == b'+' {
        let build_start = pos + 1;
        build = parse_identifiers(&s[build_start..], build_start, false)?;
    }

    Ok(Version {
        major,
        minor,
        patch,
        pre,
        build,
    })
}

fn numeric_identifier(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(is_digit)
}

fn compare_pre_identifier(a: &str, b: &str) -> Ordering {
    let a_numeric = numeric_identifier(a);
    let b_numeric = numeric_identifier(b);

    match (a_numeric, b_numeric) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (true, true) => a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes())),
        (false, false) => a.as_bytes().cmp(b.as_bytes()),
    }
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    let core = (a.major, a.minor, a.patch).cmp(&(b.major, b.minor, b.patch));
    if core != Ordering::Equal {
        return core;
    }

    match (a.pre.is_empty(), b.pre.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => {
            for (x, y) in a.pre.iter().zip(&b.pre) {
                let ordering = compare_pre_identifier(x, y);
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
    comparators: Vec<Comparator>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RangeError {
    Empty { at: usize },
    Syntax { at: usize },
    Version { at: usize, error: VersionError },
}

#[derive(Clone, Debug)]
struct Comparator {
    op: Op,
    version: Version,
}

#[derive(Clone, Copy, Debug)]
enum Op {
    Equal,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

fn is_range_whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let bytes = s.as_bytes();
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
        let token_bytes = &bytes[token_start..token_end];

        let (op, prefix_len) = if token_bytes.starts_with(b">=") {
            (Op::GreaterEqual, 2)
        } else if token_bytes.starts_with(b"<=") {
            (Op::LessEqual, 2)
        } else if token_bytes.starts_with(b">") {
            (Op::Greater, 1)
        } else if token_bytes.starts_with(b"<") {
            (Op::Less, 1)
        } else if token_bytes.starts_with(b"=") {
            (Op::Equal, 1)
        } else {
            (Op::Equal, 0)
        };

        let operand_start = token_start + prefix_len;
        if operand_start == token_end || !bytes[operand_start].is_ascii_digit() {
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

    Ok(Range { comparators })
}

pub fn matches(v: &Version, r: &Range) -> bool {
    let candidate_core = (v.major, v.minor, v.patch);
    let admits_pre = v.pre.is_empty()
        || r.comparators.iter().any(|comparator| {
            !comparator.version.pre.is_empty()
                && (comparator.version.major, comparator.version.minor, comparator.version.patch)
                    == candidate_core
        });

    admits_pre
        && r.comparators.iter().all(|comparator| {
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