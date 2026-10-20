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

const TOKENS: [(u32, &[u8]); 13] = [
    (1000, b"M"),
    (900, b"CM"),
    (500, b"D"),
    (400, b"CD"),
    (100, b"C"),
    (90, b"XC"),
    (50, b"L"),
    (40, b"XL"),
    (10, b"X"),
    (9, b"IX"),
    (5, b"V"),
    (4, b"IV"),
    (1, b"I"),
];

pub fn roman_encode(value: u32, case: RomanCase) -> Result<String, RomanError> {
    if value > 3999 {
        return Err(RomanError::Range);
    }
    if value == 0 {
        return Ok(if case == RomanCase::Upper {
            String::from("N")
        } else {
            String::from("n")
        });
    }

    let mut n = value;
    let mut result = Vec::with_capacity(15);
    for &(amount, token) in TOKENS.iter() {
        while n >= amount {
            for &byte in token {
                result.push(if case == RomanCase::Upper {
                    byte
                } else {
                    byte.to_ascii_lowercase()
                });
            }
            n -= amount;
        }
    }

    Ok(String::from_utf8(result).expect("Roman tokens are ASCII"))
}

pub fn roman_decode(input: &str) -> Result<u32, RomanError> {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return Err(RomanError::Empty);
    }

    let mut first_non_ascii_roman = None;
    for (i, &b) in bytes.iter().enumerate() {
        let allowed = matches!(
            b,
            b'I' | b'V' | b'X' | b'L' | b'C' | b'D' | b'M' | b'N'
                | b'i' | b'v' | b'x' | b'l' | b'c' | b'd' | b'm' | b'n'
        );
        if !allowed && first_non_ascii_roman.is_none() {
            first_non_ascii_roman = Some(i);
        }
    }
    if let Some(index) = first_non_ascii_roman {
        return Err(RomanError::InvalidByte { index });
    }

    let upper = bytes[0].is_ascii_uppercase();
    if let Some(index) = bytes.iter().position(|&b| b.is_ascii_uppercase() != upper) {
        return Err(RomanError::MixedCase { index });
    }

    if bytes.len() == 1 && (bytes[0] == b'N' || bytes[0] == b'n') {
        return Ok(0);
    }
    if bytes.iter().any(|&b| b == b'N' || b == b'n') {
        return Err(RomanError::NonCanonical { index: 0 });
    }

    let mut total = 0u32;
    let mut i = 0;
    while i < bytes.len() {
        let current = symbol(bytes[i]);
        if i + 1 < bytes.len() {
            let next = symbol(bytes[i + 1]);
            if next > current {
                total = total.saturating_add(next - current).min(4000);
                i += 2;
                continue;
            }
        }
        total = total.saturating_add(current).min(4000);
        i += 1;
    }

    if total > 3999 {
        return Err(RomanError::Range);
    }

    let spelling = roman_encode(
        total,
        if upper { RomanCase::Upper } else { RomanCase::Lower },
    )
    .expect("bounded value encodes");
    let expected = spelling.as_bytes();

    for i in 0..bytes.len().min(expected.len()) {
        if bytes[i] != expected[i] {
            return Err(RomanError::NonCanonical { index: i });
        }
    }
    if bytes.len() != expected.len() {
        return Err(RomanError::NonCanonical {
            index: bytes.len().min(expected.len()),
        });
    }

    Ok(total)
}

fn symbol(byte: u8) -> u32 {
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
