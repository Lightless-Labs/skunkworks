#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    MissingCheck { offset: usize },
    InvalidByte { offset: usize, byte: u8 },
    InvalidLength { offset: usize },
    TrailingBits { offset: usize },
    CheckMismatch { offset: usize, expected: u8, found: u8 },
}

const DATA_ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const CHECK_ALPHABET: &[u8; 37] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ*~$=U";

fn data_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'A'..=b'H' => Some(byte - b'A' + 10),
        b'a'..=b'h' => Some(byte - b'a' + 10),
        b'J'..=b'K' => Some(byte - b'J' + 18),
        b'j'..=b'k' => Some(byte - b'j' + 18),
        b'M'..=b'N' => Some(byte - b'M' + 20),
        b'm'..=b'n' => Some(byte - b'm' + 20),
        b'P'..=b'T' => Some(byte - b'P' + 22),
        b'p'..=b't' => Some(byte - b'p' + 22),
        b'V'..=b'Z' => Some(byte - b'V' + 27),
        b'v'..=b'z' => Some(byte - b'v' + 27),
        b'I' | b'i' | b'L' | b'l' => Some(1),
        b'O' | b'o' => Some(0),
        _ => None,
    }
}

fn check_value(byte: u8) -> Option<u8> {
    if let Some(value) = data_value(byte) {
        return Some(value);
    }
    match byte {
        b'*' => Some(32),
        b'~' => Some(33),
        b'$' => Some(34),
        b'=' => Some(35),
        b'U' | b'u' => Some(36),
        _ => None,
    }
}

pub fn encode(input: &[u8], checked: bool) -> String {
    let mut output = String::new();
    let mut remainder = 0u8;
    let mut accumulator = 0u16;
    let mut bits = 0u8;

    for &byte in input {
        accumulator = (accumulator << 8) | u16::from(byte);
        bits += 8;

        while bits >= 5 {
            bits -= 5;
            let value = ((accumulator >> bits) & 0x1f) as u8;
            output.push(DATA_ALPHABET[value as usize] as char);
            remainder = ((u16::from(remainder) * 32 + u16::from(value)) % 37) as u8;
        }

        accumulator &= (1u16 << bits) - 1;
    }

    if bits != 0 {
        let value = ((accumulator << (5 - bits)) & 0x1f) as u8;
        output.push(DATA_ALPHABET[value as usize] as char);
        remainder = ((u16::from(remainder) * 32 + u16::from(value)) % 37) as u8;
    }

    if checked {
        output.push(CHECK_ALPHABET[remainder as usize] as char);
    }

    output
}

pub fn decode(input: &str, checked: bool) -> Result<Vec<u8>, DecodeError> {
    let bytes = input.as_bytes();
    let significant: Vec<(usize, u8)> = bytes
        .iter()
        .copied()
        .enumerate()
        .filter(|&(_, byte)| byte != b'-')
        .collect();

    if checked && significant.is_empty() {
        return Err(DecodeError::MissingCheck {
            offset: input.len(),
        });
    }

    let data_len = if checked {
        significant.len() - 1
    } else {
        significant.len()
    };

    let mut values = Vec::with_capacity(data_len);
    for &(offset, byte) in significant.iter().take(data_len) {
        match data_value(byte) {
            Some(value) => values.push((offset, value)),
            None => return Err(DecodeError::InvalidByte { offset, byte }),
        }
    }

    let check = if checked {
        let &(offset, byte) = significant.last().unwrap();
        match check_value(byte) {
            Some(value) => Some((offset, value)),
            None => return Err(DecodeError::InvalidByte { offset, byte }),
        }
    } else {
        None
    };

    let residue = data_len % 8;
    let unused_bits = match residue {
        0 => 0,
        2 => 2,
        4 => 4,
        5 => 1,
        7 => 3,
        _ => {
            return Err(DecodeError::InvalidLength {
                offset: input.len(),
            });
        }
    };

    if unused_bits != 0 {
        let (offset, value) = values.last().copied().unwrap();
        if value & ((1 << unused_bits) - 1) != 0 {
            return Err(DecodeError::TrailingBits { offset });
        }
    }

    if let Some((offset, found)) = check {
        let mut remainder = 0u16;
        for &(_, value) in &values {
            remainder = (remainder * 32 + u16::from(value)) % 37;
        }
        let expected = remainder as u8;
        if found != expected {
            return Err(DecodeError::CheckMismatch {
                offset,
                expected,
                found,
            });
        }
    }

    let mut output = Vec::with_capacity(data_len * 5 / 8);
    let mut accumulator = 0u16;
    let mut bits = 0u8;

    for &(_, value) in &values {
        accumulator = (accumulator << 5) | u16::from(value);
        bits += 5;

        while bits >= 8 {
            bits -= 8;
            output.push(((accumulator >> bits) & 0xff) as u8);
        }

        accumulator &= (1u16 << bits) - 1;
    }

    Ok(output)
}