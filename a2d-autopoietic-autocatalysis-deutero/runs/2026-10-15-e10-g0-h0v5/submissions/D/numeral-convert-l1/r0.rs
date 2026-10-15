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
    let mut output = String::new();

    for &(amount, token) in TOKENS {
        while remaining >= amount {
            output.push_str(token);
            remaining -= amount;
        }
    }

    if case == RomanCase::Lower {
        output.make_ascii_lowercase();
    }

    Ok(output)
}

pub fn roman_decode(input: &str) -> Result<u32, RomanError> {
    if input.is_empty() {
        return Err(RomanError::Empty);
    }

    for (index, byte) in input.bytes().enumerate() {
        if !matches!(
            byte,
            b'I' | b'V' | b'X' | b'L' | b'C' | b'D' | b'M' | b'N'
                | b'i' | b'v' | b'x' | b'l' | b'c' | b'd' | b'm' | b'n'
        ) {
            return Err(RomanError::InvalidByte { index });
        }
    }

    let mut case = None;
    for (index, byte) in input.bytes().enumerate() {
        let byte_case = if byte.is_ascii_uppercase() {
            Some(true)
        } else {
            Some(false)
        };

        match case {
            None => case = byte_case,
            Some(first_is_upper) if Some(first_is_upper) != byte_case => {
                return Err(RomanError::MixedCase { index });
            }
            _ => {}
        }
    }

    if input == "N" || input == "n" {
        return Ok(0);
    }

    if input.bytes().any(|byte| byte == b'N' || byte == b'n') {
        return Err(RomanError::NonCanonical { index: 0 });
    }

    let bytes = input.as_bytes();
    let mut index = 0;
    let mut value = 0u32;

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

    let roman_case = if case == Some(true) {
        RomanCase::Upper
    } else {
        RomanCase::Lower
    };
    let canonical = roman_encode(value, roman_case).expect("value is within range");

    if input != canonical {
        let differing_index = input
            .bytes()
            .zip(canonical.bytes())
            .position(|(actual, expected)| actual != expected)
            .unwrap_or_else(|| input.len().min(canonical.len()));
        return Err(RomanError::NonCanonical {
            index: differing_index,
        });
    }

    Ok(value)
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