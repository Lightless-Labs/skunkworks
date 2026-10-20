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

fn base(system: &SignedSystem) -> i128 {
    match system {
        SignedSystem::BalancedTernary => 3,
        SignedSystem::Negabinary => -2,
    }
}

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
    let sum = payload.iter().enumerate().fold(0_u64, |acc, (index, &byte)| {
        let code = match byte {
            b'T' => 2_u64,
            b'1' => 1_u64,
            _ => 0_u64,
        };
        (acc + (((index as u64 + 1) % 7) * code) % 7) % 7
    });
    sum as u8
}

pub fn signed_encode(
    value: i128,
    system: SignedSystem,
    checksum_enabled: bool,
) -> Result<String, SignedError> {
    if !(-LIMIT..=LIMIT).contains(&value) {
        return Err(SignedError::OutOfRange);
    }

    let mut n = value;
    let mut reversed = Vec::new();

    if n == 0 {
        reversed.push(b'0');
    } else {
        match system {
            SignedSystem::BalancedTernary => {
                while n != 0 {
                    let mut remainder = n % 3;
                    if remainder < 0 {
                        remainder += 3;
                    }
                    let digit = if remainder == 2 { -1 } else { remainder };
                    reversed.push(match digit {
                        -1 => b'T',
                        0 => b'0',
                        _ => b'1',
                    });
                    n = (n - digit) / 3;
                }
            }
            SignedSystem::Negabinary => {
                while n != 0 {
                    let digit = n.rem_euclid(2);
                    reversed.push(if digit == 0 { b'0' } else { b'1' });
                    n = (n - digit) / -2;
                }
            }
        }
    }

    reversed.reverse();
    let mut output = String::from_utf8(reversed).expect("encoding emits ASCII digits");

    if checksum_enabled {
        let sum = checksum(output.as_bytes());
        output.push(':');
        output.push(char::from(b'0' + sum));
    }

    Ok(output)
}

pub fn signed_decode(input: &str, system: SignedSystem) -> Result<i128, SignedError> {
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

    let radix = base(&system);
    let mut value = 0_i128;

    for (index, &byte) in payload.iter().enumerate() {
        let digit = digit_value(byte, &system).expect("payload was validated");
        value = value * radix + digit;
        if !(-LIMIT..=LIMIT).contains(&value) {
            return Err(SignedError::Range { index });
        }
    }

    if let Some(found) = supplied_checksum {
        let expected = checksum(payload);
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
    checksum_enabled: bool,
) -> Result<String, SignedError> {
    let value = signed_decode(input, from)?;
    signed_encode(value, to, checksum_enabled)
}