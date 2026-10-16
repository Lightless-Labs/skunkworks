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

    let lower = matches!(case, RomanCase::Lower);
    if value == 0 {
        return Ok(if lower { "n" } else { "N" }.to_owned());
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
            for byte in token.bytes() {
                result.push(if lower {
                    (byte as char).to_ascii_lowercase()
                } else {
                    byte as char
                });
            }
        }
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

    let mut case = None;
    for (index, &byte) in bytes.iter().enumerate() {
        let current = if byte.is_ascii_uppercase() {
            Some(false)
        } else {
            Some(true)
        };
        if let Some(first_is_lower) = case {
            if current != Some(first_is_lower) {
                return Err(RomanError::MixedCase { index });
            }
        } else {
            case = current;
        }
    }

    if bytes == b"N" || bytes == b"n" {
        return Ok(0);
    }

    if let Some(index) = bytes.iter().position(|&b| b == b'N' || b == b'n') {
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

    let mut value = 0u32;
    let mut index = 0;
    while index < bytes.len() {
        let current = symbol_value(bytes[index]);
        if index + 1 < bytes.len() {
            let next = symbol_value(bytes[index + 1]);
            if next > current {
                value = value.saturating_add(next - current).min(4000);
                index += 2;
                continue;
            }
        }
        value = value.saturating_add(current).min(4000);
        index += 1;
    }

    if value > 3999 {
        return Err(RomanError::Range);
    }

    let expected = roman_encode(value, if case == Some(true) {
        RomanCase::Lower
    } else {
        RomanCase::Upper
    })?;

    if input == expected {
        return Ok(value);
    }

    let expected_bytes = expected.as_bytes();
    let differing_index = bytes
        .iter()
        .zip(expected_bytes.iter())
        .position(|(actual, expected)| actual != expected)
        .unwrap_or_else(|| bytes.len().min(expected_bytes.len()));

    Err(RomanError::NonCanonical {
        index: differing_index,
    })
}