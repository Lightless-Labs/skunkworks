#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    InvalidByte { offset: usize, byte: u8 },
    Padding { offset: usize },
    InvalidLength { offset: usize },
    TrailingBits { offset: usize },
}

const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

pub fn encode(input: &[u8]) -> String {
    let mut out = String::new();
    let mut acc: u32 = 0;
    let mut bits: u32 = 0;

    for &byte in input {
        acc = (acc << 8) | u32::from(byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((acc >> bits) & 31) as usize] as char);
        }
        if bits == 0 {
            acc = 0;
        } else {
            acc &= (1u32 << bits) - 1;
        }
    }

    if bits != 0 {
        let value = (acc << (5 - bits)) & 31;
        out.push(ALPHABET[value as usize] as char);
    }

    let padding = (8 - (out.len() % 8)) % 8;
    for _ in 0..padding {
        out.push('=');
    }
    out
}

fn value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'2'..=b'7' => Some(byte - b'2' + 26),
        _ => None,
    }
}

pub fn decode(input: &str) -> Result<Vec<u8>, DecodeError> {
    let bytes = input.as_bytes();
    let n = bytes.len();

    // Pass 1: lexical validation across the entire input.
    for (offset, &byte) in bytes.iter().enumerate() {
        if byte != b'=' && value(byte).is_none() {
            return Err(DecodeError::InvalidByte { offset, byte });
        }
    }

    // Pass 2: find the first padding byte and reject data after it.
    let p = bytes.iter().position(|&b| b == b'=').unwrap_or(n);
    for (offset, &byte) in bytes.iter().enumerate().skip(p) {
        if byte != b'=' {
            return Err(DecodeError::Padding { offset });
        }
    }

    // Pass 3: validate the number of data symbols.
    let residue = p % 8;
    let required_padding = match residue {
        0 => 0,
        2 => 6,
        4 => 4,
        5 => 3,
        7 => 1,
        _ => return Err(DecodeError::InvalidLength { offset: n }),
    };

    // Pass 4: validate exact padding count and total block length.
    if n - p != required_padding || n % 8 != 0 {
        return Err(DecodeError::Padding { offset: p });
    }

    // Pass 5: unused low bits in the final symbol must be zero.
    let unused_bits = match residue {
        0 => 0,
        2 => 2,
        4 => 4,
        5 => 1,
        7 => 3,
        _ => unreachable!(),
    };
    if unused_bits != 0 {
        let last = value(bytes[p - 1]).expect("lexical validation accepted data symbol");
        if last & ((1u8 << unused_bits) - 1) != 0 {
            return Err(DecodeError::TrailingBits { offset: p - 1 });
        }
    }

    let mut output = Vec::with_capacity(p.saturating_mul(5) / 8);
    let mut acc: u32 = 0;
    let mut bits: u32 = 0;

    for &byte in &bytes[..p] {
        acc = (acc << 5) | u32::from(value(byte).expect("validated alphabet symbol"));
        bits += 5;
        while bits >= 8 {
            bits -= 8;
            output.push(((acc >> bits) & 0xff) as u8);
        }
        if bits == 0 {
            acc = 0;
        } else {
            acc &= (1u32 << bits) - 1;
        }
    }

    Ok(output)
}
