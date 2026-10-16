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
                return Err(VersionError::MissingCore { at: pos });
            }
            return Err(VersionError::InvalidChar { at: pos });
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
            if pos >= bytes.len() || matches!(bytes[pos], b'-' | b'+') {
                return Err(VersionError::MissingCore { at: pos });
            }
            if bytes[pos] != b'.' {
                return Err(VersionError::InvalidChar { at: pos });
            }
            pos += 1;
        }
    }

    let (pre, build) = match bytes.get(pos).copied() {
        None => (Vec::new(), Vec::new()),
        Some(b'-') => {
            let pre_start = pos + 1;
            let plus = bytes[pre_start..]
                .iter()
                .position(|&b| b == b'+')
                .map(|i| pre_start + i);
            let pre_end = plus.unwrap_or(bytes.len());
            let pre = parse_identifiers(s, pre_start, pre_end, true)?;
            let build = if let Some(plus_pos) = plus {
                parse_identifiers(s, plus_pos + 1, bytes.len(), false)?
            } else {
                Vec::new()
            };
            (pre, build)
        }
        Some(b'+') => {
            let build = parse_identifiers(s, pos + 1, bytes.len(), false)?;
            (Vec::new(), build)
        }
        Some(_) => return Err(VersionError::InvalidChar { at: pos }),
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
    s: &str,
    start: usize,
    end: usize,
    check_numeric_leading_zero: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut result = Vec::new();
    let mut id_start = start;

    loop {
        let id_end = bytes[id_start..end]
            .iter()
            .position(|&b| b == b'.')
            .map(|i| id_start + i)
            .unwrap_or(end);

        if id_start == id_end {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }

        for index in id_start..id_end {
            let b = bytes[index];
            if !(b.is_ascii_alphanumeric() || b == b'-') {
                return Err(VersionError::InvalidChar { at: index });
            }
        }

        if check_numeric_leading_zero
            && bytes[id_start..id_end].iter().all(u8::is_ascii_digit)
            && id_end - id_start > 1
            && bytes[id_start] == b'0'
        {
            return Err(VersionError::LeadingZero { at: id_start });
        }

        result.push(s[id_start..id_end].to_owned());
        if id_end == end {
            break;
        }
        id_start = id_end + 1;
    }

    Ok(result)
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
        let left_numeric = left.bytes().all(|b| b.is_ascii_digit());
        let right_numeric = right.bytes().all(|b| b.is_ascii_digit());

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RangeError {
    Empty { at: usize },
    Syntax { at: usize },
    Version { at: usize, error: VersionError },
}

#[derive(Clone, Debug)]
pub struct Range {
    comparators: Vec<Comparator>,
    admits_pre: Vec<(u64, u64, u64)>,
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

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let bytes = s.as_bytes();
    let mut comparators = Vec::new();
    let mut admits_pre = Vec::new();
    let mut pos = 0;

    while pos < bytes.len() {
        while pos < bytes.len() && is_range_space(bytes[pos]) {
            pos += 1;
        }
        if pos == bytes.len() {
            break;
        }

        let token_start = pos;
        while pos < bytes.len() && !is_range_space(bytes[pos]) {
            pos += 1;
        }
        let token_end = pos;

        let mut operand_start = token_start;
        let op = match bytes[token_start] {
            b'=' => {
                operand_start += 1;
                Op::Equal
            }
            b'<' => {
                if bytes.get(token_start + 1) == Some(&b'=') {
                    operand_start += 2;
                    Op::LessEqual
                } else {
                    operand_start += 1;
                    Op::Less
                }
            }
            b'>' => {
                if bytes.get(token_start + 1) == Some(&b'=') {
                    operand_start += 2;
                    Op::GreaterEqual
                } else {
                    operand_start += 1;
                    Op::Greater
                }
            }
            _ => Op::Equal,
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
            admits_pre.push((version.major, version.minor, version.patch));
        }
        comparators.push(Comparator { op, version });
    }

    if comparators.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }

    Ok(Range {
        comparators,
        admits_pre,
    })
}

fn is_range_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

pub fn matches(v: &Version, r: &Range) -> bool {
    if !v.pre.is_empty()
        && !r
            .admits_pre
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