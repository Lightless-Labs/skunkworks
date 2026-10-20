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

    for (index, value) in core.iter_mut().enumerate() {
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

        let mut number = 0u64;
        for &digit in digits {
            number = match number.checked_mul(10).and_then(|n| n.checked_add((digit - b'0') as u64)) {
                Some(n) => n,
                None => return Err(VersionError::Overflow { at: start }),
            };
        }
        *value = number;

        if index < 2 {
            if pos >= bytes.len() || matches!(bytes[pos], b'-' | b'+') {
                return Err(VersionError::MissingCore { at: pos });
            }
            if bytes[pos] != b'.' {
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
        let pre_start = pos;
        let plus = bytes[pos..].iter().position(|&b| b == b'+').map(|i| pos + i);
        let pre_end = plus.unwrap_or(bytes.len());
        parse_identifiers(bytes, pre_start, pre_end, true, &mut pre)?;
        pos = pre_end;
    }

    if pos < bytes.len() && bytes[pos] == b'+' {
        pos += 1;
        parse_identifiers(bytes, pos, bytes.len(), false, &mut build)?;
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
    bytes: &[u8],
    start: usize,
    end: usize,
    prerelease: bool,
    out: &mut Vec<String>,
) -> Result<(), VersionError> {
    let mut id_start = start;

    loop {
        let mut pos = id_start;
        while pos < end && bytes[pos] != b'.' {
            let b = bytes[pos];
            if !(b.is_ascii_alphanumeric() || b == b'-') {
                return Err(VersionError::InvalidChar { at: pos });
            }
            pos += 1;
        }

        if pos == id_start {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }

        let identifier = &bytes[id_start..pos];
        if prerelease
            && identifier.iter().all(u8::is_ascii_digit)
            && identifier.len() > 1
            && identifier[0] == b'0'
        {
            return Err(VersionError::LeadingZero { at: id_start });
        }

        out.push(String::from_utf8(identifier.to_vec()).unwrap());

        if pos == end {
            break;
        }
        id_start = pos + 1;
        if id_start == end {
            return Err(VersionError::EmptyIdentifier { at: id_start });
        }
    }

    Ok(())
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
            a.pre.len().cmp(&b.pre.len())
        }
    }
}

#[derive(Clone, Debug)]
pub struct Range {
    comparators: Vec<Vec<Comparator>>,
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
    Eq,
    Lt,
    Le,
    Gt,
    Ge,
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let bytes = s.as_bytes();
    let mut arms = vec![Vec::new()];
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

        arms.last_mut().unwrap().push(Comparator { op, version });
    }

    if arms[0].is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }

    Ok(Range { comparators: arms })
}

fn is_range_whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

pub fn matches(v: &Version, r: &Range) -> bool {
    let candidate_core = (v.major, v.minor, v.patch);

    r.comparators.iter().any(|arm| {
        let prerelease_admitted = v.pre.is_empty()
            || arm.iter().any(|comparator| {
                !comparator.version.pre.is_empty()
                    && (comparator.version.major, comparator.version.minor, comparator.version.patch)
                        == candidate_core
            });

        prerelease_admitted
            && arm.iter().all(|comparator| {
                let ordering = compare_versions(v, &comparator.version);
                match comparator.op {
                    Op::Eq => ordering == std::cmp::Ordering::Equal,
                    Op::Lt => ordering == std::cmp::Ordering::Less,
                    Op::Le => ordering != std::cmp::Ordering::Greater,
                    Op::Gt => ordering == std::cmp::Ordering::Greater,
                    Op::Ge => ordering != std::cmp::Ordering::Less,
                }
            })
    })
}