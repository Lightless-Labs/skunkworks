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

fn checksum(payload: &[u8]) -> u8 {
    payload
        .iter()
        .enumerate()
        .fold(0_u64, |sum, (index, &byte)| {
            let code = match byte {
                b'T' => 2_u64,
                b'0' => 0,
                b'1' => 1,
                _ => 0,
            };
            (sum + (((index as u64 + 1) % 7) * code)) % 7
        }) as u8
}

pub fn signed_encode(
    value: i128,
    system: SignedSystem,
    with_checksum: bool,
) -> Result<String, SignedError> {
    if !(-LIMIT..=LIMIT).contains(&value) {
        return Err(SignedError::OutOfRange);
    }

    let mut n = value;
    let mut digits = Vec::new();

    if n == 0 {
        digits.push(b'0');
    } else {
        while n != 0 {
            match system {
                SignedSystem::BalancedTernary => {
                    let mut remainder = n % 3;
                    n /= 3;
                    if remainder == 2 {
                        remainder = -1;
                        n += 1;
                    } else if remainder == -2 {
                        remainder = 1;
                        n -= 1;
                    }
                    digits.push(match remainder {
                        -1 => b'T',
                        0 => b'0',
                        1 => b'1',
                        _ => unreachable!(),
                    });
                }
                SignedSystem::Negabinary => {
                    let remainder = n.rem_euclid(2);
                    n = (n - remainder) / -2;
                    digits.push(if remainder == 0 { b'0' } else { b'1' });
                }
            }
        }
        digits.reverse();
    }

    let mut output = String::from_utf8(digits).expect("ASCII digits");
    if with_checksum {
        let sum = checksum(output.as_bytes());
        output.push(':');
        output.push(char::from(b'0' + sum));
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

    let payload = &bytes[..payload_end];

    for (index, &byte) in payload.iter().enumerate() {
        if digit_value(byte, &system).is_none() {
            return Err(SignedError::InvalidByte { index });
        }
    }

    let supplied_checksum = if let Some(colon_index) = colon {
        if bytes.len() != colon_index + 2
            || !(b'0'..=b'6').contains(&bytes[colon_index + 1])
        {
            return Err(SignedError::InvalidChecksum { index: colon_index });
        }
        Some(bytes[colon_index + 1] - b'0')
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
        let digit = digit_value(byte, &system).expect("payload already validated");
        accumulator = accumulator * base + digit;
        if !(-LIMIT..=LIMIT).contains(&accumulator) {
            return Err(SignedError::Range { index });
        }
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
    checksum: bool,
) -> Result<String, SignedError> {
    let value = signed_decode(input, from)?;
    signed_encode(value, to, checksum)
}