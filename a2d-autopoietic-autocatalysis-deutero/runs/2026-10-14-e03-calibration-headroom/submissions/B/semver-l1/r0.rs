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

pub fn parse_version(input: &str) -> Result<Version, ParseError> {
    if input.is_empty() {
        return Err(ParseError::EmptyInput);
    }

    let components: Vec<&str> = input.split('.').collect();
    if components.len() != 3 {
        return Err(ParseError::ComponentCount {
            found: components.len(),
        });
    }

    let mut values = [0_u64; 3];

    for (index, component) in components.iter().enumerate() {
        if component.is_empty() {
            return Err(ParseError::EmptyComponent { index });
        }

        if let Some((offset, _)) = component
            .char_indices()
            .find(|(_, character)| !character.is_ascii_digit())
        {
            return Err(ParseError::InvalidDigit { index, offset });
        }

        if component.len() > 1 && component.starts_with('0') {
            return Err(ParseError::LeadingZero { index });
        }

        let mut value = 0_u64;
        for byte in component.bytes() {
            value = value
                .checked_mul(10)
                .and_then(|current| current.checked_add(u64::from(byte - b'0')))
                .ok_or(ParseError::Overflow { index })?;
        }
        values[index] = value;
    }

    Ok(Version {
        major: values[0],
        minor: values[1],
        patch: values[2],
    })
}