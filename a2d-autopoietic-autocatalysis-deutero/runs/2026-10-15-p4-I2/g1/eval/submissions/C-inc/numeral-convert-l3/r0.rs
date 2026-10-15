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

const LIMIT: i128 = (1i128 << 120) - 1;

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

fn digit_char(digit: i128) -> char {
    match digit {
        -1 => 'T',
        0 => '0',
        1 => '1',
        _ => unreachable!(),
    }
}

fn checksum(payload: &str) -> u8 {
    payload
        .bytes()
        .enumerate()
        .fold(0u64, |sum, (index, byte)| {
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
    with_checksum: bool,
) -> Result<String, SignedError> {
    if !(-LIMIT..=LIMIT).contains(&value) {
        return Err(SignedError::OutOfRange);
    }

    let mut digits = Vec::new();
    let mut current = value;

    if current == 0 {
        digits.push('0');
    } else {
        while current != 0 {
            let digit = match system {
                SignedSystem::BalancedTernary => {
                    let remainder = current.rem_euclid(3);
                    let digit = if remainder == 2 { -1 } else { remainder };
                    current = (current - digit) / 3;
                    digit
                }
                SignedSystem::Negabinary => {
                    let digit = current.rem_euclid(2);
                    current = (current - digit) / -2;
                    digit
                }
            };
            digits.push(digit_char(digit));
        }
        digits.reverse();
    }

    let mut output: String = digits.into_iter().collect();
    if with_checksum {
        let check = checksum(&output);
        output.push(':');
        output.push(char::from(b'0' + check));
    }
    Ok(output)
}

pub fn signed_decode(input: &str, system: SignedSystem) -> Result<i128, SignedError> {
    if input.is_empty() {
        return Err(SignedError::Empty);
    }

    let colon = input.as_bytes().iter().position(|&byte| byte == b':');
    let payload_end = colon.unwrap_or(input.len());

    if payload_end == 0 {
        return Err(SignedError::Empty);
    }

    let payload_bytes = &input.as_bytes()[..payload_end];
    for (index, &byte) in payload_bytes.iter().enumerate() {
        if digit_value(byte, &system).is_none() {
            return Err(SignedError::InvalidByte { index });
        }
    }

    let supplied_checksum = if let Some(colon_index) = colon {
        let suffix = &input.as_bytes()[colon_index + 1..];
        if suffix.len() != 1 || !(b'0'..=b'6').contains(&suffix[0]) {
            return Err(SignedError::InvalidChecksum { index: colon_index });
        }
        Some((colon_index, suffix[0] - b'0'))
    } else {
        None
    };

    if payload_bytes.len() > 1 && payload_bytes[0] == b'0' {
        return Err(SignedError::LeadingZero);
    }

    let base: i128 = match system {
        SignedSystem::BalancedTernary => 3,
        SignedSystem::Negabinary => -2,
    };

    let mut accumulator = 0i128;
    for (index, &byte) in payload_bytes.iter().enumerate() {
        let digit = digit_value(byte, &system).expect("payload already validated");
        accumulator = accumulator
            .checked_mul(base)
            .and_then(|n| n.checked_add(digit))
            .ok_or(SignedError::Range { index })?;

        if !(-LIMIT..=LIMIT).contains(&accumulator) {
            return Err(SignedError::Range { index });
        }
    }

    if let Some((_, found)) = supplied_checksum {
        let computed = checksum(&input[..payload_end]);
        if computed != found {
            return Err(SignedError::ChecksumMismatch {
                expected: computed,
                found,
            });
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