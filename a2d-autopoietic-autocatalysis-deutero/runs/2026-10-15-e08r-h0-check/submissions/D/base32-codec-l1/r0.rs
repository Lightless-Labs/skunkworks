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
    let mut buffer: u32 = 0;
    let mut bits = 0usize;

    for &byte in input {
        buffer = (buffer << 8) | u32::from(byte);
        bits += 8;

        while bits >= 5 {
            bits -= 5;
            output.push(ALPHABET[((buffer >> bits) & 0x1f) as usize] as char);
        }
    }

    if bits > 0 {
        output.push(ALPHABET[((buffer << (5 - bits)) & 0x1f) as usize] as char);
    }

    while output.len() % 8 != 0 {
        output.push('=');
    }

    output
}

pub fn decode(input: &str) -> Result<Vec<u8>, DecodeError> {
    let bytes = input.as_bytes();
    let n = bytes.len();

    // Lexical validation
    for (offset, &byte) in bytes.iter().enumerate() {
        if byte != b'=' && !ALPHABET.contains(&byte) {
            return Err(DecodeError::InvalidByte { offset, byte });
        }
    }

    // Padding placement
    let p = bytes.iter().position(|&byte| byte == b'=').unwrap_or(n);
    if let Some(offset) = bytes[p..]
        .iter()
        .position(|&byte| byte != b'=')
        .map(|offset| p + offset)
    {
        return Err(DecodeError::Padding { offset });
    }

    // Data length
    let residue = p % 8;
    if !matches!(residue, 0 | 2 | 4 | 5 | 7) {
        return Err(DecodeError::InvalidLength { offset: n });
    }

    // Padding shape
    let required_padding = match residue {
        0 => 0,
        2 => 6,
        4 => 4,
        5 => 3,
        7 => 1,
        _ => unreachable!(),
    };
    if n - p != required_padding || n % 8 != 0 {
        return Err(DecodeError::Padding { offset: p });
    }

    // Trailing bits
    let unused_bits = match residue {
        0 => 0,
        2 => 2,
        4 => 4,
        5 => 1,
        7 => 3,
        _ => unreachable!(),
    };
    if unused_bits > 0 {
        let value = ALPHABET
            .iter()
            .position(|&symbol| symbol == bytes[p - 1])
            .unwrap() as u8;
        if value & ((1 << unused_bits) - 1) != 0 {
            return Err(DecodeError::TrailingBits { offset: p - 1 });
        }
    }

    // Decode the validated data symbols into bytes.
    let mut output = Vec::with_capacity(p * 5 / 8);
    let mut buffer: u32 = 0;
    let mut bits = 0usize;

    for &symbol in &bytes[..p] {
        let value = ALPHABET
            .iter()
            .position(|&candidate| candidate == symbol)
            .unwrap() as u32;
        buffer = (buffer << 5) | value;
        bits += 5;

        while bits >= 8 {
            bits -= 8;
            output.push(((buffer >> bits) & 0xff) as u8);
        }
    }

    Ok(output)
}