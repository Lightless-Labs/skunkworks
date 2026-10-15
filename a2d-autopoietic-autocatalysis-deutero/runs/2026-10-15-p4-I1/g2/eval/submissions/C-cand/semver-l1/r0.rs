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

fn is_core_terminator(b: Option<u8>) -> bool {
    matches!(b, None | Some(b'.' | b'-' | b'+'))
}

fn parse_core(s: &str, pos: &mut usize) -> Result<u64, VersionError> {
    let bytes = s.as_bytes();
    let start = *pos;

    while *pos < bytes.len() && bytes[*pos].is_ascii_digit() {
        *pos += 1;
    }

    if *pos == start {
        return if is_core_terminator(bytes.get(*pos).copied()) {
            Err(VersionError::MissingCore { at: start })
        } else {
            Err(VersionError::InvalidChar { at: start })
        };
    }

    if *pos - start > 1 && bytes[start] == b'0' {
        return Err(VersionError::LeadingZero { at: start });
    }

    let mut value = 0u64;
    for &digit in &bytes[start..*pos] {
        value = value
            .checked_mul(10)
            .and_then(|n| n.checked_add(u64::from(digit - b'0')))
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
        let mut pos = ident_start;
        while pos < end && bytes[pos] != b'.' {
            let b = bytes[pos];
            if !(b.is_ascii_alphanumeric() || b == b'-') {
                return Err(VersionError::InvalidChar { at: pos });
            }
            pos += 1;
        }

        if pos == ident_start {
            return Err(VersionError::EmptyIdentifier { at: ident_start });
        }

        if is_pre {
            let ident = &bytes[ident_start..pos];
            if ident.iter().all(u8::is_ascii_digit)
                && ident.len() > 1
                && ident[0] == b'0'
            {
                return Err(VersionError::LeadingZero { at: ident_start });
            }
        }

        identifiers.push(s[ident_start..pos].to_owned());

        if pos == end {
            break;
        }

        ident_start = pos + 1;
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
    if bytes.get(pos) != Some(&b'.') {
        return if is_core_terminator(bytes.get(pos).copied()) {
            Err(VersionError::MissingCore { at: pos })
        } else {
            Err(VersionError::InvalidChar { at: pos })
        };
    }
    pos += 1;

    let minor = parse_core(s, &mut pos)?;
    if bytes.get(pos) != Some(&b'.') {
        return if is_core_terminator(bytes.get(pos).copied()) {
            Err(VersionError::MissingCore { at: pos })
        } else {
            Err(VersionError::InvalidChar { at: pos })
        };
    }
    pos += 1;

    let patch = parse_core(s, &mut pos)?;

    if pos == bytes.len() {
        return Ok(Version {
            major,
            minor,
            patch,
            pre: Vec::new(),
            build: Vec::new(),
        });
    }

    match bytes[pos] {
        b'-' | b'+' => {}
        _ => return Err(VersionError::InvalidChar { at: pos }),
    }

    let mut pre = Vec::new();
    let mut build = Vec::new();

    if bytes[pos] == b'-' {
        let pre_start = pos + 1;
        let build_sep = bytes[pre_start..]
            .iter()
            .position(|&b| b == b'+')
            .map(|offset| pre_start + offset);

        let pre_end = build_sep.unwrap_or(bytes.len());
        pre = parse_identifiers(s, pre_start, pre_end, true)?;

        if let Some(plus_pos) = build_sep {
            let build_start = plus_pos + 1;
            build = parse_identifiers(s, build_start, bytes.len(), false)?;
        }
    } else {
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
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn compare_numeric_identifiers(a: &str, b: &str) -> Ordering {
    a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes()))
}

pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    let core_order = (a.major, a.minor, a.patch).cmp(&(b.major, b.minor, b.patch));
    if core_order != Ordering::Equal {
        return core_order;
    }

    match (a.pre.is_empty(), b.pre.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        (false, false) => {}
    }

    for (ai, bi) in a.pre.iter().zip(&b.pre) {
        let a_numeric = is_numeric_identifier(ai);
        let b_numeric = is_numeric_identifier(bi);
        let order = match (a_numeric, b_numeric) {
            (true, true) => compare_numeric_identifiers(ai, bi),
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => ai.as_bytes().cmp(bi.as_bytes()),
        };
        if order != Ordering::Equal {
            return order;
        }
    }

    a.pre.len().cmp(&b.pre.len())
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
    op: Operator,
    version: Version,
}

#[derive(Clone, Copy, Debug)]
enum Operator {
    Eq,
    Lt,
    Le,
    Gt,
    Ge,
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
        let token = &s[token_start..token_end];

        let (op, op_len) = if token.starts_with("<=") {
            (Operator::Le, 2)
        } else if token.starts_with(">=") {
            (Operator::Ge, 2)
        } else if token.starts_with('<') {
            (Operator::Lt, 1)
        } else if token.starts_with('>') {
            (Operator::Gt, 1)
        } else if token.starts_with('=') {
            (Operator::Eq, 1)
        } else {
            (Operator::Eq, 0)
        };

        let operand_start = token_start + op_len;
        if operand_start >= token_end || !bytes[operand_start].is_ascii_digit() {
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

    Ok(Range {
        arms: vec![comparators],
    })
}

pub fn matches(v: &Version, r: &Range) -> bool {
    r.arms.iter().any(|arm| {
        let admits_prerelease = v.pre.is_empty()
            || arm.iter().any(|comparator| {
                !comparator.version.pre.is_empty()
                    && (comparator.version.major, comparator.version.minor, comparator.version.patch)
                        == (v.major, v.minor, v.patch)
            });

        admits_prerelease
            && arm.iter().all(|comparator| {
                let order = compare_versions(v, &comparator.version);
                match comparator.op {
                    Operator::Eq => order == Ordering::Equal,
                    Operator::Lt => order == Ordering::Less,
                    Operator::Le => order != Ordering::Greater,
                    Operator::Gt => order == Ordering::Greater,
                    Operator::Ge => order != Ordering::Less,
                }
            })
    })
}

fn main() {}