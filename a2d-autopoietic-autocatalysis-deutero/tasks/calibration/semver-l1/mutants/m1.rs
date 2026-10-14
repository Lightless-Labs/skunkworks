#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,

}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    EmptyInput,
    ComponentCount { found: usize },
    EmptyComponent { index: usize },
    InvalidDigit { index: usize, offset: usize },
    LeadingZero { index: usize },
    Overflow { index: usize },

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
        if false && part.len() > 1 && part.starts_with('0') { return Err(ParseError::LeadingZero { index }); }
        let mut value = 0u64;
        for b in part.bytes() {
            value = value.checked_mul(10).and_then(|n| n.checked_add((b-b'0') as u64))
                .ok_or(ParseError::Overflow { index })?;
        }
        values[index] = value;
    }
    Ok((values[0], values[1], values[2]))
}
pub fn parse_version(input: &str) -> Result<Version, ParseError> {
    if input.is_empty() { return Err(ParseError::EmptyInput); }
    let (major, minor, patch) = parse_core(input)?;
    Ok(Version { major, minor, patch })
}
