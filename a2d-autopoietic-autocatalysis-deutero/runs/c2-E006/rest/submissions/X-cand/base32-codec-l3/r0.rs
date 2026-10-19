#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    MissingCheck { offset: usize },
    InvalidByte { offset: usize, byte: u8 },
    InvalidLength { offset: usize },
    TrailingBits { offset: usize },
    CheckMismatch {
        offset: usize,
        expected: u8,
        found: u8,
    },
}

const DATA_ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const CHECK_ALPHABET: &[u8; 37] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ*~$=U";

pub fn encode(input: &[u8], checked: bool) -> String {
    let mut output = String::new();
    let mut remainder = 0u8;

    let mut bit_buffer = 0u16;
    let mut bits_in_buffer = 0u8;

    for &byte in input {
        bit_buffer = (bit_buffer << 8) | u16::from(byte);
        bits_in_buffer += 8;

        while bits_in_buffer >= 5 {
            bits_in_buffer -= 5;
            let value = ((bit_buffer >> bits_in_buffer) & 0x1f) as u8;
            output.push(DATA_ALPHABET[value as usize] as char);
            remainder = ((u16::from(remainder) * 32 + u16::from(value)) % 37) as u8;
        }

        bit_buffer &= (1u16 << bits_in_buffer) - 1;
    }

    if bits_in_buffer != 0 {
        let value = ((bit_buffer << (5 - bits_in_buffer)) & 0x1f) as u8;
        output.push(DATA_ALPHABET[value as usize] as char);
        remainder = ((u16::from(remainder) * 32 + u16::from(value)) % 37) as u8;
    }

    if checked {
        output.push(CHECK_ALPHABET[remainder as usize] as char);
    }

    output
}

fn data_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'A'..=b'H' => Some(byte - b'A' + 10),
        b'a'..=b'h' => Some(byte - b'a' + 10),
        b'I' | b'i' | b'L' | b'l' => Some(1),
        b'J'..=b'K' => Some(byte - b'J' + 18),
        b'j'..=b'k' => Some(byte - b'j' + 18),
        b'M'..=b'N' => Some(byte - b'M' + 20),
        b'm'..=b'n' => Some(byte - b'm' + 20),
        b'O' | b'o' => Some(0),
        b'P'..=b'T' => Some(byte - b'P' + 22),
        b'p'..=b't' => Some(byte - b'p' + 22),
        b'V'..=b'Z' => Some(byte - b'V' + 27),
        b'v'..=b'z' => Some(byte - b'v' + 27),
        _ => None,
    }
}

fn check_value(byte: u8) -> Option<u8> {
    match byte {
        b'*' => Some(32),
        b'~' => Some(33),
        b'$' => Some(34),
        b'=' => Some(35),
        b'U' | b'u' => Some(36),
        _ => data_value(byte),
    }
}

pub fn decode(input: &str, checked: bool) -> Result<Vec<u8>, DecodeError> {
    let bytes = input.as_bytes();
    let significant: Vec<(usize, u8)> = bytes
        .iter()
        .copied()
        .enumerate()
        .filter(|&(_, byte)| byte != b'-')
        .collect();

    let (data_end, check_symbol) = if checked {
        let Some((&check, data)) = significant.split_last() else {
            return Err(DecodeError::MissingCheck {
                offset: input.len(),
            });
        };
        (data.len(), Some(check))
    } else {
        (significant.len(), None)
    };

    let mut values = Vec::with_capacity(data_end);
    for &(offset, byte) in &significant[..data_end] {
        let Some(value) = data_value(byte) else {
            return Err(DecodeError::InvalidByte { offset, byte });
        };
        values.push((value, offset));
    }

    let normalized_check = if let Some((offset, byte)) = check_symbol {
        let Some(value) = check_value(byte) else {
            return Err(DecodeError::InvalidByte { offset, byte });
        };
        Some((value, offset))
    } else {
        None
    };

    let residue = values.len() % 8;
    if !matches!(residue, 0 | 2 | 4 | 5 | 7) {
        return Err(DecodeError::InvalidLength {
            offset: input.len(),
        });
    }

    let unused_bits = match residue {
        2 => 2,
        4 => 4,
        5 => 1,
        7 => 3,
        _ => 0,
    };

    if unused_bits != 0 {
        let (last_value, last_offset) = values[values.len() - 1];
        if last_value & ((1u8 << unused_bits) - 1) != 0 {
            return Err(DecodeError::TrailingBits {
                offset: last_offset,
            });
        }
    }

    let mut remainder = 0u8;
    for &(value, _) in &values {
        remainder = ((u16::from(remainder) * 32 + u16::from(value)) % 37) as u8;
    }

    if let Some((found, offset)) = normalized_check {
        if found != remainder {
            return Err(DecodeError::CheckMismatch {
                offset,
                expected: remainder,
                found,
            });
        }
    }

    let mut output = Vec::with_capacity(values.len().saturating_mul(5) / 8);
    let mut bit_buffer = 0u16;
    let mut bits_in_buffer = 0u8;

    for &(value, _) in &values {
        bit_buffer = (bit_buffer << 5) | u16::from(value);
        bits_in_buffer += 5;

        while bits_in_buffer >= 8 {
            bits_in_buffer -= 8;
            output.push(((bit_buffer >> bits_in_buffer) & 0xff) as u8);
        }

        bit_buffer &= (1u16 << bits_in_buffer) - 1;
    }

    Ok(output)
}