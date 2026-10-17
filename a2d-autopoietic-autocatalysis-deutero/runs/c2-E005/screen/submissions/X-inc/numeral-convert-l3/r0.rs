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

pub fn signed_encode(
    value: i128,
    system: SignedSystem,
    checksum: bool,
) -> Result<String, SignedError> {
    if !(-LIMIT..=LIMIT).contains(&value) {
        return Err(SignedError::OutOfRange);
    }

    let mut n = value;
    let mut reversed = Vec::new();

    if n == 0 {
        reversed.push(b'0');
    } else {
        while n != 0 {
            match system {
                SignedSystem::BalancedTernary => {
                    let mut digit = n % 3;
                    if digit == 2 {
                        digit = -1;
                    } else if digit == -2 {
                        digit = 1;
                    }

                    reversed.push(match digit {
                        -1 => b'T',
                        0 => b'0',
                        _ => b'1',
                    });
                    n = (n - digit) / 3;
                }
                SignedSystem::Negabinary => {
                    let digit = n.rem_euclid(2);
                    reversed.push(if digit == 0 { b'0' } else { b'1' });
                    n = (n - digit) / -2;
                }
            }
        }
    }

    reversed.reverse();
    let digits = String::from_utf8(reversed).expect("ASCII digits");
    if !checksum {
        return Ok(digits);
    }

    let check = checksum_value(digits.as_bytes());
    Ok(format!("{digits}:{check}"))
}

pub fn signed_decode(input: &str, system: SignedSystem) -> Result<i128, SignedError> {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return Err(SignedError::Empty);
    }

    let colon = bytes.iter().position(|&b| b == b':');
    let payload_end = colon.unwrap_or(bytes.len());
    if payload_end == 0 {
        return Err(SignedError::Empty);
    }

    let payload = &bytes[..payload_end];

    for (index, &byte) in payload.iter().enumerate() {
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

    if payload.len() > 1 && payload[0] == b'0' {
        return Err(SignedError::LeadingZero);
    }

    let mut accumulator: i128 = 0;
    for (index, &byte) in payload.iter().enumerate() {
        let digit = match byte {
            b'T' => -1,
            b'0' => 0,
            b'1' => 1,
            _ => unreachable!("payload was validated"),
        };
        let base = match system {
            SignedSystem::BalancedTernary => 3,
            SignedSystem::Negabinary => -2,
        };

        accumulator = accumulator
            .checked_mul(base)
            .and_then(|n| n.checked_add(digit))
            .ok_or(SignedError::Range { index })?;

        if !(-LIMIT..=LIMIT).contains(&accumulator) {
            return Err(SignedError::Range { index });
        }
    }

    if let Some(found) = supplied_checksum {
        let expected = checksum_value(payload);
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

fn checksum_value(payload: &[u8]) -> u8 {
    payload
        .iter()
        .enumerate()
        .fold(0_u8, |sum, (index, &byte)| {
            let code = match byte {
                b'T' => 2,
                b'0' => 0,
                b'1' => 1,
                _ => 0,
            };
            (sum + (((index + 1) % 7) as u8) * code) % 7
        })
}