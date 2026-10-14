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

fn parse_core_field(s: &str, pos: &mut usize) -> Result<u64, VersionError> {
    let start = *pos;
    while let Some(byte) = byte_at(s, *pos) {
        if byte.is_ascii_digit() {
            *pos += 1;
        } else {
            break;
        }
    }
    if *pos == start {
        return match byte_at(s, start) {
            None | Some(b'.' | b'-' | b'+') => Err(VersionError::MissingCore { at: start }),
            Some(_) => Err(VersionError::InvalidChar { at: start }),
        };
    }
    let digits = &s[start..*pos];
    if digits.len() > 1 && digits.starts_with('0') {
        return Err(VersionError::LeadingZero { at: start });
    }
    digits.parse::<u64>().map_err(|_| VersionError::Overflow { at: start })
}

fn parse_identifiers(
    s: &str,
    pos: &mut usize,
    prerelease: bool,
) -> Result<Vec<String>, VersionError> {
    let mut items = Vec::new();
    loop {
        let start = *pos;
        while let Some(byte) = byte_at(s, *pos) {
            if byte.is_ascii_alphanumeric() || byte == b'-' {
                *pos += 1;
            } else {
                break;
            }
        }
        if *pos == start {
            return Err(VersionError::EmptyIdentifier { at: start });
        }
        let ident = &s[start..*pos];
        if let Some((offset, _)) = ident
            .bytes()
            .enumerate()
            .find(|(_, byte)| !(byte.is_ascii_alphanumeric() || *byte == b'-'))
        {
            return Err(VersionError::InvalidChar { at: start + offset });
        }
        if prerelease
            && ident.len() > 1
            && ident.bytes().all(|byte| byte.is_ascii_digit())
            && ident.starts_with('0')
        {
            return Err(VersionError::LeadingZero { at: start });
        }
        items.push(ident.to_string());
        match byte_at(s, *pos) {
            Some(b'.') => *pos += 1,
            _ => break,
        }
    }
    Ok(items)
}

/// Parses a strict three-part semantic version.
pub fn parse_version(s: &str) -> Result<Version, VersionError> {
    if s.is_empty() {
        return Err(VersionError::Empty);
    }
    let mut pos = 0;
    let major = parse_core_field(s, &mut pos)?;
    if byte_at(s, pos) != Some(b'.') {
        return match byte_at(s, pos) {
            None | Some(b'-' | b'+') => Err(VersionError::MissingCore { at: pos }),
            Some(_) => Err(VersionError::InvalidChar { at: pos }),
        };
    }
    pos += 1;

    let minor = parse_core_field(s, &mut pos)?;
    if byte_at(s, pos) != Some(b'.') {
        return match byte_at(s, pos) {
            None | Some(b'-' | b'+') => Err(VersionError::MissingCore { at: pos }),
            Some(_) => Err(VersionError::InvalidChar { at: pos }),
        };
    }
    pos += 1;

    let patch = parse_core_field(s, &mut pos)?;
    match byte_at(s, pos) {
        Some(b'-' | b'+') | None => {}
        Some(_) => return Err(VersionError::InvalidChar { at: pos }),
    }

    let mut pre = Vec::new();
    let mut build = Vec::new();
    if byte_at(s, pos) == Some(b'-') {
        pos += 1;
        pre = parse_identifiers(s, &mut pos, true)?;
        if byte_at(s, pos) == Some(b'+') {
            pos += 1;
            build = parse_identifiers(s, &mut pos, false)?;
        }
    } else if byte_at(s, pos) == Some(b'+') {
        pos += 1;
        build = parse_identifiers(s, &mut pos, false)?;
    }
    if pos != s.len() {
        return Err(VersionError::InvalidChar { at: pos });
    }

    Ok(Version { major, minor, patch, pre, build })
}

fn is_numeric(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|byte| byte.is_ascii_digit())
}

fn compare_ident(a: &str, b: &str) -> Ordering {
    let an = is_numeric(a);
    let bn = is_numeric(b);
    match (an, bn) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (true, true) => a.len().cmp(&b.len()).then_with(|| a.as_bytes().cmp(b.as_bytes())),
        (false, false) => a.as_bytes().cmp(b.as_bytes()),
    }
}

/// Compares semantic version precedence, ignoring build metadata.
pub fn compare_versions(a: &Version, b: &Version) -> Ordering {
    let core = (a.major, a.minor, a.patch).cmp(&(b.major, b.minor, b.patch));
    if core != Ordering::Equal {
        return core;
    }
    match (a.pre.is_empty(), b.pre.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => {
            for (left, right) in a.pre.iter().zip(b.pre.iter()) {
                let ordering = compare_ident(left, right);
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
            a.pre.len().cmp(&b.pre.len())
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RangeError {
    Empty { at: usize },
    Syntax { at: usize },
    Version { at: usize, error: VersionError },
}

#[derive(Clone, Debug)]
struct Bound {
    version: Version,
    op: Op,
    explicit: bool,
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
struct Arm {
    bounds: Vec<Bound>,
    anchors: Vec<(u64, u64, u64)>,
}

#[derive(Clone, Debug)]
pub struct Range {
    arms: Vec<Arm>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateError {
    pub index: usize,
    pub error: VersionError,
}

#[derive(Clone, Copy)]
struct Token<'a> {
    text: &'a str,
    start: usize,
}

fn is_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

fn tokenize(s: &str) -> Vec<Token<'_>> {
    let mut tokens = Vec::new();
    let mut pos = 0;
    while pos < s.len() {
        while pos < s.len() && is_space(s.as_bytes()[pos]) {
            pos += 1;
        }
        if pos == s.len() {
            break;
        }
        let start = pos;
        while pos < s.len() && !is_space(s.as_bytes()[pos]) {
            pos += 1;
        }
        tokens.push(Token { text: &s[start..pos], start });
    }
    tokens
}

fn syntax(at: usize) -> RangeError {
    RangeError::Syntax { at }
}

fn full_operand(token: Token<'_>) -> Result<Version, RangeError> {
    parse_version(token.text).map_err(|error| RangeError::Version { at: token.start, error })
}

fn partial_operand(token: Token<'_>) -> Result<(Version, usize, bool), RangeError> {
    let text = token.text;
    if text.contains('-') || text.contains('+') {
        let version = full_operand(token)?;
        return Ok((version, 3, true));
    }
    let fields: Vec<&str> = text.split('.').collect();
    if fields.len() >= 3 && fields.iter().all(|field| !matches!(*field, "*" | "x" | "X")) {
        let version = full_operand(token)?;
        return Ok((version, 3, true));
    }
    if fields.is_empty() || fields.len() > 3 || fields.iter().any(|field| field.is_empty()) {
        return Err(syntax(token.start));
    }
    let mut values = [0u64; 3];
    let mut wildcard_seen = false;
    let mut count = 0;
    for (index, field) in fields.iter().enumerate() {
        if matches!(*field, "*" | "x" | "X") {
            wildcard_seen = true;
        } else {
            if wildcard_seen
                || !field.bytes().all(|byte| byte.is_ascii_digit())
                || (field.len() > 1 && field.starts_with('0'))
            {
                return Err(syntax(token.start));
            }
            values[index] = field.parse::<u64>().map_err(|_| syntax(token.start))?;
            count += 1;
        }
    }
    Ok((
        Version { major: values[0], minor: values[1], patch: values[2], pre: Vec::new(), build: Vec::new() },
        count,
        false,
    ))
}

fn core_tuple(v: &Version) -> (u64, u64, u64) {
    (v.major, v.minor, v.patch)
}

fn add_bound(arm: &mut Arm, version: Version, op: Op, explicit: bool) {
    if explicit && !version.pre.is_empty() {
        arm.anchors.push(core_tuple(&version));
    }
    arm.bounds.push(Bound { version, op, explicit });
}

fn upper_version(major: u64, minor: u64, patch: u64) -> Version {
    Version { major, minor, patch, pre: vec!["0".to_string()], build: Vec::new() }
}

fn increment(value: u64, token_start: usize) -> Result<u64, RangeError> {
    value.checked_add(1).ok_or_else(|| syntax(token_start))
}

fn expand_bare(arm: &mut Arm, token: Token<'_>) -> Result<(), RangeError> {
    let (v, count, full) = partial_operand(token)?;
    if full {
        add_bound(arm, v, Op::Eq, true);
    } else if count == 1 {
        add_bound(arm, v.clone(), Op::Ge, false);
        add_bound(arm, upper_version(increment(v.major, token.start)?, 0, 0), Op::Lt, false);
    } else if count == 2 {
        add_bound(arm, v.clone(), Op::Ge, false);
        add_bound(arm, upper_version(v.major, increment(v.minor, token.start)?, 0), Op::Lt, false);
    }
    Ok(())
}

fn expand_caret(arm: &mut Arm, token: Token<'_>) -> Result<(), RangeError> {
    let operand = Token { text: &token.text[1..], start: token.start + 1 };
    let (v, count, _) = partial_operand(operand)?;
    if count == 0 {
        return Err(syntax(token.start));
    }
    add_bound(arm, v.clone(), Op::Ge, !v.pre.is_empty());
    let upper = if v.major > 0 || count == 1 {
        upper_version(increment(v.major, operand.start)?, 0, 0)
    } else if v.minor > 0 || count == 2 {
        upper_version(0, increment(v.minor, operand.start)?, 0)
    } else {
        upper_version(0, 0, increment(v.patch, operand.start)?)
    };
    add_bound(arm, upper, Op::Lt, false);
    Ok(())
}

fn expand_tilde(arm: &mut Arm, token: Token<'_>) -> Result<(), RangeError> {
    let operand = Token { text: &token.text[1..], start: token.start + 1 };
    let (v, count, _) = partial_operand(operand)?;
    if count == 0 {
        return Err(syntax(token.start));
    }
    add_bound(arm, v.clone(), Op::Ge, !v.pre.is_empty());
    let upper = if count == 1 {
        upper_version(increment(v.major, operand.start)?, 0, 0)
    } else {
        upper_version(v.major, increment(v.minor, operand.start)?, 0)
    };
    add_bound(arm, upper, Op::Lt, false);
    Ok(())
}

fn parse_comparator(arm: &mut Arm, token: Token<'_>) -> Result<(), RangeError> {
    let bytes = token.text.as_bytes();
    let (op, skip) = match bytes.first().copied() {
        Some(b'=') => (Op::Eq, 1),
        Some(b'<') if bytes.get(1) == Some(&b'=') => (Op::Le, 2),
        Some(b'<') => (Op::Lt, 1),
        Some(b'>') if bytes.get(1) == Some(&b'=') => (Op::Ge, 2),
        Some(b'>') => (Op::Gt, 1),
        _ => return Err(syntax(token.start)),
    };
    if skip >= token.text.len() || !token.text.as_bytes()[skip].is_ascii_digit() {
        return Err(syntax(token.start));
    }
    let operand = Token { text: &token.text[skip..], start: token.start + skip };
    let version = full_operand(operand)?;
    add_bound(arm, version, op, true);
    Ok(())
}

fn parse_hyphen(arm_tokens: &[Token<'_>]) -> Result<Arm, RangeError> {
    let mut arm = Arm { bounds: Vec::new(), anchors: Vec::new() };
    let dash = arm_tokens.iter().find(|token| token.text == "-").copied();
    if let Some(dash_token) = dash {
        if arm_tokens.len() != 3 || arm_tokens[1].text != "-" {
            return Err(syntax(dash_token.start));
        }
        let (lower, lower_count, lower_full) = partial_operand(arm_tokens[0])?;
        let (upper, upper_count, upper_full) = partial_operand(arm_tokens[2])?;
        if lower_count == 0 {
            return Err(syntax(arm_tokens[0].start));
        }
        if upper_count == 0 {
            return Err(syntax(arm_tokens[2].start));
        }
        add_bound(&mut arm, lower.clone(), Op::Ge, lower_full && !lower.pre.is_empty());
        if upper_full {
            add_bound(&mut arm, upper.clone(), Op::Le, !upper.pre.is_empty());
        } else if upper_count == 1 {
            add_bound(&mut arm, upper_version(increment(upper.major, arm_tokens[2].start)?, 0, 0), Op::Lt, false);
        } else if upper_count == 2 {
            add_bound(&mut arm, upper_version(upper.major, increment(upper.minor, arm_tokens[2].start)?, 0), Op::Lt, false);
        } else {
            add_bound(&mut arm, upper, Op::Le, false);
        }
        Ok(arm)
    } else {
        Err(syntax(arm_tokens[0].start))
    }
}

fn parse_arm(tokens: &[Token<'_>]) -> Result<Arm, RangeError> {
    if tokens.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }
    if tokens.iter().any(|token| token.text == "-") {
        return parse_hyphen(tokens);
    }
    let mut arm = Arm { bounds: Vec::new(), anchors: Vec::new() };
    for token in tokens {
        let first = token.text.as_bytes().first().copied();
        match first {
            Some(b'^') => expand_caret(&mut arm, *token)?,
            Some(b'~') => expand_tilde(&mut arm, *token)?,
            Some(b'<' | b'>' | b'=') => parse_comparator(&mut arm, *token)?,
            Some(b'0'..=b'9' | b'*' | b'x' | b'X') => expand_bare(&mut arm, *token)?,
            _ => return Err(syntax(token.start)),
        }
    }
    Ok(arm)
}

fn disjunction_positions(tokens: &[Token<'_>]) -> Vec<usize> {
    tokens.iter().enumerate().filter_map(|(index, token)| {
        if token.text == "||" { Some(index) } else { None }
    }).collect()
}

fn parse_range_inner(s: &str) -> Result<Range, RangeError> {
    let tokens = tokenize(s);
    if tokens.is_empty() {
        return Err(RangeError::Empty { at: 0 });
    }
    let mut arms = Vec::new();
    let mut start = 0;
    for (index, token) in tokens.iter().enumerate() {
        if token.text == "||" {
            if start == index {
                return Err(RangeError::Empty { at: token.start });
            }
            arms.push(parse_arm(&tokens[start..index])?);
            start = index + 1;
        }
    }
    if start == tokens.len() {
        return Err(RangeError::Empty { at: s.len() });
    }
    arms.push(parse_arm(&tokens[start..])?);
    Ok(Range { arms })
}

/// Parses a range expression with conjunctions, disjunctions, and expansions.
pub fn parse_range(s: &str) -> Result<Range, RangeError> {
    parse_range_inner(s)
}

fn compare_bound(v: &Version, bound: &Bound) -> bool {
    let ordering = compare_versions(v, &bound.version);
    match bound.op {
        Op::Eq => ordering == Ordering::Equal,
        Op::Lt => ordering == Ordering::Less,
        Op::Le => ordering != Ordering::Greater,
        Op::Gt => ordering == Ordering::Greater,
        Op::Ge => ordering != Ordering::Less,
    }
}

fn arm_matches(v: &Version, arm: &Arm) -> bool {
    if !v.pre.is_empty() && !arm.anchors.iter().any(|tuple| *tuple == core_tuple(v)) {
        return false;
    }
    arm.bounds.iter().all(|bound| compare_bound(v, bound))
}

/// Returns whether a version matches a parsed range.
pub fn matches(v: &Version, r: &Range) -> bool {
    r.arms.iter().any(|arm| arm_matches(v, arm))
}

fn extrema(candidates: &[String], r: &Range, find_max: bool) -> Result<Option<String>, CandidateError> {
    let mut parsed = Vec::with_capacity(candidates.len());
    for (index, candidate) in candidates.iter().enumerate() {
        match parse_version(candidate) {
            Ok(version) => parsed.push(version),
            Err(error) => return Err(CandidateError { index, error }),
        }
    }
    let mut best_index: Option<usize> = None;
    for (index, version) in parsed.iter().enumerate() {
        if !matches(version, r) {
            continue;
        }
        match best_index {
            None => best_index = Some(index),
            Some(current) => {
                let ordering = compare_versions(version, &parsed[current]);
                if (find_max && ordering == Ordering::Greater)
                    || (!find_max && ordering == Ordering::Less)
                {
                    best_index = Some(index);
                }
            }
        }
    }
    Ok(best_index.map(|index| candidates[index].clone()))
}

/// Returns the greatest-precedence matching original candidate string.
pub fn max_satisfying(candidates: &[String], r: &Range) -> Result<Option<String>, CandidateError> {
    extrema(candidates, r, true)
}

/// Returns the least-precedence matching original candidate string.
pub fn min_satisfying(candidates: &[String], r: &Range) -> Result<Option<String>, CandidateError> {
    extrema(candidates, r, false)
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn pv(s: &str) -> Version {
        parse_version(s).unwrap()
    }

    fn pr(s: &str) -> Range {
        parse_range(s).unwrap()
    }

    #[test]
    fn parses_and_compares_versions() {
        let version = pv("2.1.3-rc.1+build.7");
        assert_eq!((version.major, version.minor, version.patch), (2, 1, 3));
        assert_eq!(version.pre, vec!["rc", "1"]);
        assert_eq!(version.build, vec!["build", "7"]);
        assert_eq!(compare_versions(&pv("2.1.3-alpha"), &pv("2.1.3-beta")), Ordering::Less);
    }

    #[test]
    fn rejects_invalid_versions() {
        assert_eq!(parse_version(""), Err(VersionError::Empty));
        assert_eq!(parse_version("01.2.3"), Err(VersionError::LeadingZero { at: 0 }));
        assert!(matches!(parse_version("1.2"), Err(VersionError::MissingCore { at: 3 })));
    }

    #[test]
    fn ranges_and_disjunction_work_end_to_end() {
        let range = pr("^2.1.0 <3.0.0 || 4.0.0");
        assert!(matches(&pv("2.5.1"), &range));
        assert!(matches(&pv("4.0.0"), &range));
        assert!(!matches(&pv("3.0.0"), &range));
    }

    #[test]
    fn extrema_choose_precedence() {
        let candidates = vec!["2.2.0".to_string(), "2.8.0".to_string(), "3.0.0".to_string()];
        let range = pr("2.x");
        assert_eq!(max_satisfying(&candidates, &range), Ok(Some("2.8.0".to_string())));
        assert_eq!(min_satisfying(&candidates, &range), Ok(Some("2.2.0".to_string())));
    }
}