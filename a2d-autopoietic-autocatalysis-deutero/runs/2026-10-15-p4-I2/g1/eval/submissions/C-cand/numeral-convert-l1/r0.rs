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

fn roman_value(byte: u8) -> Option<u32> {
    match byte {
        b'I' | b'i' => Some(1),
        b'V' | b'v' => Some(5),
        b'X' | b'x' => Some(10),
        b'L' | b'l' => Some(50),
        b'C' | b'c' => Some(100),
        b'D' | b'd' => Some(500),
        b'M' | b'm' => Some(1000),
        _ => None,
    }
}

fn roman_encode_case(value: u32, lowercase: bool) -> String {
    if value == 0 {
        return if lowercase { "n" } else { "N" }.to_owned();
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
            if lowercase {
                result.push_str(&token.to_ascii_lowercase());
            } else {
                result.push_str(token);
            }
        }
    }
    result
}

pub fn roman_encode(value: u32, case: RomanCase) -> Result<String, RomanError> {
    if value > 3999 {
        return Err(RomanError::Range);
    }
    Ok(roman_encode_case(value, matches!(case, RomanCase::Lower)))
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

    let lowercase = bytes
        .iter()
        .find(|&&byte| byte.is_ascii_alphabetic())
        .map(|&byte| byte.is_ascii_lowercase())
        .unwrap_or(false);

    for (index, &byte) in bytes.iter().enumerate() {
        if byte.is_ascii_alphabetic() && byte.is_ascii_lowercase() != lowercase {
            return Err(RomanError::MixedCase { index });
        }
    }

    if bytes == b"N" || bytes == b"n" {
        return Ok(0);
    }

    if let Some(index) = bytes.iter().position(|&byte| byte == b'N' || byte == b'n') {
        let _ = index;
        return Err(RomanError::NonCanonical { index: 0 });
    }

    let mut value = 0u32;
    let mut index = 0usize;
    while index < bytes.len() {
        let current = roman_value(bytes[index]).expect("lexical scan validated Roman symbols");
        if index + 1 < bytes.len() {
            let next = roman_value(bytes[index + 1]).expect("lexical scan validated Roman symbols");
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

    let canonical = roman_encode_case(value, lowercase);
    let canonical_bytes = canonical.as_bytes();
    let mismatch = bytes
        .iter()
        .zip(canonical_bytes.iter())
        .position(|(left, right)| left != right)
        .unwrap_or_else(|| bytes.len().min(canonical_bytes.len()));

    if bytes != canonical_bytes {
        return Err(RomanError::NonCanonical { index: mismatch });
    }

    Ok(value)
}

fn main() {}