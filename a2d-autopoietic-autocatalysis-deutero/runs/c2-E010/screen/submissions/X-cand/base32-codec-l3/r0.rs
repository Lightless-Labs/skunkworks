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

fn digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'A'..=b'H' => Some(byte - b'A' + 10),
        b'J'..=b'K' => Some(byte - b'J' + 18),
        b'M'..=b'N' => Some(byte - b'M' + 20),
        b'P'..=b'T' => Some(byte - b'P' + 22),
        b'V'..=b'Z' => Some(byte - b'V' + 27),
        b'a'..=b'h' => Some(byte - b'a' + 10),
        b'j'..=b'k' => Some(byte - b'j' + 18),
        b'm'..=b'n' => Some(byte - b'm' + 20),
        b'p'..=b't' => Some(byte - b'p' + 22),
        b'v'..=b'z' => Some(byte - b'v' + 27),
        b'I' | b'i' | b'L' | b'l' => Some(1),
        b'O' | b'o' => Some(0),
        _ => None,
    }
}

fn checked_digit(byte: u8) -> Option<u8> {
    match byte {
        b'*' => Some(32),
        b'~' => Some(33),
        b'$' => Some(34),
        b'=' => Some(35),
        b'U' | b'u' => Some(36),
        _ => digit(byte),
    }
}

pub fn encode(input: &[u8], checked: bool) -> String {
    let mut output = String::with_capacity((input.len() * 8 + 4) / 5 + usize::from(checked));
    let mut buffer: u32 = 0;
    let mut bits = 0u32;
    let mut remainder = 0u8;

    for &byte in input {
        buffer = (buffer << 8) | byte as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            let value = ((buffer >> bits) & 31) as u8;
            output.push(DATA_ALPHABET[value as usize] as char);
            remainder = ((remainder as u16 * 32 + value as u16) % 37) as u8;
            buffer &= (1u32 << bits) - 1;
        }
    }
    if bits != 0 {
        let value = ((buffer << (5 - bits)) & 31) as u8;
        output.push(DATA_ALPHABET[value as usize] as char);
        remainder = ((remainder as u16 * 32 + value as u16) % 37) as u8;
    }
    if checked {
        output.push(CHECK_ALPHABET[remainder as usize] as char);
    }
    output
}

pub fn decode(input: &str, checked: bool) -> Result<Vec<u8>, DecodeError> {
    let bytes = input.as_bytes();
    let significant: Vec<(usize, u8)> = bytes.iter().copied().enumerate()
        .filter(|&(_, byte)| byte != b'-')
        .collect();

    if checked && significant.is_empty() {
        return Err(DecodeError::MissingCheck { offset: bytes.len() });
    }

    let data_len = significant.len() - usize::from(checked);
    let mut values = Vec::with_capacity(data_len);
    for &(offset, byte) in significant.iter().take(data_len) {
        let value = digit(byte).ok_or(DecodeError::InvalidByte { offset, byte })?;
        values.push((offset, value));
    }

    let check = if checked {
        let (offset, byte) = significant[data_len];
        Some((offset, checked_digit(byte)
            .ok_or(DecodeError::InvalidByte { offset, byte })?))
    } else {
        None
    };

    let rem = data_len % 8;
    if rem != 0 && rem != 2 && rem != 4 && rem != 5 && rem != 7 {
        return Err(DecodeError::InvalidLength { offset: bytes.len() });
    }

    if rem != 0 {
        let unused = match rem { 2 => 2, 4 => 4, 5 => 1, 7 => 3, _ => 0 };
        let (offset, value) = values[data_len - 1];
        if value & ((1u8 << unused) - 1) != 0 {
            return Err(DecodeError::TrailingBits { offset });
        }
    }

    let mut remainder = 0u8;
    for &(_, value) in &values {
        remainder = ((remainder as u16 * 32 + value as u16) % 37) as u8;
    }
    if let Some((offset, found)) = check {
        if found != remainder {
            return Err(DecodeError::CheckMismatch { offset, expected: remainder, found });
        }
    }

    let mut output = Vec::with_capacity(data_len * 5 / 8);
    let mut buffer: u32 = 0;
    let mut bits = 0u32;
    for &(_, value) in &values {
        buffer = (buffer << 5) | value as u32;
        bits += 5;
        while bits >= 8 {
            bits -= 8;
            output.push((buffer >> bits) as u8);
            buffer &= (1u32 << bits) - 1;
        }
    }
    Ok(output)
}
