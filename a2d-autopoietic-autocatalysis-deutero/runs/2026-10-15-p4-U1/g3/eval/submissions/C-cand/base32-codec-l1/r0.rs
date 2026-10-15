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
    let mut bits = 0usize;

    for &byte in input {
        accumulator = (accumulator << 8) | byte as u32;
        bits += 8;

        while bits >= 5 {
            bits -= 5;
            output.push(ALPHABET[((accumulator >> bits) & 31) as usize] as char);
        }
    }

    if bits != 0 {
        let value = (accumulator << (5 - bits)) & 31;
        output.push(ALPHABET[value as usize] as char);
    }

    while output.len() % 8 != 0 {
        output.push('=');
    }

    output
}

pub fn decode(input: &str) -> Result<Vec<u8>, DecodeError> {
    let bytes = input.as_bytes();
    let n = bytes.len();

    // Lexical validation.
    for (offset, &byte) in bytes.iter().enumerate() {
        if byte != b'=' && !ALPHABET.contains(&byte) {
            return Err(DecodeError::InvalidByte { offset, byte });
        }
    }

    let p = bytes.iter().position(|&byte| byte == b'=').unwrap_or(n);

    // Padding placement.
    if let Some(offset) = (p..n).find(|&offset| bytes[offset] != b'=') {
        return Err(DecodeError::Padding { offset });
    }

    let d = p;
    let residue = d % 8;

    // Data length.
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

    if n - p != required_padding || n % 8 != 0 {
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

    if unused_bits != 0 {
        let final_value = ALPHABET
            .iter()
            .position(|&symbol| symbol == bytes[d - 1])
            .unwrap() as u8;
        if final_value & ((1 << unused_bits) - 1) != 0 {
            return Err(DecodeError::TrailingBits { offset: d - 1 });
        }
    }

    let mut output = Vec::with_capacity(d.saturating_mul(5) / 8);
    let mut accumulator: u32 = 0;
    let mut bits = 0usize;

    for &symbol in &bytes[..d] {
        let value = ALPHABET
            .iter()
            .position(|&candidate| candidate == symbol)
            .unwrap() as u32;
        accumulator = (accumulator << 5) | value;
        bits += 5;

        if bits >= 8 {
            bits -= 8;
            output.push(((accumulator >> bits) & 0xff) as u8);
        }
    }

    Ok(output)
}

fn main() {}