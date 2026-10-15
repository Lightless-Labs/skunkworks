#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RomanCase {
    Upper,
    Lower,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RomanError {
    Empty,
    InvalidByte { index: usize },
    MixedCase { index: usize },
    NonCanonical { index: usize },
    Range,
}

pub fn roman_encode(value: u32, case: RomanCase) -> Result<String, RomanError> {
    if value > 3999 {
        return Err(RomanError::Range);
    }

    if value == 0 {
        return Ok(match case {
            RomanCase::Upper => "N".to_owned(),
            RomanCase::Lower => "n".to_owned(),
        });
    }

    const TOKENS: &[(u32, &str)] = &[
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];

    let mut remaining = value;
    let mut result = String::new();

    for &(amount, token) in TOKENS {
        while remaining >= amount {
            remaining -= amount;
            result.push_str(token);
        }
    }

    if case == RomanCase::Lower {
        result.make_ascii_lowercase();
    }

    Ok(result)
}

pub fn roman_decode(input: &str) -> Result<u32, RomanError> {
    if input.is_empty() {
        return Err(RomanError::Empty);
    }

    let bytes = input.as_bytes();

    for (index, &byte) in bytes.iter().enumerate() {
        if !matches!(
            byte,
            b'I' | b'V' | b'X' | b'L' | b'C' | b'D' | b'M' | b'N'
                | b'i' | b'v' | b'x' | b'l' | b'c' | b'd' | b'm' | b'n'
        ) {
            return Err(RomanError::InvalidByte { index });
        }
    }

    let first_case = bytes[0].is_ascii_lowercase();
    for (index, &byte) in bytes.iter().enumerate() {
        if byte.is_ascii_alphabetic() && byte.is_ascii_lowercase() != first_case {
            return Err(RomanError::MixedCase { index });
        }
    }

    if bytes.len() == 1 && matches!(bytes[0], b'N' | b'n') {
        return Ok(0);
    }

    if let Some(index) = bytes.iter().position(|&byte| matches!(byte, b'N' | b'n')) {
        let _ = index;
        return Err(RomanError::NonCanonical { index: 0 });
    }

    fn symbol_value(byte: u8) -> u32 {
        match byte.to_ascii_uppercase() {
            b'I' => 1,
            b'V' => 5,
            b'X' => 10,
            b'L' => 50,
            b'C' => 100,
            b'D' => 500,
            b'M' => 1000,
            _ => 0,
        }
    }

    let mut provisional = 0u32;
    let mut index = 0;
    while index < bytes.len() {
        let current = symbol_value(bytes[index]);
        if index + 1 < bytes.len() {
            let next = symbol_value(bytes[index + 1]);
            if next > current {
                provisional = provisional.saturating_add(next - current).min(4000);
                index += 2;
                continue;
            }
        }
        provisional = provisional.saturating_add(current).min(4000);
        index += 1;
    }

    if provisional > 3999 {
        return Err(RomanError::Range);
    }

    let case = if first_case {
        RomanCase::Lower
    } else {
        RomanCase::Upper
    };
    let canonical = roman_encode(provisional, case)?;

    let differing_index = bytes
        .iter()
        .zip(canonical.as_bytes())
        .position(|(left, right)| left != right)
        .unwrap_or_else(|| bytes.len().min(canonical.len()));

    if input != canonical {
        return Err(RomanError::NonCanonical {
            index: differing_index,
        });
    }

    Ok(provisional)
}