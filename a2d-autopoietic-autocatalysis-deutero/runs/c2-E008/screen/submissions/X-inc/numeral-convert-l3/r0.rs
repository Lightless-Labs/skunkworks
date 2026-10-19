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
    let mut digits = Vec::new();

    if n == 0 {
        digits.push(b'0');
    } else {
        match system {
            SignedSystem::BalancedTernary => {
                while n != 0 {
                    let mut rem = n % 3;
                    n /= 3;
                    if rem == 2 {
                        rem = -1;
                        n += 1;
                    } else if rem == -2 {
                        rem = 1;
                        n -= 1;
                    }
                    digits.push(match rem {
                        -1 => b'T',
                        0 => b'0',
                        1 => b'1',
                        _ => unreachable!(),
                    });
                }
            }
            SignedSystem::Negabinary => {
                while n != 0 {
                    let rem = n.rem_euclid(2);
                    digits.push(if rem == 0 { b'0' } else { b'1' });
                    n = (n - rem) / -2;
                }
            }
        }
        digits.reverse();
    }

    let mut output = String::from_utf8(digits).expect("ASCII digits");
    if checksum {
        let sum = checksum_value(output.as_bytes());
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
    let colon = bytes.iter().position(|&b| b == b':');
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

    if let Some(index) = colon {
        if bytes.len() != index + 2 || !matches!(bytes[index + 1], b'0'..=b'6') {
            return Err(SignedError::InvalidChecksum { index });
        }
    }

    let payload = &bytes[..payload_end];
    if payload.len() > 1 && payload[0] == b'0' {
        return Err(SignedError::LeadingZero);
    }

    let (base, _) = match system {
        SignedSystem::BalancedTernary => (3_i128, ()),
        SignedSystem::Negabinary => (-2_i128, ()),
    };

    let mut accumulator = 0_i128;
    for (index, &byte) in payload.iter().enumerate() {
        let digit = match byte {
            b'T' => -1_i128,
            b'0' => 0,
            b'1' => 1,
            _ => unreachable!(),
        };
        accumulator = accumulator * base + digit;
        if !(-LIMIT..=LIMIT).contains(&accumulator) {
            return Err(SignedError::Range { index });
        }
    }

    if let Some(index) = colon {
        let expected = checksum_value(payload);
        let found = bytes[index + 1] - b'0';
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

fn checksum_value(payload: &[u8]) -> u8 {
    payload
        .iter()
        .enumerate()
        .fold(0_u8, |sum, (index, &byte)| {
            let code = match byte {
                b'T' => 2_u8,
                b'0' => 0,
                b'1' => 1,
                _ => 0,
            };
            (sum + (((index as u64 + 1) % 7) as u8) * code) % 7
        })
}