#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub prerelease: Vec<String>,
    pub build: Vec<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section { Prerelease, Build }
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
fn parse_core(input: &str) -> Result<(u64, u64, u64), ParseError> {
    let parts: Vec<_> = input.split('.').collect();
    if parts.len() != 3 { return Err(ParseError::ComponentCount { found: parts.len() }); }
    let mut values = [0u64; 3];
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() { return Err(ParseError::EmptyComponent { index }); }
        if let Some(offset) = part.bytes().position(|b| !b.is_ascii_digit()) {
            return Err(ParseError::InvalidDigit { index, offset });
        }
        if part.len() > 1 && part.starts_with('0') { return Err(ParseError::LeadingZero { index }); }
        let mut value = 0u64;
        for b in part.bytes() {
            value = value.checked_mul(10).and_then(|n| n.checked_add((b-b'0') as u64))
                .ok_or(ParseError::Overflow { index })?;
        }
        values[index] = value;
    }
    Ok((values[0], values[1], values[2]))
}
fn identifiers(input: &str, section: Section) -> Result<Vec<String>, ParseError> {
    let mut result = Vec::new();
    for (index, part) in input.split('.').enumerate() {
        if part.is_empty() { return Err(ParseError::EmptyIdentifier { section, index }); }
        if let Some(offset) = part.bytes().position(|b| !(b.is_ascii_alphanumeric() || b == b'-')) {
            return Err(ParseError::InvalidIdentifier { section, index, offset });
        }
        if section == Section::Prerelease && part.bytes().all(|b| b.is_ascii_digit())
            && part.len() > 1 && part.starts_with('0') {
            return Err(ParseError::NumericLeadingZero { index });
        }
        result.push(part.to_string());
    }
    Ok(result)
}
pub fn parse_version(input: &str) -> Result<Version, ParseError> {
    if input.is_empty() { return Err(ParseError::EmptyInput); }
    let (left, build_text) = match input.split_once('+') {
        Some((a, b)) => (a, Some(b)), None => (input, None),
    };
    let (core_text, pre_text) = match left.split_once('-') {
        Some((a, b)) => (a, Some(b)), None => (left, None),
    };
    let (major, minor, patch) = parse_core(core_text)?;
    let prerelease = match pre_text { Some(s) => identifiers(s, Section::Prerelease)?, None => Vec::new() };
    let build = match build_text { Some(s) => identifiers(s, Section::Build)?, None => Vec::new() };
    Ok(Version { major, minor, patch, prerelease, build })
}
pub fn precedence(a: &Version, b: &Version) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let c = (a.major, a.minor, a.patch).cmp(&(b.major, b.minor, b.patch));
    if c != Ordering::Equal { return c; }
    match (a.prerelease.is_empty(), b.prerelease.is_empty()) {
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => (),
    }
    for (x, y) in a.prerelease.iter().zip(&b.prerelease) {
        let xn = x.bytes().all(|b| b.is_ascii_digit());
        let yn = y.bytes().all(|b| b.is_ascii_digit());
        let c = match (xn, yn) {
            (true, true) => x.len().cmp(&y.len()).then_with(|| x.cmp(y)),
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => x.cmp(y),
        };
        if c != Ordering::Equal { return c; }
    }
    a.prerelease.len().cmp(&b.prerelease.len())
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op { Lt, Le, Eq, Ge, Gt }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comparator {
    pub op: Op,
    pub version: Version,
    pub allows_prerelease: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Range { pub alternatives: Vec<Vec<Comparator>> }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RangeError {
    EmptyAlternative { alternative: usize },
    InvalidVersion { alternative: usize, index: usize, error: ParseError },

}
fn core_tuple(v: &Version) -> (u64, u64, u64) { (v.major, v.minor, v.patch) }
fn comparator(op: Op, version: Version) -> Comparator {
    let allows_prerelease = !version.prerelease.is_empty();
    Comparator { op, version, allows_prerelease }
}
fn operator(token: &str) -> Option<(Op, &str)> {
    for (prefix, op) in [(">=", Op::Ge), ("<=", Op::Le), (">", Op::Gt), ("<", Op::Lt), ("=", Op::Eq)] {
        if let Some(rest) = token.strip_prefix(prefix) { return Some((op, rest)); }
    }
    None
}
fn ascii_tokens(input: &str) -> impl Iterator<Item = &str> {
    input.split(|c| matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{b}' | '\u{c}'))
        .filter(|s| !s.is_empty())
}
pub fn satisfies(version: &Version, range: &Range) -> bool {
    use std::cmp::Ordering::{Less, Equal, Greater};
    range.alternatives.iter().any(|set| {
        if !version.prerelease.is_empty() && !set.iter().any(|c|
            c.allows_prerelease && core_tuple(&c.version) == core_tuple(version)) { return false; }
        set.iter().all(|c| {
            let order = precedence(version, &c.version);
            match c.op {
                Op::Lt => order == Less, Op::Le => order != Greater,
                Op::Eq => order == Equal, Op::Ge => order != Less, Op::Gt => order == Greater,
            }
        })
    })
}
pub fn parse_range(input: &str) -> Result<Range, RangeError> {
    if ascii_tokens(input).next().is_none() { return Ok(Range { alternatives: vec![vec![]] }); }
    let mut alternatives = Vec::new();
    for (alternative, branch) in input.split("||").enumerate() {
        let tokens: Vec<_> = ascii_tokens(branch).collect();
        if tokens.is_empty() { return Err(RangeError::EmptyAlternative { alternative }); }
        let mut set = Vec::new();
        for (index, token) in tokens.iter().enumerate() {
            let (op, text) = operator(token).unwrap_or((Op::Eq, token));
            let version = parse_version(text).map_err(|error| RangeError::InvalidVersion { alternative, index, error })?;
            set.push(comparator(op, version));
        }
        alternatives.push(set);
    }
    Ok(Range { alternatives })
}
