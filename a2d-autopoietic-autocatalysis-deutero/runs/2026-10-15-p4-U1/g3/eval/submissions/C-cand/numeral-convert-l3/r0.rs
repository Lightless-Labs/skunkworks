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
    payload
        .iter()
        .enumerate()
        .fold(0_u64, |sum, (index, &byte)| {
            let code = match byte {
                b'T' => 2_u64,
                b'1' => 1_u64,
                _ => 0_u64,
            };
            (sum + ((index as u64 + 1) * code)) % 7
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
                    let mut remainder = n % 2;
                    if remainder < 0 {
                        remainder += 2;
                    }
                    reversed.push(if remainder == 0 { b'0' } else { b'1' });
                    n = (n - remainder) / -2;
                }
            }
        }
    }

    reversed.reverse();
    let mut result = String::from_utf8(reversed).expect("ASCII digits are valid UTF-8");

    if with_checksum {
        let value = checksum(result.as_bytes());
        result.push(':');
        result.push(char::from(b'0' + value));
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

    if payload_end == 0 {
        return Err(SignedError::Empty);
    }

    let payload = &bytes[..payload_end];

    for (index, &byte) in payload.iter().enumerate() {
        if digit_value(byte, &system).is_none() {
            return Err(SignedError::InvalidByte { index });
        }
    }

    if let Some(colon_index) = colon {
        let suffix = &bytes[colon_index + 1..];
        if suffix.len() != 1 || !(b'0'..=b'6').contains(&suffix[0]) {
            return Err(SignedError::InvalidChecksum { index: colon_index });
        }
    }

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
        accumulator = accumulator
            .checked_mul(base)
            .and_then(|value| value.checked_add(digit))
            .ok_or(SignedError::Range { index })?;

        if !(-LIMIT..=LIMIT).contains(&accumulator) {
            return Err(SignedError::Range { index });
        }
    }

    if let Some(colon_index) = colon {
        let expected = checksum(payload);
        let found = bytes[colon_index + 1] - b'0';
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
    with_checksum: bool,
) -> Result<String, SignedError> {
    let value = signed_decode(input, from)?;
    signed_encode(value, to, with_checksum)
}

fn main() {}