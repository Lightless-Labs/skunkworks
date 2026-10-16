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
                    let rem = n.rem_euclid(3);
                    let digit = if rem == 2 { -1 } else { rem };
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
    let mut output = String::from_utf8(reversed).expect("ASCII digits");

    if checksum {
        let sum = output
            .bytes()
            .enumerate()
            .map(|(i, byte)| (i as u8 + 1) * digit_code(byte))
            .fold(0_u8, u8::wrapping_add)
            % 7;
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

    if let Some(colon_index) = colon {
        let suffix = &bytes[colon_index + 1..];
        if suffix.len() != 1 || !matches!(suffix[0], b'0'..=b'6') {
            return Err(SignedError::InvalidChecksum { index: colon_index });
        }
    }

    let payload = &bytes[..payload_end];
    if payload.len() > 1 && payload[0] == b'0' {
        return Err(SignedError::LeadingZero);
    }

    let mut acc = 0_i128;
    for (index, &byte) in payload.iter().enumerate() {
        let (base, digit) = match system {
            SignedSystem::BalancedTernary => (
                3_i128,
                match byte {
                    b'T' => -1_i128,
                    b'0' => 0,
                    _ => 1,
                },
            ),
            SignedSystem::Negabinary => (
                -2_i128,
                if byte == b'0' { 0_i128 } else { 1 },
            ),
        };

        acc = acc * base + digit;
        if !(-LIMIT..=LIMIT).contains(&acc) {
            return Err(SignedError::Range { index });
        }
    }

    if let Some(colon_index) = colon {
        let expected = payload
            .iter()
            .enumerate()
            .map(|(i, &byte)| ((i as u8 + 1) * digit_code(byte)) % 7)
            .fold(0_u8, |sum, value| (sum + value) % 7);
        let found = bytes[colon_index + 1] - b'0';
        if expected != found {
            return Err(SignedError::ChecksumMismatch { expected, found });
        }
    }

    Ok(acc)
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

fn digit_code(byte: u8) -> u8 {
    match byte {
        b'T' => 2,
        b'1' => 1,
        _ => 0,
    }
}