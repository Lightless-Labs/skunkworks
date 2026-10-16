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

fn core_missing_byte(b: Option<u8>) -> bool {
    matches!(b, None | Some(b'.' | b'-' | b'+'))
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }

    let bytes = s.as_bytes();
    let mut pos = 0usize;
    let mut core = [0u64; 3];

    for field in 0..3 {
        let start = pos;
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            pos += 1;
        }

        if pos == start {
            if core_missing_byte(bytes.get(pos).copied()) {
                return Err(VersionError::MissingCore { at: start });
            }
            return Err(VersionError::InvalidChar { at: start });
        }

        if pos - start > 1 && bytes[start] == b'0' {
            return Err(VersionError::LeadingZero { at: start });
        }

        let mut value = 0u64;
        for &digit in &bytes[start..pos] {
            value = value
                .checked_mul(10)
                .and_then(|v| v.checked_add((digit - b'0') as u64))
                .ok_or(VersionError::Overflow { at: start })?;
        }
        core[field] = value;

        if field < 2 {
            match bytes.get(pos).copied() {
                Some(b'.') => pos += 1,
                b @ (None | Some(b'-' | b'+')) => {
                    let _ = b;
                    return Err(VersionError::MissingCore { at: pos });
                }
                _ => return Err(VersionError::InvalidChar { at: pos }),
            }
        } else {
            match bytes.get(pos).copied() {
                None | Some(b'-' | b'+') => {}
                _ => return Err(VersionError::InvalidChar { at: pos }),
            }
        }
    }

    let mut pre = Vec::new();
    let mut build = Vec::new();

    if bytes.get(pos) == Some(&b'-') {
        pos += 1;
        let pre_start = pos;
        let build_separator = bytes[pos..].iter().position(|&b| b == b'+')
            .map(|i| pos + i)
            .unwrap_or(bytes.len());
        parse_identifiers(s, pre_start, build_separator, true, &mut pre)?;

        pos = build_separator;
    }

    if bytes.get(pos) == Some(&b'+') {
        pos += 1;
        parse_identifiers(s, pos, bytes.len(), false, &mut build)?;
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
    is_pre: bool,
    out: &mut Vec<String>,
) -> Result<(), VersionError> {
    let bytes = s.as_bytes();
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

        let mut numeric = true;
        for i in id_start..id_end {
            let b = bytes[i];
            if !(b.is_ascii_alphanumeric() || b == b'-') {
                return Err(VersionError::InvalidChar { at: i });
            }
            if !b.is_ascii_digit() {
                numeric = false;
            }
        }

        if is_pre && numeric && id_end - id_start > 1 && bytes[id_start] == b'0' {
            return Err(VersionError::LeadingZero { at: id_start });
        }

        out.push(s[id_start..id_end].to_owned());

        if id_end == end {
            break;
        }
        id_start = id_end + 1;
    }

    Ok(())
}

fn numeric_identifier(s: &str) -> bool {
    !s.is_empty() && s.as_bytes().iter().all(u8::is_ascii_digit)
}

fn compare_numeric_identifiers(a: &str, b: &str) -> Ordering {
    match a.len().cmp(&b.len()) {
        Ordering::Equal => a.as_bytes().cmp(b.as_bytes()),
        other => other,
    }
}

fn compare_pre(a: &[String], b: &[String]) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        (false, false) => {}
    }

    for (left, right) in a.iter().zip(b.iter()) {
        let left_numeric = numeric_identifier(left);
        let right_numeric = numeric_identifier(right);

        let order = match (left_numeric, right_numeric) {
            (true, true) => compare_numeric_identifiers(left, right),
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
        .then_with(|| compare_pre(&a.pre, &b.pre))
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
    op: CompareOp,
    version: Version,
}

#[derive(Clone, Copy, Debug)]
enum CompareOp {
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
    let mut i = 0usize;
    let mut comparators = Vec::new();

    while i < bytes.len() {
        while i < bytes.len() && is_range_whitespace(bytes[i]) {
            i += 1;
        }
        if i == bytes.len() {
            break;
        }

        let token_start = i;
        let (op, op_len) = match bytes[i] {
            b'=' => (CompareOp::Equal, 1),
            b'<' if bytes.get(i + 1) == Some(&b'=') => (CompareOp::LessEqual, 2),
            b'<' => (CompareOp::Less, 1),
            b'>' if bytes.get(i + 1) == Some(&b'=') => (CompareOp::GreaterEqual, 2),
            b'>' => (CompareOp::Greater, 1),
            _ => (CompareOp::Equal, 0),
        };
        let operand_start = i + op_len;

        if operand_start >= bytes.len() || !bytes[operand_start].is_ascii_digit() {
            return Err(RangeError::Syntax { at: token_start });
        }

        let mut operand_end = operand_start;
        while operand_end < bytes.len() && !is_range_whitespace(bytes[operand_end]) {
            operand_end += 1;
        }

        let operand = &s[operand_start..operand_end];
        let version = parse_version(operand).map_err(|error| RangeError::Version {
            at: operand_start,
            error,
        })?;
        comparators.push(Comparator { op, version });
        i = operand_end;
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
        if !v.pre.is_empty()
            && !arm.iter().any(|comparator| {
                !comparator.version.pre.is_empty()
                    && (
                        comparator.version.major,
                        comparator.version.minor,
                        comparator.version.patch,
                    ) == (v.major, v.minor, v.patch)
            })
        {
            return false;
        }

        arm.iter().all(|comparator| {
            let order = compare_versions(v, &comparator.version);
            match comparator.op {
                CompareOp::Equal => order == Ordering::Equal,
                CompareOp::Less => order == Ordering::Less,
                CompareOp::LessEqual => order != Ordering::Greater,
                CompareOp::Greater => order == Ordering::Greater,
                CompareOp::GreaterEqual => order != Ordering::Less,
            }
        })
    })
}