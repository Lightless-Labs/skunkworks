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
        if section == Section::Prerelease && part.bytes().all(|b| b.is_ascii_digit()) && part.parse::<u64>().is_err() { return Err(ParseError::Overflow { index }); }
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
