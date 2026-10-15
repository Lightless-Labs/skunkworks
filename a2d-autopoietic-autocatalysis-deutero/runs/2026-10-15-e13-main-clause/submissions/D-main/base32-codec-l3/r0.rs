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
    }

    if bits > 0 {
        let value = ((accumulator << (5 - bits)) & 0x1f) as u8;
        output.push(DATA_ALPHABET[value as usize] as char);
        remainder = ((u16::from(remainder) * 32 + u16::from(value)) % 37) as u8;
    }

    if checked {
        output.push(CHECK_ALPHABET[remainder as usize] as char);
    }

    output
}

fn normalize_symbol(byte: u8, check: bool) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'A'..=b'Z' | b'a'..=b'z' => {
            let upper = byte.to_ascii_uppercase();
            match upper {
                b'I' | b'L' => Some(1),
                b'O' => Some(0),
                _ => DATA_ALPHABET
                    .iter()
                    .position(|&symbol| symbol == upper)
                    .map(|value| value as u8),
            }
        }
        b'*' if check => Some(32),
        b'~' if check => Some(33),
        b'$' if check => Some(34),
        b'=' if check => Some(35),
        _ => None,
    }
}

pub fn decode(input: &str, checked: bool) -> Result<Vec<u8>, DecodeError> {
    let significant: Vec<(usize, u8)> = input
        .bytes()
        .enumerate()
        .filter(|&(_, byte)| byte != b'-')
        .collect();

    if checked && significant.is_empty() {
        return Err(DecodeError::MissingCheck {
            offset: input.len(),
        });
    }

    let data_len = significant.len() - usize::from(checked);
    let mut values = Vec::with_capacity(data_len);

    for &(offset, byte) in &significant[..data_len] {
        match normalize_symbol(byte, false) {
            Some(value) => values.push((offset, value)),
            None => return Err(DecodeError::InvalidByte { offset, byte }),
        }
    }

    let check_value = if checked {
        let (offset, byte) = significant[data_len];
        match normalize_symbol(byte, true) {
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

    if unused_bits > 0 {
        let (offset, value) = values[data_len - 1];
        if value & ((1 << unused_bits) - 1) != 0 {
            return Err(DecodeError::TrailingBits { offset });
        }
    }

    if let Some((offset, found)) = check_value {
        let mut remainder = 0u8;
        for &(_, value) in &values {
            remainder = ((u16::from(remainder) * 32 + u16::from(value)) % 37) as u8;
        }
        if found != remainder {
            return Err(DecodeError::CheckMismatch {
                offset,
                expected: remainder,
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
    }

    Ok(output)
}

fn main() {}