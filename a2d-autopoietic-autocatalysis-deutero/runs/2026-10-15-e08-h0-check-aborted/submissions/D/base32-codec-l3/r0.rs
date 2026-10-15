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
    let mut accumulator = 0u32;
    let mut bits = 0u32;
    let mut remainder = 0u8;

    for &byte in input {
        accumulator = (accumulator << 8) | u32::from(byte);
        bits += 8;

        while bits >= 5 {
            bits -= 5;
            let value = ((accumulator >> bits) & 31) as u8;
            output.push(DATA_ALPHABET[value as usize] as char);
            remainder = ((u16::from(remainder) * 32 + u16::from(value)) % 37) as u8;
        }

        if bits == 0 {
            accumulator = 0;
        } else {
            accumulator &= (1u32 << bits) - 1;
        }
    }

    if bits > 0 {
        let value = ((accumulator << (5 - bits)) & 31) as u8;
        output.push(DATA_ALPHABET[value as usize] as char);
        remainder = ((u16::from(remainder) * 32 + u16::from(value)) % 37) as u8;
    }

    if checked {
        output.push(CHECK_ALPHABET[remainder as usize] as char);
    }

    output
}

fn normalize_data(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'A'..=b'H' => Some(byte - b'A' + 10),
        b'a'..=b'h' => Some(byte - b'a' + 10),
        b'I' | b'i' | b'L' | b'l' => Some(1),
        b'J'..=b'K' => Some(byte - b'A' + 10),
        b'j'..=b'k' => Some(byte - b'a' + 10),
        b'M'..=b'N' => Some(byte - b'A' + 10),
        b'm'..=b'n' => Some(byte - b'a' + 10),
        b'O' | b'o' => Some(0),
        b'P'..=b'T' => Some(byte - b'A' + 10),
        b'p'..=b't' => Some(byte - b'a' + 10),
        b'V'..=b'Z' => Some(byte - b'A' + 10),
        b'v'..=b'z' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn normalize_check(byte: u8) -> Option<u8> {
    match byte {
        b'*' => Some(32),
        b'~' => Some(33),
        b'$' => Some(34),
        b'=' => Some(35),
        b'U' | b'u' => Some(36),
        _ => normalize_data(byte),
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

    let check = if checked {
        match significant.last().copied() {
            Some(last) => Some(last),
            None => return Err(DecodeError::MissingCheck { offset: input.len() }),
        }
    } else {
        None
    };

    let data_len = significant.len() - usize::from(checked);
    let mut values = Vec::with_capacity(data_len);

    for &(offset, byte) in significant.iter().take(data_len) {
        let value = normalize_data(byte)
            .ok_or(DecodeError::InvalidByte { offset, byte })?;
        values.push((offset, value));
    }

    let normalized_check = if let Some((offset, byte)) = check {
        Some((
            offset,
            normalize_check(byte).ok_or(DecodeError::InvalidByte { offset, byte })?,
        ))
    } else {
        None
    };

    let residue = values.len() % 8;
    let unused_bits = match residue {
        0 => 0,
        2 => 2,
        4 => 4,
        5 => 1,
        7 => 3,
        _ => return Err(DecodeError::InvalidLength { offset: input.len() }),
    };

    if unused_bits > 0 {
        let (offset, last_value) = values[values.len() - 1];
        if last_value & ((1u8 << unused_bits) - 1) != 0 {
            return Err(DecodeError::TrailingBits { offset });
        }
    }

    if let Some((offset, found)) = normalized_check {
        let remainder = values.iter().fold(0u8, |remainder, &(_, value)| {
            ((u16::from(remainder) * 32 + u16::from(value)) % 37) as u8
        });

        if found != remainder {
            return Err(DecodeError::CheckMismatch {
                offset,
                expected: remainder,
                found,
            });
        }
    }

    let mut output = Vec::with_capacity(values.len() * 5 / 8);
    let mut accumulator = 0u32;
    let mut bits = 0u32;

    for &(_, value) in &values {
        accumulator = (accumulator << 5) | u32::from(value);
        bits += 5;

        while bits >= 8 {
            bits -= 8;
            output.push(((accumulator >> bits) & 0xff) as u8);
        }

        if bits == 0 {
            accumulator = 0;
        } else {
            accumulator &= (1u32 << bits) - 1;
        }
    }

    Ok(output)
}