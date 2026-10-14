use std::cmp::Ordering;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub prerelease: Vec<String>,
    pub build: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Prerelease,
    Build,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    EmptyInput,
    ComponentCount { found: usize },
    EmptyComponent { index: usize },
    InvalidDigit { index: usize, offset: usize },
    LeadingZero { index: usize },
    Overflow { index: usize },
    EmptyIdentifier { section: Section, index: usize },
    InvalidIdentifier {
        section: Section,
        index: usize,
        offset: usize,
    },
    NumericLeadingZero { index: usize },
}

fn parse_core(parts: &[&str]) -> Result<[u64; 3], ParseError> {
    if parts.len() != 3 {
        return Err(ParseError::ComponentCount { found: parts.len() });
    }

    let mut values = [0; 3];
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() {
            return Err(ParseError::EmptyComponent { index });
        }
        if let Some(offset) = part.bytes().position(|b| !b.is_ascii_digit()) {
            return Err(ParseError::InvalidDigit { index, offset });
        }
        if part.len() > 1 && part.starts_with('0') {
            return Err(ParseError::LeadingZero { index });
        }

        let mut value = 0u64;
        for digit in part.bytes() {
            value = value
                .checked_mul(10)
                .and_then(|n| n.checked_add((digit - b'0') as u64))
                .ok_or(ParseError::Overflow { index })?;
        }
        values[index] = value;
    }
    Ok(values)
}

fn parse_identifiers(
    text: &str,
    section: Section,
) -> Result<Vec<String>, ParseError> {
    let mut identifiers = Vec::new();
    for (index, identifier) in text.split('.').enumerate() {
        if identifier.is_empty() {
            return Err(ParseError::EmptyIdentifier { section, index });
        }
        if let Some(offset) = identifier.bytes().position(|b| {
            !(b.is_ascii_alphanumeric() || b == b'-')
        }) {
            return Err(ParseError::InvalidIdentifier {
                section,
                index,
                offset,
            });
        }
        if section == Section::Prerelease
            && identifier.len() > 1
            && identifier.starts_with('0')
            && identifier.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(ParseError::NumericLeadingZero { index });
        }
        identifiers.push(identifier.to_owned());
    }
    Ok(identifiers)
}

pub fn parse_version(input: &str) -> Result<Version, ParseError> {
    if input.is_empty() {
        return Err(ParseError::EmptyInput);
    }

    let (before_build, build_text) = match input.split_once('+') {
        Some((left, right)) => (left, Some(right)),
        None => (input, None),
    };
    let (core_text, prerelease_text) = match before_build.split_once('-') {
        Some((left, right)) => (left, Some(right)),
        None => (before_build, None),
    };

    let core_parts: Vec<&str> = core_text.split('.').collect();
    let [major, minor, patch] = parse_core(&core_parts)?;

    let prerelease = match prerelease_text {
        Some(text) => parse_identifiers(text, Section::Prerelease)?,
        None => Vec::new(),
    };
    let build = match build_text {
        Some(text) => parse_identifiers(text, Section::Build)?,
        None => Vec::new(),
    };

    Ok(Version {
        major,
        minor,
        patch,
        prerelease,
        build,
    })
}

fn numeric_identifier(identifier: &str) -> bool {
    !identifier.is_empty() && identifier.bytes().all(|b| b.is_ascii_digit())
}

fn compare_numeric_strings(a: &str, b: &str) -> Ordering {
    let a = a.trim_start_matches('0');
    let b = b.trim_start_matches('0');
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

pub fn precedence(a: &Version, b: &Version) -> Ordering {
    let core_order = (a.major, a.minor, a.patch).cmp(&(b.major, b.minor, b.patch));
    if core_order != Ordering::Equal {
        return core_order;
    }

    match (a.prerelease.is_empty(), b.prerelease.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        (false, false) => {}
    }

    for (left, right) in a.prerelease.iter().zip(&b.prerelease) {
        let left_numeric = numeric_identifier(left);
        let right_numeric = numeric_identifier(right);
        let order = match (left_numeric, right_numeric) {
            (true, true) => compare_numeric_strings(left, right),
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => left.as_bytes().cmp(right.as_bytes()),
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    a.prerelease.len().cmp(&b.prerelease.len())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Lt,
    Le,
    Eq,
    Ge,
    Gt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comparator {
    pub op: Op,
    pub version: Version,
    pub allows_prerelease: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Range {
    pub alternatives: Vec<Vec<Comparator>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RangeError {
    EmptyAlternative {
        alternative: usize,
    },
    InvalidVersion {
        alternative: usize,
        index: usize,
        error: ParseError,
    },
    InvalidToken {
        alternative: usize,
        index: usize,
    },
    BoundOverflow {
        alternative: usize,
        index: usize,
    },
}

fn ascii_whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

fn split_ascii_whitespace(input: &str) -> Vec<&str> {
    let bytes = input.as_bytes();
    let mut tokens = Vec::new();
    let mut start = None;
    for (index, byte) in bytes.iter().copied().enumerate() {
        if ascii_whitespace(byte) {
            if let Some(begin) = start.take() {
                tokens.push(&input[begin..index]);
            }
        } else if start.is_none() {
            start = Some(index);
        }
    }
    if let Some(begin) = start {
        tokens.push(&input[begin..]);
    }
    tokens
}

fn parsed_comparator(op: Op, version: Version) -> Comparator {
    Comparator {
        allows_prerelease: !version.prerelease.is_empty(),
        version,
        op,
    }
}

fn parse_endpoint(
    text: &str,
    alternative: usize,
    index: usize,
) -> Result<(Version, usize), RangeError> {
    let parts: Vec<&str> = text.split('.').collect();
    if parts.is_empty() || parts.len() > 3 {
        return Err(RangeError::InvalidToken { alternative, index });
    }
    for part in &parts {
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return Err(RangeError::InvalidToken { alternative, index });
        }
    }
    if parts.len() > 1 && parts[0].len() > 1 && parts[0].starts_with('0') {
        return Err(RangeError::InvalidToken { alternative, index });
    }
    for (i, part) in parts.iter().enumerate() {
        if part.len() > 1 && part.starts_with('0') {
            return Err(RangeError::InvalidToken { alternative, index });
        }
        let mut value = 0u64;
        for digit in part.bytes() {
            value = value
                .checked_mul(10)
                .and_then(|n| n.checked_add((digit - b'0') as u64))
                .ok_or(RangeError::InvalidToken { alternative, index })?;
        }
        let _ = (i, value);
    }

    let supplied = parts.len();
    let version = if supplied == 3 {
        parse_version(text).map_err(|_| RangeError::InvalidToken { alternative, index })?
    } else {
        let mut values = [0u64; 3];
        for (i, part) in parts.iter().enumerate() {
            values[i] = part.parse().map_err(|_| RangeError::InvalidToken {
                alternative,
                index,
            })?;
        }
        Version {
            major: values[0],
            minor: values[1],
            patch: values[2],
            prerelease: Vec::new(),
            build: Vec::new(),
        }
    };
    Ok((version, supplied))
}

fn upper_bound(
    version: &Version,
    axis: usize,
    alternative: usize,
    index: usize,
) -> Result<Version, RangeError> {
    let mut values = [version.major, version.minor, version.patch];
    values[axis] = values[axis]
        .checked_add(1)
        .ok_or(RangeError::BoundOverflow { alternative, index })?;
    for value in values.iter_mut().skip(axis + 1) {
        *value = 0;
    }
    Ok(Version {
        major: values[0],
        minor: values[1],
        patch: values[2],
        prerelease: vec!["0".to_owned()],
        build: Vec::new(),
    })
}

fn lower_comparator(version: Version) -> Comparator {
    parsed_comparator(Op::Ge, version)
}

fn upper_comparator(version: Version) -> Comparator {
    Comparator {
        op: Op::Lt,
        version,
        allows_prerelease: false,
    }
}

fn expand_endpoint_range(
    token: &str,
    alternative: usize,
    index: usize,
) -> Result<Vec<Comparator>, RangeError> {
    if token == "*" || token == "x" || token == "X" {
        return Ok(Vec::new());
    }

    let (prefix, mode) = if let Some(rest) = token.strip_prefix('^') {
        (rest, Some('^'))
    } else if let Some(rest) = token.strip_prefix('~') {
        (rest, Some('~'))
    } else {
        (token, None)
    };

    if prefix.contains('*') || prefix.contains('x') || prefix.contains('X') {
        return Err(RangeError::InvalidToken { alternative, index });
    }

    let (version, supplied) = parse_endpoint(prefix, alternative, index)?;
    if let Some(mode) = mode {
        let axis = if mode == '^' {
            if version.major > 0 || supplied == 1 {
                0
            } else if version.minor > 0 || supplied == 2 {
                1
            } else {
                2
            }
        } else if supplied == 1 {
            0
        } else {
            1
        };
        let upper = upper_bound(&version, axis, alternative, index)?;
        return Ok(vec![lower_comparator(version), upper_comparator(upper)]);
    }

    if supplied == 3 {
        return Ok(vec![parsed_comparator(Op::Eq, version)]);
    }
    let upper = upper_bound(&version, supplied - 1, alternative, index)?;
    Ok(vec![lower_comparator(version), upper_comparator(upper)])
}

fn expand_wildcard(
    token: &str,
    alternative: usize,
    index: usize,
) -> Result<Option<Vec<Comparator>>, RangeError> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.iter().any(|part| matches!(*part, "*" | "x" | "X")) {
        let wildcard_positions: Vec<usize> = parts
            .iter()
            .enumerate()
            .filter_map(|(i, part)| matches!(*part, "*" | "x" | "X").then_some(i))
            .collect();
        if wildcard_positions.len() != 1
            || wildcard_positions[0] + 1 != parts.len()
            || parts.len() > 3
            || wildcard_positions[0] == 0
        {
            return Err(RangeError::InvalidToken { alternative, index });
        }
        let supplied = parts.len() - 1;
        let prefix = parts[..supplied].join(".");
        let (version, parsed_supplied) = parse_endpoint(&prefix, alternative, index)?;
        if parsed_supplied != supplied {
            return Err(RangeError::InvalidToken { alternative, index });
        }
        let upper = upper_bound(&version, supplied - 1, alternative, index)?;
        return Ok(Some(vec![
            lower_comparator(version),
            upper_comparator(upper),
        ]));
    }
    Ok(None)
}

fn expand_ordinary_token(
    token: &str,
    alternative: usize,
    index: usize,
) -> Result<Vec<Comparator>, RangeError> {
    for (prefix, op) in [
        (">=", Op::Ge),
        ("<=", Op::Le),
        (">", Op::Gt),
        ("<", Op::Lt),
        ("=", Op::Eq),
    ] {
        if let Some(remainder) = token.strip_prefix(prefix) {
            let version = parse_version(remainder).map_err(|error| {
                RangeError::InvalidVersion {
                    alternative,
                    index,
                    error,
                }
            })?;
            return Ok(vec![parsed_comparator(op, version)]);
        }
    }

    if let Some(expanded) = expand_wildcard(token, alternative, index)? {
        return Ok(expanded);
    }
    expand_endpoint_range(token, alternative, index)
}

fn expand_hyphen(
    tokens: &[&str],
    alternative: usize,
) -> Result<Vec<Comparator>, RangeError> {
    let (left, _) = parse_endpoint(tokens[0], alternative, 0)?;
    let (right, right_supplied) = parse_endpoint(tokens[2], alternative, 2)?;
    let mut comparators = vec![lower_comparator(left)];
    if right_supplied == 3 {
        comparators.push(parsed_comparator(Op::Le, right));
    } else {
        comparators.push(upper_comparator(upper_bound(
            &right,
            right_supplied - 1,
            alternative,
            2,
        )?));
    }
    Ok(comparators)
}

pub fn parse_range(input: &str) -> Result<Range, RangeError> {
    let alternatives_text: Vec<&str> = input.split("||").collect();
    let only_ascii_whitespace = input.bytes().all(ascii_whitespace);
    if only_ascii_whitespace {
        return Ok(Range {
            alternatives: vec![Vec::new()],
        });
    }

    let mut alternatives = Vec::with_capacity(alternatives_text.len());
    for (alternative, text) in alternatives_text.into_iter().enumerate() {
        let tokens = split_ascii_whitespace(text);
        if tokens.is_empty() {
            return Err(RangeError::EmptyAlternative { alternative });
        }

        let comparators = if tokens.len() == 3 && tokens[1] == "-" {
            expand_hyphen(&tokens, alternative)?
        } else {
            let mut output = Vec::new();
            for (index, token) in tokens.iter().enumerate() {
                output.extend(expand_ordinary_token(token, alternative, index)?);
            }
            output
        };
        alternatives.push(comparators);
    }
    Ok(Range { alternatives })
}

fn comparator_matches(version: &Version, comparator: &Comparator) -> bool {
    let order = precedence(version, &comparator.version);
    match comparator.op {
        Op::Lt => order == Ordering::Less,
        Op::Le => order != Ordering::Greater,
        Op::Eq => order == Ordering::Equal,
        Op::Ge => order != Ordering::Less,
        Op::Gt => order == Ordering::Greater,
    }
}

pub fn satisfies(version: &Version, range: &Range) -> bool {
    range.alternatives.iter().any(|alternative| {
        if !alternative
            .iter()
            .all(|comparator| comparator_matches(version, comparator))
        {
            return false;
        }

        if version.prerelease.is_empty() {
            return true;
        }

        alternative.iter().any(|comparator| {
            comparator.allows_prerelease
                && (comparator.version.major, comparator.version.minor, comparator.version.patch)
                    == (version.major, version.minor, version.patch)
        })
    })
}

pub fn max_satisfying<'a>(
    versions: &'a [Version],
    range: &Range,
) -> Option<&'a Version> {
    let mut best: Option<&Version> = None;
    for version in versions {
        if satisfies(version, range)
            && best
                .map(|current| precedence(version, current) == Ordering::Greater)
                .unwrap_or(true)
        {
            best = Some(version);
        }
    }
    best
}