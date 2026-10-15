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

fn checksum(payload: &[u8]) -> u8 {
    payload.iter().enumerate().fold(0_u64, |sum, (index, byte)| {
        let code = match byte {
            b'T' => 2,
            b'1' => 1,
            _ => 0,
        };
        (sum + ((index as u64 + 1) * code)) % 7
    }) as u8
}

pub fn signed_encode(
    value: i128,
    system: SignedSystem,
    checksum_enabled: bool,
) -> Result<String, SignedError> {
    if !(-LIMIT..=LIMIT).contains(&value) {
        return Err(SignedError::OutOfRange);
    }

    let mut digits = Vec::new();
    let mut current = value;

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
    }

    digits.reverse();
    let mut output = String::from_utf8(digits).expect("encoder emits ASCII digits");
    if checksum_enabled {
        let sum = checksum(output.as_bytes());
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

    let (base, mut accumulator) = match system {
        SignedSystem::BalancedTernary => (3_i128, 0_i128),
        SignedSystem::Negabinary => (-2_i128, 0_i128),
    };

    for (index, &byte) in payload.iter().enumerate() {
        let digit = match byte {
            b'T' => -1,
            b'0' => 0,
            b'1' => 1,
            _ => unreachable!("payload was lexically validated"),
        };
        accumulator = accumulator
            .checked_mul(base)
            .and_then(|value| value.checked_add(digit))
            .ok_or(SignedError::Range { index })?;
        if !(-LIMIT..=LIMIT).contains(&accumulator) {
            return Err(SignedError::Range { index });
        }
    }

    if let Some(found) = supplied_checksum {
        let expected = checksum(payload);
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
    checksum_enabled: bool,
) -> Result<String, SignedError> {
    let value = signed_decode(input, from)?;
    signed_encode(value, to, checksum_enabled)
}

fn main() {}