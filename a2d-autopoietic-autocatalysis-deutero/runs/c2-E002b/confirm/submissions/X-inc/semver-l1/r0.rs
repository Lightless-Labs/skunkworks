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

fn parse_core(s: &str, pos: &mut usize) -> Result<u64, VersionError> {
    let bytes = s.as_bytes();
    let start = *pos;
    while *pos < bytes.len() && bytes[*pos].is_ascii_digit() {
        *pos += 1;
    }

    if *pos == start {
        if *pos == bytes.len() || missing_core_byte(bytes[*pos]) {
            return Err(VersionError::MissingCore { at: start });
        }
        return Err(VersionError::InvalidChar { at: start });
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
            let b = bytes[i];
            if !(b.is_ascii_alphanumeric() || b == b'-') {
                return Err(VersionError::InvalidChar { at: i });
            }
            i += 1;
        }

        if i == ident_start {
            return Err(VersionError::EmptyIdentifier { at: ident_start });
        }

        let ident_bytes = &bytes[ident_start..i];
        if is_pre
            && ident_bytes.len() > 1
            && ident_bytes.iter().all(u8::is_ascii_digit)
            && ident_bytes[0] == b'0'
        {
            return Err(VersionError::LeadingZero { at: ident_start });
        }

        identifiers.push(
            std::str::from_utf8(ident_bytes)
                .expect("validated ASCII identifier")
                .to_owned(),
        );

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

    let bytes = s.as_bytes();
    let mut pos = 0;
    let major = parse_core(s, &mut pos)?;

    if pos >= bytes.len() || bytes[pos] != b'.' {
        if pos >= bytes.len() || missing_core_byte(bytes[pos]) {
            return Err(VersionError::MissingCore { at: pos });
        }
        return Err(VersionError::InvalidChar { at: pos });
    }
    pos += 1;

    let minor = parse_core(s, &mut pos)?;
    if pos >= bytes.len() || bytes[pos] != b'.' {
        if pos >= bytes.len() || missing_core_byte(bytes[pos]) {
            return Err(VersionError::MissingCore { at: pos });
        }
        return Err(VersionError::InvalidChar { at: pos });
    }
    pos += 1;

    let patch = parse_core(s, &mut pos)?;
    if pos < bytes.len() && bytes[pos] != b'-' && bytes[pos] != b'+' {
        return Err(VersionError::InvalidChar { at: pos });
    }

    let mut pre = Vec::new();
    let mut build = Vec::new();

    if pos < bytes.len() && bytes[pos] == b'-' {
        let pre_start = pos + 1;
        let build_marker = bytes[pre_start..]
            .iter()
            .position(|&b| b == b'+')
            .map(|offset| pre_start + offset);
        let pre_end = build_marker.unwrap_or(bytes.len());
        pre = parse_identifiers(s, pre_start, pre_end, true)?;
        pos = pre_end;

        if pos < bytes.len() {
            let build_start = pos + 1;
            build = parse_identifiers(s, build_start, bytes.len(), false)?;
        }
    } else if pos < bytes.len() && bytes[pos] == b'+' {
        let build_start = pos + 1;
        build = parse_identifiers(s, build_start, bytes.len(), false)?;
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
                let order = compare_identifiers(left, right);
                if order != Ordering::Equal {
                    return order;
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
enum Op {
    Eq,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Debug)]
struct Comparator {
    op: Op,
    version: Version,
}

fn is_range_whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn parse_comparator_token(token: &str, token_start: usize) -> Result<Comparator, RangeError> {
    let bytes = token.as_bytes();
    let (op, operator_len) = if bytes.starts_with(b">=") {
        (Op::Ge, 2)
    } else if bytes.starts_with(b"<=") {
        (Op::Le, 2)
    } else if bytes.starts_with(b">") {
        (Op::Gt, 1)
    } else if bytes.starts_with(b"<") {
        (Op::Lt, 1)
    } else if bytes.starts_with(b"=") {
        (Op::Eq, 1)
    } else {
        (Op::Eq, 0)
    };

    let operand_start = operator_len;
    if operand_start >= bytes.len() || !bytes[operand_start].is_ascii_digit() {
        return Err(RangeError::Syntax { at: token_start });
    }

    let version = parse_version(&token[operand_start..]).map_err(|error| RangeError::Version {
        at: token_start + operand_start,
        error,
    })?;

    Ok(Comparator { op, version })
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let bytes = s.as_bytes();
    let mut arms = vec![Vec::new()];
    let mut pos = 0;
    let mut saw_token = false;

    while pos < bytes.len() {
        while pos < bytes.len() && is_range_whitespace(bytes[pos]) {
            pos += 1;
        }
        if pos == bytes.len() {
            break;
        }

        let start = pos;
        while pos < bytes.len() && !is_range_whitespace(bytes[pos]) {
            pos += 1;
        }
        saw_token = true;
        let token = &s[start..pos];
        arms[0].push(parse_comparator_token(token, start)?);
    }

    if !saw_token {
        return Err(RangeError::Empty { at: 0 });
    }

    Ok(Range { arms })
}

fn comparator_matches(version: &Version, comparator: &Comparator) -> bool {
    let ordering = compare_versions(version, &comparator.version);
    match comparator.op {
        Op::Eq => ordering == Ordering::Equal,
        Op::Lt => ordering == Ordering::Less,
        Op::Le => ordering != Ordering::Greater,
        Op::Gt => ordering == Ordering::Greater,
        Op::Ge => ordering != Ordering::Less,
    }
}

pub fn matches(v: &Version, r: &Range) -> bool {
    r.arms.iter().any(|arm| {
        let core = (v.major, v.minor, v.patch);
        let prerelease_admitted = v.pre.is_empty()
            || arm.iter().any(|comparator| {
                !comparator.version.pre.is_empty()
                    && (
                        comparator.version.major,
                        comparator.version.minor,
                        comparator.version.patch,
                    ) == core
            });

        prerelease_admitted && arm.iter().all(|c| comparator_matches(v, c))
    })
}