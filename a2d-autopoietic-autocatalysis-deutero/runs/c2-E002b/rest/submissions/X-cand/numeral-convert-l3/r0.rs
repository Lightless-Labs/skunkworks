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

fn checksum(payload: &[u8]) -> u8 {
    let sum = payload.iter().enumerate().fold(0_u64, |sum, (i, &byte)| {
        let code = match byte {
            b'T' => 2_u64,
            b'0' => 0,
            b'1' => 1,
            _ => 0,
        };
        (sum + ((i as u64 + 1) % 7) * code) % 7
    });
    sum as u8
}

pub fn signed_encode(
    value: i128,
    system: SignedSystem,
    checksum_suffix: bool,
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
                    let mut rem = n % 3;
                    if rem == 2 {
                        rem = -1;
                    } else if rem == -2 {
                        rem = 1;
                    }
                    let digit = rem as i8;
                    reversed.push(match digit {
                        -1 => b'T',
                        0 => b'0',
                        _ => b'1',
                    });
                    n = (n - rem) / 3;
                }
                SignedSystem::Negabinary => {
                    let rem = n.rem_euclid(2);
                    reversed.push(if rem == 0 { b'0' } else { b'1' });
                    n = (n - rem) / -2;
                }
            }
        }
    }

    reversed.reverse();
    let mut output = String::from_utf8(reversed).expect("digits are ASCII");

    if checksum_suffix {
        let value = checksum(output.as_bytes());
        output.push(':');
        output.push(char::from(b'0' + value));
    }

    Ok(output)
}

pub fn signed_decode(input: &str, system: SignedSystem) -> Result<i128, SignedError> {
    if input.is_empty() {
        return Err(SignedError::Empty);
    }

    let bytes = input.as_bytes();
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

    let radix = base(&system);
    let mut value = 0_i128;

    for (index, &byte) in payload.iter().enumerate() {
        let digit = match byte {
            b'T' => -1,
            b'0' => 0,
            b'1' => 1,
            _ => unreachable!(),
        };

        value = value
            .checked_mul(radix)
            .and_then(|v| v.checked_add(digit))
            .ok_or(SignedError::Range { index })?;

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
    checksum_suffix: bool,
) -> Result<String, SignedError> {
    let value = signed_decode(input, from)?;
    signed_encode(value, to, checksum_suffix)
}