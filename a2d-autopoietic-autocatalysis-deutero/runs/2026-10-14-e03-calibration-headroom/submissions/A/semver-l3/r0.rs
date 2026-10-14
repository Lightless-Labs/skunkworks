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

fn parse_core(core: &str) -> Result<(u64, u64, u64), ParseError> {
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
        if let Some(offset) = component.bytes().position(|b| !b.is_ascii_digit()) {
            return Err(ParseError::InvalidDigit { index, offset });
        }
        if component.len() > 1 && component.starts_with('0') {
            return Err(ParseError::LeadingZero { index });
        }
        values[index] = component
            .parse::<u64>()
            .map_err(|_| ParseError::Overflow { index })?;
    }

    Ok((values[0], values[1], values[2]))
}

fn parse_identifiers(
    text: &str,
    section: Section,
) -> Result<Vec<String>, ParseError> {
    let identifiers: Vec<&str> = text.split('.').collect();
    let mut result = Vec::with_capacity(identifiers.len());

    for (index, identifier) in identifiers.iter().enumerate() {
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
    let (core, prerelease_text) = match before_build.split_once('-') {
        Some((left, right)) => (left, Some(right)),
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

    Ok(Version {
        major,
        minor,
        patch,
        prerelease,
        build,
    })
}

fn is_numeric_identifier(identifier: &str) -> bool {
    !identifier.is_empty() && identifier.bytes().all(|b| b.is_ascii_digit())
}

fn compare_numeric_identifiers(a: &str, b: &str) -> Ordering {
    let a_trimmed = a.trim_start_matches('0');
    let b_trimmed = b.trim_start_matches('0');
    let a_value = if a_trimmed.is_empty() { "0" } else { a_trimmed };
    let b_value = if b_trimmed.is_empty() { "0" } else { b_trimmed };

    a_value
        .len()
        .cmp(&b_value.len())
        .then_with(|| a_value.as_bytes().cmp(b_value.as_bytes()))
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
    EmptyAlternative { alternative: usize },
    InvalidVersion {
        alternative: usize,
        index: usize,
        error: ParseError,
    },
}

fn is_range_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

fn parse_comparator_token(
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
    if input.bytes().all(is_range_whitespace) {
        return Ok(Range {
            alternatives: vec![Vec::new()],
        });
    }

    let mut alternatives = Vec::new();
    for (alternative_index, alternative_text) in input.split("||").enumerate() {
        if alternative_text.bytes().all(is_range_whitespace) {
            return Err(RangeError::EmptyAlternative {
                alternative: alternative_index,
            });
        }

        let bytes = alternative_text.as_bytes();
        let mut comparators = Vec::new();
        let mut cursor = 0;
        let mut token_index = 0;

        while cursor < bytes.len() {
            while cursor < bytes.len() && is_range_whitespace(bytes[cursor]) {
                cursor += 1;
            }
            if cursor == bytes.len() {
                break;
            }

            let start = cursor;
            while cursor < bytes.len() && !is_range_whitespace(bytes[cursor]) {
                cursor += 1;
            }

            let token = &alternative_text[start..cursor];
            comparators.push(parse_comparator_token(
                token,
                alternative_index,
                token_index,
            )?);
            token_index += 1;
        }

        alternatives.push(comparators);
    }

    Ok(Range { alternatives })
}

fn comparator_matches(comparator: &Comparator, version: &Version) -> bool {
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
            .all(|comparator| comparator_matches(comparator, version))
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