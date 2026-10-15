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

fn checksum(payload: &[u8]) -> u8 {
    payload
        .iter()
        .enumerate()
        .fold(0_u64, |sum, (index, &byte)| {
            let code = match byte {
                b'T' => 2_u64,
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

    let mut current = value;
    let mut reversed = Vec::new();

    if current == 0 {
        reversed.push('0');
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
            reversed.push(digit_char(digit));
        }
    }

    reversed.reverse();
    let mut output: String = reversed.into_iter().collect();

    if checksum_enabled {
        let check = checksum(output.as_bytes());
        output.push(':');
        output.push(char::from(b'0' + check));
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

    for (index, &byte) in bytes[..payload_end].iter().enumerate() {
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

    let payload = &bytes[..payload_end];

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
        let Some(next) = accumulator.checked_mul(base).and_then(|v| v.checked_add(digit)) else {
            return Err(SignedError::Range { index });
        };
        if !(-LIMIT..=LIMIT).contains(&next) {
            return Err(SignedError::Range { index });
        }
        accumulator = next;
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
    checksum_enabled: bool,
) -> Result<String, SignedError> {
    let value = signed_decode(input, from)?;
    signed_encode(value, to, checksum_enabled)
}

fn main() {}