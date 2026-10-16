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

    let mut case = None;
    for (index, &byte) in bytes.iter().enumerate() {
        let is_upper = byte.is_ascii_uppercase();
        match case {
            None => case = Some(is_upper),
            Some(first_is_upper) if first_is_upper != is_upper => {
                return Err(RomanError::MixedCase { index });
            }
            _ => {}
        }
    }

    if bytes.len() == 1 && matches!(bytes[0], b'N' | b'n') {
        return Ok(0);
    }

    if let Some(index) = bytes.iter().position(|&byte| matches!(byte, b'N' | b'n')) {
        return Err(RomanError::NonCanonical { index: 0.max(index.min(0)) });
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

    let mut total = 0u32;
    let mut index = 0;
    while index < bytes.len() {
        let current = symbol_value(bytes[index]);
        if index + 1 < bytes.len() {
            let next = symbol_value(bytes[index + 1]);
            if next > current {
                total = total.saturating_add(next - current).min(4000);
                index += 2;
                if total > 3999 {
                    return Err(RomanError::Range);
                }
                continue;
            }
        }

        total = total.saturating_add(current).min(4000);
        index += 1;
        if total > 3999 {
            return Err(RomanError::Range);
        }
    }

    let roman_case = if case == Some(false) {
        RomanCase::Lower
    } else {
        RomanCase::Upper
    };
    let canonical = roman_encode(total, roman_case)?;

    let canonical_bytes = canonical.as_bytes();
    let mismatch = bytes
        .iter()
        .zip(canonical_bytes.iter())
        .position(|(left, right)| left != right)
        .unwrap_or_else(|| bytes.len().min(canonical_bytes.len()));

    if bytes == canonical_bytes {
        Ok(total)
    } else {
        Err(RomanError::NonCanonical { index: mismatch })
    }
}