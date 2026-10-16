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

fn checksum_digit(payload: &str) -> u8 {
    payload
        .bytes()
        .enumerate()
        .fold(0_u64, |sum, (index, byte)| {
            let code = match byte {
                b'T' => 2,
                b'0' => 0,
                b'1' => 1,
                _ => 0,
            };
            (sum + ((index as u64 + 1) * code)) % 7
        }) as u8
}

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
                    if digit < 0 {
                        digit += 3;
                    }
                    if digit == 2 {
                        digit = -1;
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
    let mut output = String::from_utf8(reversed).expect("digits are ASCII");
    if checksum {
        let digit = checksum_digit(&output);
        output.push(':');
        output.push((b'0' + digit) as char);
    }
    Ok(output)
}

pub fn signed_decode(input: &str, system: SignedSystem) -> Result<i128, SignedError> {
    if input.is_empty() {
        return Err(SignedError::Empty);
    }

    let colon = input.as_bytes().iter().position(|&b| b == b':');
    let payload_end = colon.unwrap_or(input.len());
    if payload_end == 0 {
        return Err(SignedError::Empty);
    }

    let payload = &input.as_bytes()[..payload_end];
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
        let suffix = &input.as_bytes()[colon_index + 1..];
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

    let base: i128 = match system {
        SignedSystem::BalancedTernary => 3,
        SignedSystem::Negabinary => -2,
    };
    let mut accumulator: i128 = 0;

    for (index, &byte) in payload.iter().enumerate() {
        let digit: i128 = match byte {
            b'T' => -1,
            b'0' => 0,
            b'1' => 1,
            _ => unreachable!(),
        };

        let next = accumulator
            .checked_mul(base)
            .and_then(|value| value.checked_add(digit));
        accumulator = match next {
            Some(value) if (-LIMIT..=LIMIT).contains(&value) => value,
            _ => return Err(SignedError::Range { index }),
        };
    }

    if let Some(found) = supplied_checksum {
        let expected = checksum_digit(std::str::from_utf8(payload).expect("validated ASCII"));
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