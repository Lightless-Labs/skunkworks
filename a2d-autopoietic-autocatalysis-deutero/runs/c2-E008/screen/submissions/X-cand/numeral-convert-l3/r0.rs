#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignedSystem {
    BalancedTernary,
    Negabinary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignedError {
    Empty,
    InvalidByte { index: usize },
    InvalidChecksum { index: usize },
    LeadingZero,
    Range { index: usize },
    ChecksumMismatch { expected: u8, found: u8 },
    OutOfRange,
}

const LIMIT: i128 = (1_i128 << 120) - 1;

fn checksum_digit(payload: &[u8]) -> u8 {
    let mut sum = 0_u8;
    for (index, &byte) in payload.iter().enumerate() {
        let code = match byte {
            b'T' => 2_u8,
            b'1' => 1_u8,
            _ => 0_u8,
        };
        let position_mod_7 = ((index % 7) as u8 + 1) % 7;
        sum = (sum + position_mod_7 * code) % 7;
    }
    sum
}

pub fn signed_encode(
    value: i128,
    system: SignedSystem,
    checksum: bool,
) -> Result<String, SignedError> {
    if !(-LIMIT..=LIMIT).contains(&value) {
        return Err(SignedError::OutOfRange);
    }

    let mut current = value;
    let mut digits = Vec::new();

    if current == 0 {
        digits.push(b'0');
    } else {
        match system {
            SignedSystem::BalancedTernary => {
                while current != 0 {
                    let remainder = current.rem_euclid(3);
                    let digit = if remainder == 2 { -1 } else { remainder };
                    digits.push(match digit {
                        -1 => b'T',
                        0 => b'0',
                        _ => b'1',
                    });
                    current = (current - digit) / 3;
                }
            }
            SignedSystem::Negabinary => {
                while current != 0 {
                    let digit = current.rem_euclid(2);
                    digits.push(if digit == 0 { b'0' } else { b'1' });
                    current = (current - digit) / -2;
                }
            }
        }
        digits.reverse();
    }

    let mut output = String::from_utf8(digits).expect("digits are ASCII");
    if checksum {
        let check = checksum_digit(output.as_bytes());
        output.push(':');
        output.push(char::from(b'0' + check));
    }
    Ok(output)
}

pub fn signed_decode(input: &str, system: SignedSystem) -> Result<i128, SignedError> {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return Err(SignedError::Empty);
    }

    let colon = bytes.iter().position(|&byte| byte == b':');
    let payload_end = colon.unwrap_or(bytes.len());
    if payload_end == 0 {
        return Err(SignedError::Empty);
    }

    for (index, &byte) in bytes[..payload_end].iter().enumerate() {
        let valid = match system {
            SignedSystem::BalancedTernary => matches!(byte, b'T' | b'0' | b'1'),
            SignedSystem::Negabinary => matches!(byte, b'0' | b'1'),
        };
        if !valid {
            return Err(SignedError::InvalidByte { index });
        }
    }

    let supplied_checksum = if let Some(colon_index) = colon {
        let suffix = &bytes[colon_index + 1..];
        if suffix.len() != 1 || !matches!(suffix[0], b'0'..=b'6') {
            return Err(SignedError::InvalidChecksum { index: colon_index });
        }
        Some(suffix[0] - b'0')
    } else {
        None
    };

    let payload = &bytes[..payload_end];
    if payload.len() > 1 && payload[0] == b'0' {
        return Err(SignedError::LeadingZero);
    }

    let base = match system {
        SignedSystem::BalancedTernary => 3_i128,
        SignedSystem::Negabinary => -2_i128,
    };

    let mut value = 0_i128;
    for (index, &byte) in payload.iter().enumerate() {
        let digit = match byte {
            b'T' => -1_i128,
            b'1' => 1_i128,
            _ => 0_i128,
        };

        value = value
            .checked_mul(base)
            .and_then(|accumulator| accumulator.checked_add(digit))
            .ok_or(SignedError::Range { index })?;

        if !(-LIMIT..=LIMIT).contains(&value) {
            return Err(SignedError::Range { index });
        }
    }

    if let Some(found) = supplied_checksum {
        let expected = checksum_digit(payload);
        if found != expected {
            return Err(SignedError::ChecksumMismatch { expected, found });
        }
    }

    Ok(value)
}

pub fn signed_recode(
    input: &str,
    from: SignedSystem,
    to: SignedSystem,
    checksum: bool,
) -> Result<String, SignedError> {
    let value = signed_decode(input, from)?;
    signed_encode(value, to, checksum)
}