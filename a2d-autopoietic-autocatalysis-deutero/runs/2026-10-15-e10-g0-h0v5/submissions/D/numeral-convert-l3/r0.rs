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
    match (system, byte) {
        (SignedSystem::BalancedTernary, b'T') => Some(-1),
        (SignedSystem::BalancedTernary, b'0') => Some(0),
        (SignedSystem::BalancedTernary, b'1') => Some(1),
        (SignedSystem::Negabinary, b'0') => Some(0),
        (SignedSystem::Negabinary, b'1') => Some(1),
        _ => None,
    }
}

fn checksum_digit(payload: &[u8]) -> u8 {
    let sum = payload.iter().enumerate().fold(0_u64, |sum, (index, &byte)| {
        let code = match byte {
            b'T' => 2_u64,
            b'0' => 0,
            b'1' => 1,
            _ => 0,
        };
        (sum + ((index as u64 + 1) * code)) % 7
    });
    sum as u8
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
    let mut reversed = Vec::new();

    if current == 0 {
        reversed.push(b'0');
    } else {
        while current != 0 {
            match system {
                SignedSystem::BalancedTernary => {
                    let mut remainder = current.rem_euclid(3);
                    if remainder == 2 {
                        remainder = -1;
                    }
                    let digit = remainder;
                    reversed.push(match digit {
                        -1 => b'T',
                        0 => b'0',
                        1 => b'1',
                        _ => unreachable!(),
                    });
                    current = (current - digit) / 3;
                }
                SignedSystem::Negabinary => {
                    let digit = current.rem_euclid(2);
                    reversed.push(if digit == 0 { b'0' } else { b'1' });
                    current = (current - digit) / -2;
                }
            }
        }
    }

    reversed.reverse();
    let mut output = String::from_utf8(reversed).expect("digits are ASCII");
    if checksum {
        let sum = checksum_digit(output.as_bytes());
        output.push(':');
        output.push(char::from(b'0' + sum));
    }
    Ok(output)
}

pub fn signed_decode(input: &str, system: SignedSystem) -> Result<i128, SignedError> {
    if input.is_empty() {
        return Err(SignedError::Empty);
    }

    let bytes = input.as_bytes();
    let colon = bytes.iter().position(|&byte| byte == b':');
    let payload_end = colon.unwrap_or(bytes.len());

    if payload_end == 0 {
        return Err(SignedError::Empty);
    }

    let payload = &bytes[..payload_end];

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
    let mut accumulator: i128 = 0;

    for (index, &byte) in payload.iter().enumerate() {
        let digit = digit_value(byte, &system).expect("payload was validated");
        accumulator = accumulator * base + digit;
        if !(-LIMIT..=LIMIT).contains(&accumulator) {
            return Err(SignedError::Range { index });
        }
    }

    if let Some(found) = supplied_checksum {
        let expected = checksum_digit(payload);
        if found != expected {
            return Err(SignedError::ChecksumMismatch { expected, found });
        }
    }

    Ok(accumulator)
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