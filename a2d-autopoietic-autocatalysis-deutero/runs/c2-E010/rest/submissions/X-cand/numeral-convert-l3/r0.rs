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

fn digit(byte: u8, system: &SignedSystem) -> Option<i128> {
    match (system, byte) {
        (SignedSystem::BalancedTernary, b'T') => Some(-1),
        (_, b'0') => Some(0),
        (_, b'1') => Some(1),
        _ => None,
    }
}

fn checksum_bytes(bytes: &[u8]) -> u8 {
    bytes.iter().enumerate().fold(0u8, |sum, (i, &b)| {
        let code = if b == b'T' { 2u16 } else if b == b'1' { 1 } else { 0 };
        ((u16::from(sum) + (((i as u128 + 1) % 7) as u16) * code) % 7) as u8
    })
}

pub fn signed_encode(
    value: i128,
    system: SignedSystem,
    with_checksum: bool,
) -> Result<String, SignedError> {
    if value < -LIMIT || value > LIMIT {
        return Err(SignedError::OutOfRange);
    }

    // Use little-endian output digits, then reverse them for display.
    let mut n = value;
    let mut digits = Vec::new();
    while n != 0 {
        let (d, next) = match system {
            SignedSystem::BalancedTernary => {
                let r = n.rem_euclid(3);
                let d = if r == 2 { -1 } else { r };
                (d, (n - d) / 3)
            }
            SignedSystem::Negabinary => {
                let d = n.rem_euclid(2);
                (d, (n - d) / -2)
            }
        };
        digits.push(match d {
            -1 => b'T',
            0 => b'0',
            _ => b'1',
        });
        n = next;
    }

    if digits.is_empty() {
        digits.push(b'0');
    }
    let mut lo = 0usize;
    let mut hi = digits.len();
    while lo < hi {
        hi -= 1;
        digits.swap(lo, hi);
        lo += 1;
    }

    let mut result = String::with_capacity(digits.len() + if with_checksum { 2 } else { 0 });
    for &b in &digits {
        result.push(char::from(b));
    }
    if with_checksum {
        let c = checksum_bytes(&digits);
        result.push(':');
        result.push(char::from(b'0' + c));
    }
    Ok(result)
}

pub fn signed_decode(input: &str, system: SignedSystem) -> Result<i128, SignedError> {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return Err(SignedError::Empty);
    }

    let colon = bytes.iter().position(|&b| b == b':');
    let end = colon.unwrap_or(bytes.len());
    if end == 0 {
        return Err(SignedError::Empty);
    }

    for (i, &b) in bytes[..end].iter().enumerate() {
        if digit(b, &system).is_none() {
            return Err(SignedError::InvalidByte { index: i });
        }
    }

    if let Some(c) = colon {
        if bytes.len() != c + 2 || !(b'0'..=b'6').contains(&bytes[c + 1]) {
            return Err(SignedError::InvalidChecksum { index: c });
        }
    }

    let payload = &bytes[..end];
    if payload.len() > 1 && payload[0] == b'0' {
        return Err(SignedError::LeadingZero);
    }

    let base = match system {
        SignedSystem::BalancedTernary => 3i128,
        SignedSystem::Negabinary => -2i128,
    };
    let mut value = 0i128;
    for (i, &b) in payload.iter().enumerate() {
        let d = digit(b, &system).expect("digit validated above");
        value = match value.checked_mul(base).and_then(|x| x.checked_add(d)) {
            Some(x) if (-LIMIT..=LIMIT).contains(&x) => x,
            _ => return Err(SignedError::Range { index: i }),
        };
    }

    if let Some(c) = colon {
        let expected = checksum_bytes(payload);
        let found = bytes[c + 1] - b'0';
        if expected != found {
            return Err(SignedError::ChecksumMismatch { expected, found });
        }
    }

    Ok(value)
}

pub fn signed_recode(
    input: &str,
    from: SignedSystem,
    to: SignedSystem,
    with_checksum: bool,
) -> Result<String, SignedError> {
    match signed_decode(input, from) {
        Ok(value) => signed_encode(value, to, with_checksum),
        Err(error) => Err(error),
    }
}
