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

fn digit_value(byte: u8, system: &SignedSystem) -> Option<i128> {
    match system {
        SignedSystem::BalancedTernary => match byte {
            b'T' => Some(-1),
            b'0' => Some(0),
            b'1' => Some(1),
            _ => None,
        },
        SignedSystem::Negabinary => match byte {
            b'0' => Some(0),
            b'1' => Some(1),
            _ => None,
        },
    }
}

fn checksum(payload: &[u8]) -> u8 {
    let mut sum = 0_u128;
    for (index, &byte) in payload.iter().enumerate() {
        let code = match byte {
            b'T' => 2_u128,
            b'0' => 0,
            b'1' => 1,
            _ => 0,
        };
        sum = (sum + ((index as u128 + 1) * code)) % 7;
    }
    sum as u8
}

pub fn signed_encode(
    value: i128,
    system: SignedSystem,
    include_checksum: bool,
) -> Result<String, SignedError> {
    if !(-LIMIT..=LIMIT).contains(&value) {
        return Err(SignedError::OutOfRange);
    }

    let mut digits = Vec::new();
    let mut current = value;

    if current == 0 {
        digits.push(b'0');
    } else {
        while current != 0 {
            match system {
                SignedSystem::BalancedTernary => {
                    let remainder = current.rem_euclid(3);
                    let digit = if remainder == 2 { -1 } else { remainder };
                    digits.push(match digit {
                        -1 => b'T',
                        0 => b'0',
                        _ => b'1',
                    });
                    current = (current - digit) / 3;
                }
                SignedSystem::Negabinary => {
                    let digit = current.rem_euclid(2);
                    digits.push(if digit == 0 { b'0' } else { b'1' });
                    current = (current - digit) / -2;
                }
            }
        }
        digits.reverse();
    }

    let mut result = String::from_utf8(digits).expect("digit alphabet is ASCII");
    if include_checksum {
        let check = checksum(result.as_bytes());
        result.push(':');
        result.push(char::from(b'0' + check));
    }
    Ok(result)
}

pub fn signed_decode(input: &str, system: SignedSystem) -> Result<i128, SignedError> {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return Err(SignedError::Empty);
    }

    let colon = bytes.iter().position(|&byte| byte == b':');
    let payload_end = colon.unwrap_or(bytes.len());
    let payload = &bytes[..payload_end];

    if payload.is_empty() {
        return Err(SignedError::Empty);
    }

    for (index, &byte) in payload.iter().enumerate() {
        if digit_value(byte, &system).is_none() {
            return Err(SignedError::InvalidByte { index });
        }
    }

    let supplied_checksum = if let Some(colon_index) = colon {
        let suffix = &bytes[colon_index + 1..];
        if suffix.len() != 1 || !(b'0'..=b'6').contains(&suffix[0]) {
            return Err(SignedError::InvalidChecksum { index: colon_index });
        }
        Some(suffix[0] - b'0')
    } else {
        None
    };

    if payload.len() > 1 && payload[0] == b'0' {
        return Err(SignedError::LeadingZero);
    }

    let base: i128 = match system {
        SignedSystem::BalancedTernary => 3,
        SignedSystem::Negabinary => -2,
    };
    let mut accumulator = 0_i128;

    for (index, &byte) in payload.iter().enumerate() {
        let digit = digit_value(byte, &system).expect("payload was validated");
        let next = accumulator
            .checked_mul(base)
            .and_then(|value| value.checked_add(digit));

        accumulator = match next {
            Some(value) if (-LIMIT..=LIMIT).contains(&value) => value,
            _ => return Err(SignedError::Range { index }),
        };
    }

    if let Some(found) = supplied_checksum {
        let expected = checksum(payload);
        if expected != found {
            return Err(SignedError::ChecksumMismatch { expected, found });
        }
    }

    Ok(accumulator)
}

pub fn signed_recode(
    input: &str,
    from: SignedSystem,
    to: SignedSystem,
    include_checksum: bool,
) -> Result<String, SignedError> {
    let value = signed_decode(input, from)?;
    signed_encode(value, to, include_checksum)
}