#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alphabet {
    Standard,
    Hex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Padding {
    Required,
    Omitted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    InvalidByte { offset: usize, byte: u8 },
    Padding { offset: usize },
    InvalidLength { offset: usize },
    TrailingBits { offset: usize },
}

const STANDARD: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
const HEX: &[u8; 32] = b"0123456789ABCDEFGHIJKLMNOPQRSTUV";

fn alphabet_table(alphabet: Alphabet) -> &'static [u8; 32] {
    match alphabet {
        Alphabet::Standard => STANDARD,
        Alphabet::Hex => HEX,
    }
}

fn value_of(byte: u8, alphabet: Alphabet) -> Option<u8> {
    alphabet_table(alphabet)
        .iter()
        .position(|&candidate| candidate == byte)
        .map(|value| value as u8)
}

pub fn encode(input: &[u8], alphabet: Alphabet, padding: Padding) -> String {
    let table = alphabet_table(alphabet);
    let symbol_count = input.len().saturating_mul(8).saturating_add(4) / 5;
    let padded_len = if padding == Padding::Required {
        symbol_count.saturating_add(7) / 8 * 8
    } else {
        symbol_count
    };
    let mut output = String::with_capacity(padded_len);

    let mut accumulator: u32 = 0;
    let mut bits = 0u8;

    for &byte in input {
        accumulator = (accumulator << 8) | u32::from(byte);
        bits += 8;

        while bits >= 5 {
            bits -= 5;
            let index = ((accumulator >> bits) & 0x1f) as usize;
            output.push(table[index] as char);
        }
    }

    if bits != 0 {
        let index = ((accumulator << (5 - bits)) & 0x1f) as usize;
        output.push(table[index] as char);
    }

    if padding == Padding::Required {
        while output.len() % 8 != 0 {
            output.push('=');
        }
    }

    output
}

pub fn decode(
    input: &str,
    alphabet: Alphabet,
    padding: Padding,
) -> Result<Vec<u8>, DecodeError> {
    let bytes = input.as_bytes();
    let n = bytes.len();

    // Pass 1: validate all lexical bytes.
    for (offset, &byte) in bytes.iter().enumerate() {
        if byte != b'=' && value_of(byte, alphabet).is_none() {
            return Err(DecodeError::InvalidByte { offset, byte });
        }
    }

    let p = bytes.iter().position(|&byte| byte == b'=').unwrap_or(n);

    // Pass 2: validate padding mode and placement.
    match padding {
        Padding::Omitted if p < n => {
            return Err(DecodeError::Padding { offset: p });
        }
        Padding::Required if p < n => {
            if let Some(offset) = bytes[p..]
                .iter()
                .position(|&byte| byte != b'=')
                .map(|relative| p + relative)
            {
                return Err(DecodeError::Padding { offset });
            }
        }
        _ => {}
    }

    let d = p;
    let residue = d % 8;

    // Pass 3: validate data length.
    if !matches!(residue, 0 | 2 | 4 | 5 | 7) {
        return Err(DecodeError::InvalidLength { offset: n });
    }

    // Pass 4: validate required padding shape.
    if padding == Padding::Required {
        let expected_padding = match residue {
            0 => 0,
            2 => 6,
            4 => 4,
            5 => 3,
            7 => 1,
            _ => unreachable!(),
        };

        if n - p != expected_padding || n % 8 != 0 {
            return Err(DecodeError::Padding { offset: p });
        }
    }

    // Pass 5: validate unused low bits in the final symbol.
    let unused_bits = match residue {
        0 => 0,
        2 => 2,
        4 => 4,
        5 => 1,
        7 => 3,
        _ => unreachable!(),
    };

    if unused_bits != 0 {
        let last_value = value_of(bytes[d - 1], alphabet).unwrap();
        let mask = (1u8 << unused_bits) - 1;
        if last_value & mask != 0 {
            return Err(DecodeError::TrailingBits { offset: d - 1 });
        }
    }

    let mut output = Vec::with_capacity(d.saturating_mul(5) / 8);
    let mut accumulator: u32 = 0;
    let mut bits = 0u8;

    for &byte in &bytes[..d] {
        let value = value_of(byte, alphabet).unwrap();
        accumulator = (accumulator << 5) | u32::from(value);
        bits += 5;

        if bits >= 8 {
            bits -= 8;
            output.push(((accumulator >> bits) & 0xff) as u8);
        }
    }

    Ok(output)
}