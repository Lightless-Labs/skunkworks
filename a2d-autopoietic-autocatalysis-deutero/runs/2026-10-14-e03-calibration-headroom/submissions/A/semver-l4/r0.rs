use std::cmp::Ordering;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub prerelease: Vec<String>,
    pub build: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
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
    InvalidIdentifier { section: Section, index: usize, offset: usize },
    NumericLeadingZero { index: usize },
}

fn parse_core(core: &str) -> Result<(u64, u64, u64), ParseError> {
    let parts: Vec<&str> = core.split('.').collect();
    if parts.len() != 3 {
        return Err(ParseError::ComponentCount { found: parts.len() });
    }

    let mut values = [0u64; 3];
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() {
            return Err(ParseError::EmptyComponent { index });
        }
        if let Some(offset) = part.bytes().position(|byte| !byte.is_ascii_digit()) {
            return Err(ParseError::InvalidDigit { index, offset });
        }
        if part.len() > 1 && part.as_bytes()[0] == b'0' {
            return Err(ParseError::LeadingZero { index });
        }
        values[index] = part.parse::<u64>().map_err(|_| ParseError::Overflow { index })?;
    }
    Ok((values[0], values[1], values[2]))
}

fn parse_identifiers(text: &str, section: Section) -> Result<Vec<String>, ParseError> {
    let mut result = Vec::new();
    for (index, identifier) in text.split('.').enumerate() {
        if identifier.is_empty() {
            return Err(ParseError::EmptyIdentifier { section, index });
        }
        if let Some(offset) = identifier.bytes().position(|byte| {
            !(byte.is_ascii_alphanumeric() || byte == b'-')
        }) {
            return Err(ParseError::InvalidIdentifier { section, index, offset });
        }
        if section == Section::Prerelease
            && identifier.len() > 1
            && identifier.starts_with('0')
            && identifier.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(ParseError::NumericLeadingZero { index });
        }
        result.push(identifier.to_owned());
    }
    Ok(result)
}

pub fn parse_version(input: &str) -> Result<Version, ParseError> {
    if input.is_empty() {
        return Err(ParseError::EmptyInput);
    }

    let (before_build, build_text) = match input.split_once('+') {
        Some((before, after)) => (before, Some(after)),
        None => (input, None),
    };
    let (core, prerelease_text) = match before_build.split_once('-') {
        Some((before, after)) => (before, Some(after)),
        None => (before_build, None),
    };

    let (major, minor, patch) = parse_core(core)?;
    let prerelease = match prerelease_text {
        Some(text) => parse_identifiers(text, Section::Prerelease)?,
        None => Vec::new(),
    };
    let build = match build_text {
        Some(text) => parse_identifiers(text, Section::Build)?,
        None => Vec::new(),
    };

    Ok(Version { major, minor, patch, prerelease, build })
}

fn numeric_identifier(identifier: &str) -> bool {
    !identifier.is_empty() && identifier.bytes().all(|byte| byte.is_ascii_digit())
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
            (false, false) => left.cmp(right),
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    a.prerelease.len().cmp(&b.prerelease.len())
}

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
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
    EmptyAlternative { alternative: usize },
    InvalidVersion { alternative: usize, index: usize, error: ParseError },
    InvalidToken { alternative: usize, index: usize },
    BoundOverflow { alternative: usize, index: usize },
}

fn is_ascii_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

fn ascii_tokens(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut start = None;
    for (i, &byte) in bytes.iter().enumerate() {
        if is_ascii_space(byte) {
            if let Some(begin) = start.take() {
                tokens.push(&text[begin..i]);
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(begin) = start {
        tokens.push(&text[begin..])
    }
    tokens
}

fn is_numeric_component(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn parse_numeric_parts(token: &str) -> Option<(Version, usize)> {
    let (base, suffix) = match token.find(['-', '+']) {
        Some(position) => (&token[..position], Some(&token[position..])),
        None => (token, None),
    };
    let parts: Vec<&str> = base.split('.').collect();
    if !(1..=3).contains(&parts.len()) || parts.iter().any(|part| !is_numeric_component(part)) {
        return None;
    }
    if suffix.is_some() && parts.len() != 3 {
        return None;
    }

    let padded = format!(
        "{}.{}.{}",
        parts[0],
        parts.get(1).copied().unwrap_or("0"),
        parts.get(2).copied().unwrap_or("0")
    );
    let full = match suffix {
        Some(suffix) => format!("{padded}{suffix}"),
        None => padded,
    };
    let version = parse_version(&full).ok()?;
    Some((version, parts.len()))
}

fn upper_bound(
    version: &Version,
    axis: usize,
    alternative: usize,
    index: usize,
) -> Result<Version, RangeError> {
    let mut values = [version.major, version.minor, version.patch];
    values[axis] = values[axis].checked_add(1).ok_or(
        RangeError::BoundOverflow { alternative, index },
    )?;
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

fn comparator(op: Op, version: Version) -> Comparator {
    let allows_prerelease = !version.prerelease.is_empty();
    Comparator { op, version, allows_prerelease }
}

fn invalid_token(alternative: usize, index: usize) -> RangeError {
    RangeError::InvalidToken { alternative, index }
}

fn parse_shorthand_endpoint(
    token: &str,
    alternative: usize,
    index: usize,
) -> Result<(Version, usize), RangeError> {
    parse_numeric_parts(token).ok_or_else(|| invalid_token(alternative, index))
}

fn expand_hyphen(
    tokens: &[&str],
    alternative: usize,
) -> Result<Vec<Comparator>, RangeError> {
    let (left, _) = parse_shorthand_endpoint(tokens[0], alternative, 0)?;
    let (right, right_count) = parse_shorthand_endpoint(tokens[2], alternative, 2)?;
    let mut result = vec![comparator(Op::Ge, left)];
    if right_count == 3 {
        result.push(comparator(Op::Le, right));
    } else {
        result.push(comparator(
            Op::Lt,
            upper_bound(&right, right_count - 1, alternative, 2)?,
        ));
    }
    Ok(result)
}

fn expand_token(
    token: &str,
    alternative: usize,
    index: usize,
) -> Result<Vec<Comparator>, RangeError> {
    let explicit = if token.starts_with(">=") {
        Some((Op::Ge, 2))
    } else if token.starts_with("<=") {
        Some((Op::Le, 2))
    } else if token.starts_with('>') {
        Some((Op::Gt, 1))
    } else if token.starts_with('<') {
        Some((Op::Lt, 1))
    } else if token.starts_with('=') {
        Some((Op::Eq, 1))
    } else {
        None
    };

    if let Some((op, prefix_len)) = explicit {
        let version = parse_version(&token[prefix_len..]).map_err(|error| {
            RangeError::InvalidVersion { alternative, index, error }
        })?;
        return Ok(vec![comparator(op, version)]);
    }

    if matches!(token, "*" | "x" | "X") {
        return Ok(Vec::new());
    }

    if let Some(rest) = token.strip_prefix('^') {
        let (version, count) = parse_shorthand_endpoint(rest, alternative, index)?;
        let axis = if version.major > 0 || count == 1 {
            0
        } else if version.minor > 0 || count == 2 {
            1
        } else {
            2
        };
        return Ok(vec![
            comparator(Op::Ge, version.clone()),
            comparator(Op::Lt, upper_bound(&version, axis, alternative, index)?),
        ]);
    }

    if let Some(rest) = token.strip_prefix('~') {
        let (version, count) = parse_shorthand_endpoint(rest, alternative, index)?;
        let axis = if count == 1 { 0 } else { 1 };
        return Ok(vec![
            comparator(Op::Ge, version.clone()),
            comparator(Op::Lt, upper_bound(&version, axis, alternative, index)?),
        ]);
    }

    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() <= 3 {
        if let Some(last) = parts.last() {
            if matches!(*last, "*" | "x" | "X") {
                if parts.len() == 1 {
                    return Ok(Vec::new());
                }
                if parts[..parts.len() - 1]
                    .iter()
                    .any(|part| !is_numeric_component(part))
                {
                    return Err(invalid_token(alternative, index));
                }
                let prefix = parts[..parts.len() - 1].join(".");
                let (version, count) = parse_shorthand_endpoint(&prefix, alternative, index)?;
                let axis = count - 1;
                return Ok(vec![
                    comparator(Op::Ge, version.clone()),
                    comparator(Op::Lt, upper_bound(&version, axis, alternative, index)?),
                ]);
            }
        }
    }

    let (version, count) = parse_shorthand_endpoint(token, alternative, index)?;
    if count == 3 {
        Ok(vec![comparator(Op::Eq, version)])
    } else {
        Ok(vec![
            comparator(Op::Ge, version.clone()),
            comparator(Op::Lt, upper_bound(&version, count - 1, alternative, index)?),
        ])
    }
}

fn parse_alternative(
    text: &str,
    alternative: usize,
) -> Result<Vec<Comparator>, RangeError> {
    let tokens = ascii_tokens(text);
    if tokens.is_empty() {
        return Err(RangeError::EmptyAlternative { alternative });
    }

    if tokens.len() == 3 && tokens[1] == "-" {
        return expand_hyphen(&tokens, alternative);
    }

    let mut comparators = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        comparators.extend(expand_token(token, alternative, index)?);
    }
    Ok(comparators)
}

pub fn parse_range(input: &str) -> Result<Range, RangeError> {
    if input.is_empty() || input.bytes().all(is_ascii_space) {
        return Ok(Range { alternatives: vec![Vec::new()] });
    }

    let mut alternatives = Vec::new();
    for (index, alternative) in input.split("||").enumerate() {
        alternatives.push(parse_alternative(alternative, index)?);
    }
    Ok(Range { alternatives })
}

fn comparator_matches(version: &Version, comparator: &Comparator) -> bool {
    match comparator.op {
        Op::Lt => precedence(version, &comparator.version).is_lt(),
        Op::Le => !precedence(version, &comparator.version).is_gt(),
        Op::Eq => precedence(version, &comparator.version).is_eq(),
        Op::Ge => !precedence(version, &comparator.version).is_lt(),
        Op::Gt => precedence(version, &comparator.version).is_gt(),
    }
}

pub fn satisfies(version: &Version, range: &Range) -> bool {
    range.alternatives.iter().any(|alternative| {
        if !alternative.iter().all(|comparator| comparator_matches(version, comparator)) {
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

pub fn max_satisfying<'a>(versions: &'a [Version], range: &Range) -> Option<&'a Version> {
    let mut best: Option<&Version> = None;
    for version in versions {
        if satisfies(version, range)
            && best.is_none_or(|current| precedence(version, current).is_gt())
        {
            best = Some(version);
        }
    }
    best
}