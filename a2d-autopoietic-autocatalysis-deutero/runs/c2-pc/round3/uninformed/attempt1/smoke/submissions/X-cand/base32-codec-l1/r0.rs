#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    InvalidByte { offset: usize, byte: u8 },
    Padding { offset: usize },
    InvalidLength { offset: usize },
    TrailingBits { offset: usize },
}

const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

pub fn encode(input: &[u8]) -> String {
    let mut output = String::new();
    let mut accumulator: u32 = 0;
    let mut bits = 0;

    for &byte in input {
        accumulator = (accumulator << 8) | u32::from(byte);
        bits += 8;

        while bits >= 5 {
            bits -= 5;
            output.push(ALPHABET[((accumulator >> bits) & 0x1f) as usize] as char);
        }
    }

    if bits > 0 {
        let value = (accumulator << (5 - bits)) & 0x1f;
        output.push(ALPHABET[value as usize] as char);
    }

    while output.len() % 8 != 0 {
        output.push('=');
    }

    output
}

fn alphabet_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'2'..=b'7' => Some(byte - b'2' + 26),
        _ => None,
    }
}

pub fn decode(input: &str) -> Result<Vec<u8>, DecodeError> {
    let bytes = input.as_bytes();
    let n = bytes.len();

    // Lexical validation.
    for (offset, &byte) in bytes.iter().enumerate() {
        if alphabet_value(byte).is_none() && byte != b'=' {
            return Err(DecodeError::InvalidByte { offset, byte });
        }
    }

    // Padding placement.
    let p = bytes.iter().position(|&byte| byte == b'=').unwrap_or(n);
    if let Some(offset) = bytes[p..]
        .iter()
        .position(|&byte| alphabet_value(byte).is_some())
    {
        return Err(DecodeError::Padding { offset: p + offset });
    }

    // Data length.
    let residue = p % 8;
    if !matches!(residue, 0 | 2 | 4 | 5 | 7) {
        return Err(DecodeError::InvalidLength { offset: n });
    }

    // Padding shape.
    let required_padding = match residue {
        0 => 0,
        2 => 6,
        4 => 4,
        5 => 3,
        7 => 1,
        _ => unreachable!(),
    };
    if n % 8 != 0 || n - p != required_padding {
        return Err(DecodeError::Padding { offset: p });
    }

    // Trailing bits.
    let unused_bits = match residue {
        0 => 0,
        2 => 2,
        4 => 4,
        5 => 1,
        7 => 3,
        _ => unreachable!(),
    };
    if unused_bits > 0 {
        let value = alphabet_value(bytes[p - 1]).expect("data byte was lexically validated");
        let mask = (1u8 << unused_bits) - 1;
        if value & mask != 0 {
            return Err(DecodeError::TrailingBits { offset: p - 1 });
        }
    }

    let mut output = Vec::with_capacity(p * 5 / 8);
    let mut accumulator: u32 = 0;
    let mut bits = 0;

    for &byte in &bytes[..p] {
        accumulator = (accumulator << 5) | u32::from(alphabet_value(byte).unwrap());
        bits += 5;

        while bits >= 8 {
            bits -= 8;
            output.push(((accumulator >> bits) & 0xff) as u8);
        }
    }

    Ok(output)
}