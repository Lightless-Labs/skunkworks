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

    let mut pos = 0usize;
    let mut core = [0u64; 3];

    for (field, number) in core.iter_mut().enumerate() {
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

        let mut value = 0u64;
        for &digit in &bytes[start..pos] {
            value = value
                .checked_mul(10)
                .and_then(|v| v.checked_add((digit - b'0') as u64))
                .ok_or(VersionError::Overflow { at: start })?;
        }
        *number = value;

        if field < 2 {
            if pos < bytes.len() && bytes[pos] == b'.' {
                pos += 1;
            } else if pos == bytes.len() || matches!(bytes[pos], b'-' | b'+') {
                return Err(VersionError::MissingCore { at: pos });
            } else {
                return Err(VersionError::InvalidChar { at: pos });
            }
        }
    }

    let mut pre = Vec::new();
    let mut build = Vec::new();

    if pos < bytes.len() && bytes[pos] != b'-' && bytes[pos] != b'+' {
        return Err(VersionError::InvalidChar { at: pos });
    }

    if pos < bytes.len() && bytes[pos] == b'-' {
        pos += 1;
        let pre_end = bytes[pos..]
            .iter()
            .position(|&b| b == b'+')
            .map(|n| pos + n)
            .unwrap_or(bytes.len());

        pre = parse_identifiers(bytes, pos, pre_end, true)?;
        pos = pre_end;
    }

    if pos < bytes.len() && bytes[pos] == b'+' {
        pos += 1;
        build = parse_identifiers(bytes, pos, bytes.len(), false)?;
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
    check_numeric_leading_zero: bool,
) -> Result<Vec<String>, VersionError> {
    let mut identifiers = Vec::new();
    let mut ident_start = start;

    loop {
        let ident_end = bytes[ident_start..end]
            .iter()
            .position(|&b| b == b'.')
            .map(|n| ident_start + n)
            .unwrap_or(end);

        if ident_start == ident_end {
            return Err(VersionError::EmptyIdentifier { at: ident_start });
        }

        for i in ident_start..ident_end {
            let b = bytes[i];
            if !(b.is_ascii_alphanumeric() || b == b'-') {
                return Err(VersionError::InvalidChar { at: i });
            }
        }

        let numeric = bytes[ident_start..ident_end]
            .iter()
            .all(u8::is_ascii_digit);
        if check_numeric_leading_zero
            && numeric
            && ident_end - ident_start > 1
            && bytes[ident_start] == b'0'
        {
            return Err(VersionError::LeadingZero { at: ident_start });
        }

        identifiers.push(
            std::str::from_utf8(&bytes[ident_start..ident_end])
                .expect("validated ASCII identifier")
                .to_owned(),
        );

        if ident_end == end {
            break;
        }
        ident_start = ident_end + 1;
    }

    Ok(identifiers)
}

fn is_numeric_identifier(identifier: &str) -> bool {
    !identifier.is_empty() && identifier.bytes().all(|b| b.is_ascii_digit())
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
                for (left, right) in a.pre.iter().zip(&b.pre) {
                    let left_numeric = is_numeric_identifier(left);
                    let right_numeric = is_numeric_identifier(right);
                    let ordering = match (left_numeric, right_numeric) {
                        (true, true) => compare_numeric_identifiers(left, right),
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
        })
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
    op: ComparatorOp,
    version: Version,
}

#[derive(Clone, Copy, Debug)]
enum ComparatorOp {
    Equal,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let bytes = s.as_bytes();
    let mut pos = 0usize;
    let mut comparators = Vec::new();
    let mut saw_token = false;

    while pos < bytes.len() {
        while pos < bytes.len() && is_range_whitespace(bytes[pos]) {
            pos += 1;
        }
        if pos == bytes.len() {
            break;
        }

        saw_token = true;
        let token_start = pos;
        while pos < bytes.len() && !is_range_whitespace(bytes[pos]) {
            pos += 1;
        }
        let token_end = pos;

        let (op, operator_len) = if bytes[token_start] == b'<' {
            if token_start + 1 < token_end && bytes[token_start + 1] == b'=' {
                (ComparatorOp::LessEqual, 2)
            } else {
                (ComparatorOp::Less, 1)
            }
        } else if bytes[token_start] == b'>' {
            if token_start + 1 < token_end && bytes[token_start + 1] == b'=' {
                (ComparatorOp::GreaterEqual, 2)
            } else {
                (ComparatorOp::Greater, 1)
            }
        } else if bytes[token_start] == b'=' {
            (ComparatorOp::Equal, 1)
        } else {
            (ComparatorOp::Equal, 0)
        };

        let operand_start = token_start + operator_len;
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

    if !saw_token {
        return Err(RangeError::Empty { at: 0 });
    }

    Ok(Range {
        arms: vec![comparators],
    })
}

fn is_range_whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

pub fn matches(v: &Version, r: &Range) -> bool {
    r.arms.iter().any(|arm| {
        let admits_prerelease = !v.pre.is_empty()
            && arm.iter().any(|comparator| {
                !comparator.version.pre.is_empty()
                    && (v.major, v.minor, v.patch)
                        == (
                            comparator.version.major,
                            comparator.version.minor,
                            comparator.version.patch,
                        )
            });

        if !v.pre.is_empty() && !admits_prerelease {
            return false;
        }

        arm.iter().all(|comparator| {
            let ordering = compare_versions(v, &comparator.version);
            match comparator.op {
                ComparatorOp::Equal => ordering == Ordering::Equal,
                ComparatorOp::Less => ordering == Ordering::Less,
                ComparatorOp::LessEqual => ordering != Ordering::Greater,
                ComparatorOp::Greater => ordering == Ordering::Greater,
                ComparatorOp::GreaterEqual => ordering != Ordering::Less,
            }
        })
    })
}