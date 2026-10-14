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

fn byte_at(s: &str, at: usize) -> Option<u8> {
    s.as_bytes().get(at).copied()
}

fn parse_core(s: &str, pos: &mut usize) -> Result<u64, VersionError> {
    let start = *pos;
    while matches!(byte_at(s, *pos), Some(b'0'..=b'9')) {
        *pos += 1;
    }

    if *pos == start {
        match byte_at(s, start) {
            None | Some(b'.' | b'-' | b'+') => {
                return Err(VersionError::MissingCore { at: start });
            }
            Some(_) => return Err(VersionError::InvalidChar { at: start }),
        }
    }

    let digits = &s[start..*pos];
    if digits.len() > 1 && digits.starts_with('0') {
        return Err(VersionError::LeadingZero { at: start });
    }
    digits
        .parse::<u64>()
        .map_err(|_| VersionError::Overflow { at: start })
}

fn parse_identifiers(
    s: &str,
    start: usize,
    end: usize,
    prerelease: bool,
) -> Result<Vec<String>, VersionError> {
    let mut identifiers = Vec::new();
    let mut ident_start = start;

    loop {
        let mut cursor = ident_start;
        while cursor < end {
            let byte = s.as_bytes()[cursor];
            if byte.is_ascii_alphanumeric() || byte == b'-' {
                cursor += 1;
            } else if byte == b'.' {
                break;
            } else {
                return Err(VersionError::InvalidChar { at: cursor });
            }
        }

        if cursor == ident_start {
            return Err(VersionError::EmptyIdentifier { at: ident_start });
        }

        let identifier = &s[ident_start..cursor];
        if prerelease
            && identifier.len() > 1
            && identifier.bytes().all(|byte| byte.is_ascii_digit())
            && identifier.starts_with('0')
        {
            return Err(VersionError::LeadingZero { at: ident_start });
        }
        identifiers.push(identifier.to_owned());

        if cursor == end {
            break;
        }
        ident_start = cursor + 1;
        if ident_start == end {
            return Err(VersionError::EmptyIdentifier { at: ident_start });
        }
    }

    Ok(identifiers)
}

/// Parses a full semantic version string.
pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }

    let mut pos = 0;
    let major = parse_core(s, &mut pos)?;
    for _field in 0..2 {
        match byte_at(s, pos) {
            Some(b'.') => pos += 1,
            None | Some(b'-' | b'+') => {
                return Err(VersionError::MissingCore { at: pos });
            }
            Some(_) => return Err(VersionError::InvalidChar { at: pos }),
        }
        if _field == 0 {
            let minor = parse_core(s, &mut pos)?;
            match byte_at(s, pos) {
                Some(b'.') => {
                    pos += 1;
                    let patch = parse_core(s, &mut pos)?;
                    return parse_version_tail(s, pos, major, minor, patch);
                }
                None | Some(b'-' | b'+') => {
                    return Err(VersionError::MissingCore { at: pos });
                }
                Some(_) => return Err(VersionError::InvalidChar { at: pos }),
            }
        }
    }

    Err(VersionError::MissingCore { at: pos })
}

fn parse_version_tail(
    s: &str,
    mut pos: usize,
    major: u64,
    minor: u64,
    patch: u64,
) -> Result<Version, VersionError> {
    let mut pre = Vec::new();
    let mut build = Vec::new();

    match byte_at(s, pos) {
        None => {}
        Some(b'-') => {
            pos += 1;
            let pre_start = pos;
            let build_marker = s[pos..].find('+').map(|offset| pos + offset);
            let pre_end = build_marker.unwrap_or(s.len());
            pre = parse_identifiers(s, pre_start, pre_end, true)?;
            pos = pre_end;
            if byte_at(s, pos) == Some(b'+') {
                pos += 1;
                build = parse_identifiers(s, pos, s.len(), false)?;
            }
        }
        Some(b'+') => {
            pos += 1;
            build = parse_identifiers(s, pos, s.len(), false)?;
        }
        Some(_) => return Err(VersionError::InvalidChar { at: pos }),
    }

    Ok(Version {
        major,
        minor,
        patch,
        pre,
        build,
    })
}

fn is_numeric(identifier: &str) -> bool {
    !identifier.is_empty() && identifier.bytes().all(|byte| byte.is_ascii_digit())
}

fn compare_identifiers(a: &str, b: &str) -> Ordering {
    match (is_numeric(a), is_numeric(b)) {
        (true, true) => a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes())),
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => a.as_bytes().cmp(b.as_bytes()),
    }
}

/// Compares versions by semantic precedence, ignoring build metadata.
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

fn is_range_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

/// Parses a conjunction of full-version comparators.
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
        let (op, op_len) = match bytes[pos] {
            b'=' => (Op::Equal, 1),
            b'<' if bytes.get(pos + 1) == Some(&b'=') => (Op::LessEqual, 2),
            b'>' if bytes.get(pos + 1) == Some(&b'=') => (Op::GreaterEqual, 2),
            b'<' => (Op::Less, 1),
            b'>' => (Op::Greater, 1),
            _ => (Op::Equal, 0),
        };
        pos += op_len;

        if pos >= bytes.len() || !bytes[pos].is_ascii_digit() {
            return Err(RangeError::Syntax { at: token_start });
        }

        let operand_start = pos;
        while pos < bytes.len() && !is_range_whitespace(bytes[pos]) {
            pos += 1;
        }
        let operand = &s[operand_start..pos];
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

/// Checks whether a version satisfies a parsed range.
pub fn matches(v: &Version, r: &Range) -> bool {
    let candidate_is_prerelease = !v.pre.is_empty();
    if candidate_is_prerelease
        && !r.comparators.iter().any(|comparator| {
            !comparator.version.pre.is_empty()
                && (comparator.version.major, comparator.version.minor, comparator.version.patch)
                    == (v.major, v.minor, v.patch)
        })
    {
        return false;
    }

    r.comparators.iter().all(|comparator| {
        let order = compare_versions(v, &comparator.version);
        match comparator.op {
            Op::Equal => order == Ordering::Equal,
            Op::Less => order == Ordering::Less,
            Op::LessEqual => order != Ordering::Greater,
            Op::Greater => order == Ordering::Greater,
            Op::GreaterEqual => order != Ordering::Less,
        }
    })
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(text: &str) -> Version {
        parse_version(text).expect("valid version")
    }

    #[test]
    fn parses_version_and_compares_prereleases() {
        let parsed = version("2.1.3-rc.1+build.7");
        assert_eq!((parsed.major, parsed.minor, parsed.patch), (2, 1, 3));
        assert_eq!(parsed.pre, vec!["rc", "1"]);
        assert_eq!(parsed.build, vec!["build", "7"]);
        assert_eq!(
            compare_versions(&version("2.1.3-alpha"), &version("2.1.3-beta")),
            Ordering::Less
        );
    }

    #[test]
    fn reports_invalid_core_and_identifier_inputs() {
        assert_eq!(
            parse_version("01.0.0"),
            Err(VersionError::LeadingZero { at: 0 })
        );
        assert_eq!(
            parse_version("1.0"),
            Err(VersionError::MissingCore { at: 3 })
        );
        assert_eq!(
            parse_version("1.0.0-01"),
            Err(VersionError::LeadingZero { at: 6 })
        );
    }

    #[test]
    fn range_matches_conjunction_and_prerelease_policy() {
        let range = parse_range(">=2.1.0 <3.0.0").expect("valid range");
        assert!(matches(&version("2.5.0"), &range));
        assert!(!matches(&version("3.0.0"), &range));

        let prerelease_range =
            parse_range(">=2.1.3-beta <2.1.3").expect("valid prerelease range");
        assert!(matches(&version("2.1.3-rc"), &prerelease_range));
        assert!(!matches(
            &version("2.1.4-rc"),
            &prerelease_range
        ));
    }

    #[test]
    fn empty_range_is_rejected() {
        assert_eq!(parse_range(" \t\n"), Err(RangeError::Empty { at: 0 }));
    }
}