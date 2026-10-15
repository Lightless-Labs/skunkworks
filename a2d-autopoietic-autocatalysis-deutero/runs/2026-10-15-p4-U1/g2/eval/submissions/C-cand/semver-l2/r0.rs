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

fn parse_core(s: &str, pos: &mut usize) -> Result<u64, VersionError> {
    let bytes = s.as_bytes();
    let start = *pos;
    while *pos < bytes.len() && bytes[*pos].is_ascii_digit() {
        *pos += 1;
    }

    if *pos == start {
        if *pos == bytes.len() || matches!(bytes[*pos], b'.' | b'-' | b'+') {
            return Err(VersionError::MissingCore { at: start });
        }
        return Err(VersionError::InvalidChar { at: start });
    }

    if *pos - start > 1 && bytes[start] == b'0' {
        return Err(VersionError::LeadingZero { at: start });
    }

    let mut value = 0u64;
    for &byte in &bytes[start..*pos] {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u64))
            .ok_or(VersionError::Overflow { at: start })?;
    }
    Ok(value)
}

fn require_dot(s: &str, pos: &mut usize) -> Result<(), VersionError> {
    let bytes = s.as_bytes();
    if *pos >= bytes.len() {
        return Err(VersionError::MissingCore { at: *pos });
    }
    match bytes[*pos] {
        b'.' => {
            *pos += 1;
            Ok(())
        }
        b'-' | b'+' => Err(VersionError::MissingCore { at: *pos }),
        _ => Err(VersionError::InvalidChar { at: *pos }),
    }
}

fn parse_identifiers(
    s: &str,
    pos: &mut usize,
    stop_at_plus: bool,
    is_pre: bool,
) -> Result<Vec<String>, VersionError> {
    let bytes = s.as_bytes();
    let mut identifiers = Vec::new();

    loop {
        let start = *pos;
        while *pos < bytes.len()
            && bytes[*pos] != b'.'
            && !(stop_at_plus && bytes[*pos] == b'+')
        {
            *pos += 1;
        }

        if *pos == start {
            return Err(VersionError::EmptyIdentifier { at: start });
        }

        let identifier = &s[start..*pos];
        for (offset, &byte) in bytes[start..*pos].iter().enumerate() {
            if !(byte.is_ascii_alphanumeric() || byte == b'-') {
                return Err(VersionError::InvalidChar { at: start + offset });
            }
        }

        if is_pre
            && identifier.len() > 1
            && identifier.as_bytes()[0] == b'0'
            && identifier.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(VersionError::LeadingZero { at: start });
        }

        identifiers.push(identifier.to_owned());

        if *pos == bytes.len() || (stop_at_plus && bytes[*pos] == b'+') {
            break;
        }

        *pos += 1;
        if *pos == bytes.len() || (stop_at_plus && bytes[*pos] == b'+') {
            return Err(VersionError::EmptyIdentifier { at: *pos });
        }
    }

    Ok(identifiers)
}

pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }

    let mut pos = 0;
    let major = parse_core(s, &mut pos)?;
    require_dot(s, &mut pos)?;
    let minor = parse_core(s, &mut pos)?;
    require_dot(s, &mut pos)?;
    let patch = parse_core(s, &mut pos)?;

    let mut pre = Vec::new();
    let mut build = Vec::new();

    if pos < s.len() {
        match s.as_bytes()[pos] {
            b'-' => {
                pos += 1;
                pre = parse_identifiers(s, &mut pos, true, true)?;
                if pos < s.len() && s.as_bytes()[pos] == b'+' {
                    pos += 1;
                    build = parse_identifiers(s, &mut pos, false, false)?;
                }
            }
            b'+' => {
                pos += 1;
                build = parse_identifiers(s, &mut pos, false, false)?;
            }
            _ => return Err(VersionError::InvalidChar { at: pos }),
        }
    }

    Ok(Version {
        major,
        minor,
        patch,
        pre,
        build,
    })
}

fn is_numeric(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|byte| byte.is_ascii_digit())
}

fn compare_pre(a: &[String], b: &[String]) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }

    for (x, y) in a.iter().zip(b) {
        let xn = is_numeric(x);
        let yn = is_numeric(y);
        let order = match (xn, yn) {
            (true, true) => x
                .len()
                .cmp(&y.len())
                .then_with(|| x.as_bytes().cmp(y.as_bytes())),
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => x.as_bytes().cmp(y.as_bytes()),
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RangeError {
    Empty { at: usize },
    Syntax { at: usize },
    Version { at: usize, error: VersionError },
}

#[derive(Clone, Copy, Debug)]
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
    anchor: bool,
}

#[derive(Clone, Debug)]
pub struct Range {
    arms: Vec<Vec<Comparator>>,
}

fn cmp_matches(version: &Version, comparator: &Comparator) -> bool {
    let order = compare_versions(version, &comparator.version);
    match comparator.op {
        Op::Eq => order == Ordering::Equal,
        Op::Lt => order == Ordering::Less,
        Op::Le => order != Ordering::Greater,
        Op::Gt => order == Ordering::Greater,
        Op::Ge => order != Ordering::Less,
    }
}

fn whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

fn split_tokens(s: &str, start: usize, end: usize) -> Vec<(usize, &str)> {
    let bytes = s.as_bytes();
    let mut tokens = Vec::new();
    let mut i = start;

    while i < end {
        while i < end && whitespace(bytes[i]) {
            i += 1;
        }
        if i == end {
            break;
        }

        let token_start = i;
        while i < end && !whitespace(bytes[i]) {
            i += 1;
        }
        tokens.push((token_start, &s[token_start..i]));
    }

    tokens
}

fn version_operand(s: &str, start: usize) -> Result<Version, RangeError> {
    parse_version(s).map_err(|error| RangeError::Version { at: start, error })
}

fn operand_as_partial_or_full(
    text: &str,
    start: usize,
) -> Result<(Version, usize, bool), RangeError> {
    let fields: Vec<&str> = text.split('.').collect();
    let is_full = text.contains('-')
        || text.contains('+')
        || (fields.len() >= 3 && !fields.iter().any(|field| matches!(*field, "*" | "x" | "X")));

    if is_full {
        return Ok((version_operand(text, start)?, 3, true));
    }

    if fields.is_empty() || fields.len() > 3 {
        return Err(RangeError::Syntax { at: start });
    }

    let mut numbers = Vec::new();
    let mut wildcard_seen = false;

    for field in fields {
        if field.is_empty() {
            return Err(RangeError::Syntax { at: start });
        }
        if matches!(field, "*" | "x" | "X") {
            wildcard_seen = true;
            continue;
        }
        if wildcard_seen || !field.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(RangeError::Syntax { at: start });
        }
        if field.len() > 1 && field.as_bytes()[0] == b'0' {
            return Err(RangeError::Syntax { at: start });
        }

        let mut number = 0u64;
        for byte in field.bytes() {
            number = number
                .checked_mul(10)
                .and_then(|v| v.checked_add((byte - b'0') as u64))
                .ok_or(RangeError::Syntax { at: start })?;
        }
        numbers.push(number);
    }

    let k = numbers.len();
    let version = Version {
        major: *numbers.first().unwrap_or(&0),
        minor: *numbers.get(1).unwrap_or(&0),
        patch: *numbers.get(2).unwrap_or(&0),
        pre: Vec::new(),
        build: Vec::new(),
    };
    Ok((version, k, false))
}

fn upper_bound(version: &Version, field: usize, at: usize) -> Result<Version, RangeError> {
    let mut upper = version.clone();
    upper.pre = vec!["0".to_owned()];
    upper.build.clear();

    let value = match field {
        0 => &mut upper.major,
        1 => &mut upper.minor,
        _ => &mut upper.patch,
    };
    *value = value
        .checked_add(1)
        .ok_or(RangeError::Syntax { at })?;

    match field {
        0 => {
            upper.minor = 0;
            upper.patch = 0;
        }
        1 => upper.patch = 0,
        _ => {}
    }

    Ok(upper)
}

fn add(comparators: &mut Vec<Comparator>, op: Op, version: Version, anchor: bool) {
    comparators.push(Comparator {
        op,
        version,
        anchor,
    });
}

fn expand_partial(
    text: &str,
    start: usize,
    comparators: &mut Vec<Comparator>,
) -> Result<(), RangeError> {
    let (version, k, full) = operand_as_partial_or_full(text, start)?;

    if full {
        add(
            comparators,
            Op::Eq,
            version.clone(),
            !version.pre.is_empty(),
        );
    } else if k == 1 {
        add(comparators, Op::Ge, version.clone(), false);
        add(comparators, Op::Lt, upper_bound(&version, 0, start)?, false);
    } else if k == 2 {
        add(comparators, Op::Ge, version.clone(), false);
        add(comparators, Op::Lt, upper_bound(&version, 1, start)?, false);
    }

    Ok(())
}

fn expand_caret_tilde(
    text: &str,
    start: usize,
    caret: bool,
    comparators: &mut Vec<Comparator>,
) -> Result<(), RangeError> {
    let (version, k, full) = operand_as_partial_or_full(text, start)?;
    if k == 0 {
        return Err(RangeError::Syntax { at: start });
    }

    add(
        comparators,
        Op::Ge,
        version.clone(),
        full && !version.pre.is_empty(),
    );

    let field = if caret {
        if version.major > 0 || k == 1 {
            0
        } else if version.minor > 0 || k == 2 {
            1
        } else {
            2
        }
    } else if k == 1 {
        0
    } else {
        1
    };

    add(
        comparators,
        Op::Lt,
        upper_bound(&version, field, start)?,
        false,
    );
    Ok(())
}

fn parse_arm(tokens: &[(usize, &str)]) -> Result<Vec<Comparator>, RangeError> {
    if let Some((index, (at, _))) = tokens
        .iter()
        .enumerate()
        .find(|(_, (_, token))| *token == "-")
    {
        if tokens.len() != 3 || index != 1 {
            return Err(RangeError::Syntax { at: *at });
        }

        let (lower, lower_k, lower_full) =
            operand_as_partial_or_full(tokens[0].1, tokens[0].0)?;
        let (upper, upper_k, upper_full) =
            operand_as_partial_or_full(tokens[2].1, tokens[2].0)?;

        if lower_k == 0 {
            return Err(RangeError::Syntax { at: tokens[0].0 });
        }
        if upper_k == 0 {
            return Err(RangeError::Syntax { at: tokens[2].0 });
        }

        let mut comparators = Vec::new();
        add(
            &mut comparators,
            Op::Ge,
            lower.clone(),
            lower_full && !lower.pre.is_empty(),
        );

        if upper_full {
            add(
                &mut comparators,
                Op::Le,
                upper.clone(),
                !upper.pre.is_empty(),
            );
        } else {
            let field = if upper_k == 1 { 0 } else { 1 };
            add(
                &mut comparators,
                Op::Lt,
                upper_bound(&upper, field, tokens[2].0)?,
                false,
            );
        }

        return Ok(comparators);
    }

    let mut comparators = Vec::new();

    for &(at, token) in tokens {
        if token == "&" || token.contains(',') || token.contains('(') || token.contains(')') {
            return Err(RangeError::Syntax { at });
        }

        if matches!(token, "*" | "x" | "X") {
            continue;
        }

        let bytes = token.as_bytes();
        match bytes[0] {
            b'^' | b'~' => {
                if token.len() == 1 {
                    return Err(RangeError::Syntax { at });
                }
                expand_caret_tilde(
                    &token[1..],
                    at + 1,
                    bytes[0] == b'^',
                    &mut comparators,
                )?;
            }
            b'<' | b'>' | b'=' => {
                let (op, length) = if token.starts_with("<=") {
                    (Op::Le, 2)
                } else if token.starts_with(">=") {
                    (Op::Ge, 2)
                } else {
                    (
                        match bytes[0] {
                            b'<' => Op::Lt,
                            b'>' => Op::Gt,
                            _ => Op::Eq,
                        },
                        1,
                    )
                };

                let operand = &token[length..];
                if operand.is_empty() || !operand.as_bytes()[0].is_ascii_digit() {
                    return Err(RangeError::Syntax { at });
                }

                let version = version_operand(operand, at + length)?;
                add(
                    &mut comparators,
                    op,
                    version.clone(),
                    !version.pre.is_empty(),
                );
            }
            b'0'..=b'9' => expand_partial(token, at, &mut comparators)?,
            _ => return Err(RangeError::Syntax { at }),
        }
    }

    Ok(comparators)
}

pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    let bytes = s.as_bytes();
    let mut arms = Vec::new();
    let mut arm_start = 0usize;
    let mut i = 0usize;
    let mut saw_separator = false;

    while i < bytes.len() {
        if bytes[i..].starts_with(b"||")
            && (i == 0 || whitespace(bytes[i - 1]))
            && (i + 2 == bytes.len() || whitespace(bytes[i + 2]))
        {
            let tokens = split_tokens(s, arm_start, i);
            if tokens.is_empty() {
                return Err(RangeError::Empty { at: i });
            }
            arms.push(parse_arm(&tokens)?);
            saw_separator = true;
            i += 2;
            while i < bytes.len() && whitespace(bytes[i]) {
                i += 1;
            }
            arm_start = i;
            continue;
        }
        i += 1;
    }

    let tokens = split_tokens(s, arm_start, bytes.len());
    if tokens.is_empty() {
        if saw_separator {
            return Err(RangeError::Empty { at: bytes.len() });
        }
        return Err(RangeError::Empty { at: 0 });
    }

    arms.push(parse_arm(&tokens)?);
    Ok(Range { arms })
}

pub fn matches(version: &Version, range: &Range) -> bool {
    range.arms.iter().any(|arm| {
        if !arm.iter().all(|comparator| cmp_matches(version, comparator)) {
            return false;
        }

        if version.pre.is_empty() {
            return true;
        }

        arm.iter().any(|comparator| {
            comparator.anchor
                && (
                    comparator.version.major,
                    comparator.version.minor,
                    comparator.version.patch,
                ) == (version.major, version.minor, version.patch)
        })
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError {
    pub index: usize,
    pub error: VersionError,
}

fn satisfying_extreme(
    candidates: &[String],
    range: &Range,
    greatest: bool,
) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::with_capacity(candidates.len());
    for (index, candidate) in candidates.iter().enumerate() {
        let version = parse_version(candidate)
            .map_err(|error| CandidateError { index, error })?;
        parsed.push(version);
    }

    let mut best: Option<usize> = None;
    for i in 0..candidates.len() {
        if !matches(&parsed[i], range) {
            continue;
        }

        if let Some(old) = best {
            let order = compare_versions(&parsed[i], &parsed[old]);
            if (greatest && order == Ordering::Greater)
                || (!greatest && order == Ordering::Less)
            {
                best = Some(i);
            }
        } else {
            best = Some(i);
        }
    }

    Ok(best.map(|index| candidates[index].clone()))
}

pub fn max_satisfying(
    candidates: &[String],
    range: &Range,
) -> Result<Option<String>, CandidateError> {
    satisfying_extreme(candidates, range, true)
}

pub fn min_satisfying(
    candidates: &[String],
    range: &Range,
) -> Result<Option<String>, CandidateError> {
    satisfying_extreme(candidates, range, false)
}

fn main() {}