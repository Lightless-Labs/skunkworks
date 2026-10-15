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

    let tokens: &[(u32, &str)] = &[
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

    let mut remainder = value;
    let mut output = String::new();

    if remainder == 0 {
        output.push(match case {
            RomanCase::Upper => 'N',
            RomanCase::Lower => 'n',
        });
        return Ok(output);
    }

    for &(amount, token) in tokens {
        while remainder >= amount {
            remainder -= amount;
            for byte in token.bytes() {
                let byte = match case {
                    RomanCase::Upper => byte,
                    RomanCase::Lower => byte.to_ascii_lowercase(),
                };
                output.push(byte as char);
            }
        }
    }

    Ok(output)
}

pub fn roman_decode(input: &str) -> Result<u32, RomanError> {
    if input.is_empty() {
        return Err(RomanError::Empty);
    }

    let mut first_upper = None;
    let mut first_lower = None;

    for (index, byte) in input.bytes().enumerate() {
        match byte {
            b'I' | b'V' | b'X' | b'L' | b'C' | b'D' | b'M' | b'N' => {
                if first_upper.is_none() {
                    first_upper = Some(index);
                }
            }
            b'i' | b'v' | b'x' | b'l' | b'c' | b'd' | b'm' | b'n' => {
                if first_lower.is_none() {
                    first_lower = Some(index);
                }
            }
            _ => return Err(RomanError::InvalidByte { index }),
        }
    }

    let case = match (first_upper, first_lower) {
        (Some(upper), Some(lower)) => {
            return Err(RomanError::MixedCase {
                index: upper.max(lower),
            });
        }
        (Some(_), None) => RomanCase::Upper,
        (None, Some(_)) => RomanCase::Lower,
        (None, None) => unreachable!(),
    };

    if input == "N" || input == "n" {
        return Ok(0);
    }

    if let Some(index) = input.bytes().position(|byte| byte == b'N' || byte == b'n') {
        let _ = index;
        return Err(RomanError::NonCanonical { index: 0 });
    }

    let values = input.bytes().map(|byte| match byte.to_ascii_uppercase() {
        b'I' => 1u32,
        b'V' => 5,
        b'X' => 10,
        b'L' => 50,
        b'C' => 100,
        b'D' => 500,
        b'M' => 1000,
        _ => unreachable!(),
    });

    let symbols: Vec<u32> = values.collect();
    let mut provisional = 0u32;
    let mut index = 0;

    while index < symbols.len() {
        if index + 1 < symbols.len() && symbols[index + 1] > symbols[index] {
            provisional = provisional.saturating_add(symbols[index + 1] - symbols[index]);
            index += 2;
        } else {
            provisional = provisional.saturating_add(symbols[index]);
            index += 1;
        }

        if provisional > 3999 {
            return Err(RomanError::Range);
        }
    }

    let canonical = roman_encode(provisional, case)?;
    if input != canonical {
        let mismatch = input
            .bytes()
            .zip(canonical.bytes())
            .position(|(left, right)| left != right)
            .unwrap_or_else(|| input.len().min(canonical.len()));
        return Err(RomanError::NonCanonical { index: mismatch });
    }

    Ok(provisional)
}

fn main() {}