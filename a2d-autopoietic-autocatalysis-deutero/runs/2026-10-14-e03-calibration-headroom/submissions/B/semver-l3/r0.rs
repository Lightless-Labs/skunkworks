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

fn parse_core(core: &str) -> Result<(u64, u64, u64), ParseError> {
    if core.is_empty() {
        return Err(ParseError::EmptyInput);
    }

    let components: Vec<&str> = core.split('.').collect();
    if components.len() != 3 {
        return Err(ParseError::ComponentCount {
            found: components.len(),
        });
    }

    let mut values = [0u64; 3];
    for (index, component) in components.iter().enumerate() {
        if component.is_empty() {
            return Err(ParseError::EmptyComponent { index });
        }

        if let Some((offset, _)) = component
            .bytes()
            .enumerate()
            .find(|(_, byte)| !byte.is_ascii_digit())
        {
            return Err(ParseError::InvalidDigit { index, offset });
        }

        if component.len() > 1 && component.starts_with('0') {
            return Err(ParseError::LeadingZero { index });
        }

        let mut value = 0u64;
        for byte in component.bytes() {
            value = value
                .checked_mul(10)
                .and_then(|v| v.checked_add((byte - b'0') as u64))
                .ok_or(ParseError::Overflow { index })?;
        }
        values[index] = value;
    }

    Ok((values[0], values[1], values[2]))
}

fn parse_identifiers(text: &str, section: Section) -> Result<Vec<String>, ParseError> {
    let identifiers: Vec<&str> = text.split('.').collect();
    let mut result = Vec::with_capacity(identifiers.len());

    for (index, identifier) in identifiers.iter().enumerate() {
        if identifier.is_empty() {
            return Err(ParseError::EmptyIdentifier { section, index });
        }

        if let Some((offset, _)) = identifier.bytes().enumerate().find(|(_, byte)| {
            !(byte.is_ascii_alphanumeric() || *byte == b'-')
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
            && identifier.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(ParseError::NumericLeadingZero { index });
        }

        result.push((*identifier).to_owned());
    }

    Ok(result)
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

    let (major, minor, patch) = parse_core(core_text)?;
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

fn compare_numeric_identifiers(a: &str, b: &str) -> std::cmp::Ordering {
    let a = a.trim_start_matches('0');
    let b = b.trim_start_matches('0');
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

fn is_numeric_identifier(identifier: &str) -> bool {
    !identifier.is_empty() && identifier.bytes().all(|byte| byte.is_ascii_digit())
}

pub fn precedence(a: &Version, b: &Version) -> std::cmp::Ordering {
    use std::cmp::Ordering;

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
        let left_numeric = is_numeric_identifier(left);
        let right_numeric = is_numeric_identifier(right);
        let order = match (left_numeric, right_numeric) {
            (true, true) => compare_numeric_identifiers(left, right),
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
}

fn is_ascii_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

fn parse_comparator(
    token: &str,
    alternative: usize,
    index: usize,
) -> Result<Comparator, RangeError> {
    let (op, version_text) = if let Some(rest) = token.strip_prefix(">=") {
        (Op::Ge, rest)
    } else if let Some(rest) = token.strip_prefix("<=") {
        (Op::Le, rest)
    } else if let Some(rest) = token.strip_prefix('>') {
        (Op::Gt, rest)
    } else if let Some(rest) = token.strip_prefix('<') {
        (Op::Lt, rest)
    } else if let Some(rest) = token.strip_prefix('=') {
        (Op::Eq, rest)
    } else {
        (Op::Eq, token)
    };

    let version = parse_version(version_text).map_err(|error| RangeError::InvalidVersion {
        alternative,
        index,
        error,
    })?;
    let allows_prerelease = !version.prerelease.is_empty();

    Ok(Comparator {
        op,
        version,
        allows_prerelease,
    })
}

pub fn parse_range(input: &str) -> Result<Range, RangeError> {
    let bytes = input.as_bytes();
    let mut raw_alternatives: Vec<&str> = Vec::new();
    let mut start = 0;
    let mut position = 0;

    while position + 1 < bytes.len() {
        if bytes[position] == b'|' && bytes[position + 1] == b'|' {
            raw_alternatives.push(&input[start..position]);
            position += 2;
            start = position;
        } else {
            position += 1;
        }
    }
    raw_alternatives.push(&input[start..]);

    let is_all_whitespace = input.bytes().all(is_ascii_whitespace);
    if is_all_whitespace {
        return Ok(Range {
            alternatives: vec![Vec::new()],
        });
    }

    let mut alternatives = Vec::with_capacity(raw_alternatives.len());

    for (alternative_index, alternative_text) in raw_alternatives.iter().enumerate() {
        let mut comparators = Vec::new();
        let mut token_index = 0;
        let mut cursor = 0;
        let text = *alternative_text;
        let text_bytes = text.as_bytes();

        while cursor < text_bytes.len() {
            while cursor < text_bytes.len() && is_ascii_whitespace(text_bytes[cursor]) {
                cursor += 1;
            }
            if cursor == text_bytes.len() {
                break;
            }

            let token_start = cursor;
            while cursor < text_bytes.len() && !is_ascii_whitespace(text_bytes[cursor]) {
                cursor += 1;
            }
            let token = &text[token_start..cursor];
            comparators.push(parse_comparator(token, alternative_index, token_index)?);
            token_index += 1;
        }

        if comparators.is_empty() {
            return Err(RangeError::EmptyAlternative {
                alternative: alternative_index,
            });
        }
        alternatives.push(comparators);
    }

    Ok(Range { alternatives })
}

fn relation_holds(comparator: &Comparator, version: &Version) -> bool {
    use std::cmp::Ordering;

    match (comparator.op, precedence(version, &comparator.version)) {
        (Op::Lt, Ordering::Less) => true,
        (Op::Le, Ordering::Less | Ordering::Equal) => true,
        (Op::Eq, Ordering::Equal) => true,
        (Op::Ge, Ordering::Greater | Ordering::Equal) => true,
        (Op::Gt, Ordering::Greater) => true,
        _ => false,
    }
}

pub fn satisfies(version: &Version, range: &Range) -> bool {
    range.alternatives.iter().any(|alternative| {
        if !alternative
            .iter()
            .all(|comparator| relation_holds(comparator, version))
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