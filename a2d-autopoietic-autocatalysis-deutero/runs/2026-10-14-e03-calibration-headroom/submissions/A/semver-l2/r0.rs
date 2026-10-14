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
    InvalidIdentifier { section: Section, index: usize, offset: usize },
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

        for (offset, byte) in component.bytes().enumerate() {
            if !byte.is_ascii_digit() {
                return Err(ParseError::InvalidDigit { index, offset });
            }
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

fn parse_identifiers(text: &str, section: Section) -> Result<Vec<String>, ParseError> {
    let identifiers: Vec<&str> = text.split('.').collect();
    let mut result = Vec::with_capacity(identifiers.len());

    for (index, identifier) in identifiers.iter().enumerate() {
        if identifier.is_empty() {
            return Err(ParseError::EmptyIdentifier { section, index });
        }

        for (offset, byte) in identifier.bytes().enumerate() {
            if !byte.is_ascii_alphanumeric() && byte != b'-' {
                return Err(ParseError::InvalidIdentifier {
                    section,
                    index,
                    offset,
                });
            }
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
        Some((before, after)) => (before, Some(after)),
        None => (input, None),
    };

    let (core_text, prerelease_text) = match before_build.split_once('-') {
        Some((core, after)) => (core, Some(after)),
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

fn is_numeric_identifier(identifier: &str) -> bool {
    !identifier.is_empty() && identifier.bytes().all(|byte| byte.is_ascii_digit())
}

fn compare_numeric_identifiers(a: &str, b: &str) -> std::cmp::Ordering {
    let a = a.trim_start_matches('0');
    let b = b.trim_start_matches('0');

    match a.len().cmp(&b.len()) {
        std::cmp::Ordering::Equal => a.as_bytes().cmp(b.as_bytes()),
        other => other,
    }
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