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
            RomanCase::Upper => "N".to_string(),
            RomanCase::Lower => "n".to_string(),
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
            result.push_str(token);
            remaining -= amount;
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
        let current = if byte.is_ascii_uppercase() {
            RomanCase::Upper
        } else {
            RomanCase::Lower
        };
        if let Some(first_case) = &case {
            if *first_case != current {
                return Err(RomanError::MixedCase { index });
            }
        } else {
            case = Some(current);
        }
    }

    let bytes = input.as_bytes();

    if (bytes.len() == 1 && (bytes[0] == b'N' || bytes[0] == b'n')) {
        return Ok(0);
    }

    if let Some(index) = bytes.iter().position(|&byte| byte == b'N' || byte == b'n') {
        let _ = index;
        return Err(RomanError::NonCanonical { index: 0 });
    }

    let symbol_value = |byte: u8| -> u32 {
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
    };

    let mut provisional = 0u32;
    let mut i = 0;
    while i < bytes.len() {
        let current = symbol_value(bytes[i]);
        if i + 1 < bytes.len() {
            let next = symbol_value(bytes[i + 1]);
            if next > current {
                provisional = provisional.saturating_add(next - current).min(4000);
                i += 2;
                continue;
            }
        }
        provisional = provisional.saturating_add(current).min(4000);
        i += 1;
    }

    if provisional > 3999 {
        return Err(RomanError::Range);
    }

    let spelling = roman_encode(provisional, case.unwrap())?;
    let spelling_bytes = spelling.as_bytes();

    let common_len = bytes.len().min(spelling_bytes.len());
    for index in 0..common_len {
        if bytes[index] != spelling_bytes[index] {
            return Err(RomanError::NonCanonical { index });
        }
    }

    if bytes.len() != spelling_bytes.len() {
        return Err(RomanError::NonCanonical { index: common_len });
    }

    Ok(provisional)
}