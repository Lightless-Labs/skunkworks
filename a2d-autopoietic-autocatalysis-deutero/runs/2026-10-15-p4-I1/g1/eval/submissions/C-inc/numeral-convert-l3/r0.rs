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
        (_, b'0') => Some(0),
        (_, b'1') => Some(1),
        (SignedSystem::BalancedTernary, b'T') => Some(-1),
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
        .fold(0u32, |sum, (index, byte)| {
            let code = match byte {
                b'T' => 2,
                b'0' => 0,
                b'1' => 1,
                _ => 0,
            };
            (sum + ((index as u32 + 1) * code)) % 7
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

    let mut n = value;
    let mut reversed = Vec::new();

    if n == 0 {
        reversed.push('0');
    } else {
        while n != 0 {
            match system {
                SignedSystem::BalancedTernary => {
                    let remainder = n.rem_euclid(3);
                    let digit = if remainder == 2 { -1 } else { remainder };
                    reversed.push(digit_char(digit));
                    n = (n - digit) / 3;
                }
                SignedSystem::Negabinary => {
                    let digit = n.rem_euclid(2);
                    reversed.push(digit_char(digit));
                    n = (n - digit) / -2;
                }
            }
        }
    }

    reversed.reverse();
    let mut output: String = reversed.into_iter().collect();

    if checksum_enabled {
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

    let colon = input.find(':');
    let payload_end = colon.unwrap_or(input.len());
    let payload = &input[..payload_end];

    if payload.is_empty() {
        return Err(SignedError::Empty);
    }

    for (index, byte) in payload.bytes().enumerate() {
        if digit_value(byte, &system).is_none() {
            return Err(SignedError::InvalidByte { index });
        }
    }

    let supplied_checksum = if let Some(colon_index) = colon {
        let suffix = &input[colon_index + 1..];
        let bytes = suffix.as_bytes();
        if bytes.len() != 1 || !(b'0'..=b'6').contains(&bytes[0]) {
            return Err(SignedError::InvalidChecksum { index: colon_index });
        }
        Some(bytes[0] - b'0')
    } else {
        None
    };

    if payload.len() > 1 && payload.as_bytes()[0] == b'0' {
        return Err(SignedError::LeadingZero);
    }

    let base: i128 = match system {
        SignedSystem::BalancedTernary => 3,
        SignedSystem::Negabinary => -2,
    };
    let mut accumulator = 0i128;

    for (index, byte) in payload.bytes().enumerate() {
        let digit = digit_value(byte, &system).expect("payload was validated");
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
    checksum_enabled: bool,
) -> Result<String, SignedError> {
    let value = signed_decode(input, from)?;
    signed_encode(value, to, checksum_enabled)
}

fn main() {}