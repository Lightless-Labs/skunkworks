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

fn checksum(digits: &[u8]) -> u8 {
    let mut sum = 0u8;
    for (i, &byte) in digits.iter().enumerate() {
        let code = match byte {
            b'T' => 2u8,
            b'1' => 1u8,
            _ => 0u8,
        };
        sum = (sum + (((i as u8).wrapping_add(1) % 7) * code)) % 7;
    }
    sum
}

pub fn signed_encode(
    value: i128,
    system: SignedSystem,
    checksum_enabled: bool,
) -> Result<String, SignedError> {
    if value < -LIMIT || value > LIMIT {
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
                    let rem = n.rem_euclid(3);
                    let d = if rem == 2 { -1 } else { rem };
                    digits.push(match d {
                        -1 => b'T',
                        0 => b'0',
                        _ => b'1',
                    });
                    n = (n - d) / 3;
                }
                SignedSystem::Negabinary => {
                    let d = n.rem_euclid(2);
                    digits.push(if d == 0 { b'0' } else { b'1' });
                    n = (n - d) / -2;
                }
            }
        }
    }

    digits.reverse();
    let mut result = String::from_utf8(digits).expect("ASCII digit alphabet");
    if checksum_enabled {
        let value = checksum(result.as_bytes());
        result.push(':');
        result.push(char::from(b'0' + value));
    }
    Ok(result)
}

pub fn signed_decode(input: &str, system: SignedSystem) -> Result<i128, SignedError> {
    let bytes = input.as_bytes();
    let colon = bytes.iter().position(|&b| b == b':');
    let end = colon.unwrap_or(bytes.len());

    if end == 0 {
        return Err(SignedError::Empty);
    }

    let payload = &bytes[..end];
    for (index, &byte) in payload.iter().enumerate() {
        let valid = match system {
            SignedSystem::BalancedTernary => matches!(byte, b'T' | b'0' | b'1'),
            SignedSystem::Negabinary => matches!(byte, b'0' | b'1'),
        };
        if !valid {
            return Err(SignedError::InvalidByte { index });
        }
    }

    let supplied = if let Some(colon_index) = colon {
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

    let base = match system {
        SignedSystem::BalancedTernary => 3i128,
        SignedSystem::Negabinary => -2i128,
    };
    let mut acc = 0i128;

    for (index, &byte) in payload.iter().enumerate() {
        let digit = match byte {
            b'T' => -1i128,
            b'1' => 1i128,
            _ => 0i128,
        };

        let next = acc.checked_mul(base).and_then(|x| x.checked_add(digit));
        let Some(next) = next else {
            return Err(SignedError::Range { index });
        };
        acc = next;

        if acc < -LIMIT || acc > LIMIT {
            return Err(SignedError::Range { index });
        }
    }

    if let Some(found) = supplied {
        let expected = checksum(payload);
        if found != expected {
            return Err(SignedError::ChecksumMismatch { expected, found });
        }
    }

    Ok(acc)
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