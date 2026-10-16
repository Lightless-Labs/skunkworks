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

fn alphabet_bytes(alphabet: Alphabet) -> &'static [u8; 32] {
    match alphabet {
        Alphabet::Standard => b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567",
        Alphabet::Hex => b"0123456789ABCDEFGHIJKLMNOPQRSTUV",
    }
}

fn value_of(byte: u8, alphabet: &[u8; 32]) -> Option<u8> {
    alphabet.iter().position(|&symbol| symbol == byte).map(|v| v as u8)
}

pub fn encode(input: &[u8], alphabet: Alphabet, padding: Padding) -> String {
    let symbols = alphabet_bytes(alphabet);
    let mut output = String::new();
    let mut accumulator: u16 = 0;
    let mut bits = 0u8;

    for &byte in input {
        accumulator = (accumulator << 8) | byte as u16;
        bits += 8;

        while bits >= 5 {
            bits -= 5;
            let value = ((accumulator >> bits) & 0x1f) as usize;
            output.push(symbols[value] as char);
        }
    }

    if bits > 0 {
        let value = ((accumulator << (5 - bits)) & 0x1f) as usize;
        output.push(symbols[value] as char);
    }

    if padding == Padding::Required && !output.is_empty() {
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
    let symbols = alphabet_bytes(alphabet);

    // Pass 1: lexical validation over every original UTF-8 byte.
    for (offset, &byte) in bytes.iter().enumerate() {
        if byte != b'=' && value_of(byte, symbols).is_none() {
            return Err(DecodeError::InvalidByte { offset, byte });
        }
    }

    let n = bytes.len();
    let p = bytes.iter().position(|&byte| byte == b'=').unwrap_or(n);

    // Pass 2: padding placement for the selected mode.
    match padding {
        Padding::Omitted if p < n => return Err(DecodeError::Padding { offset: p }),
        Padding::Required => {
            if let Some(offset) = bytes[p..].iter().position(|&byte| byte != b'=') {
                return Err(DecodeError::Padding { offset: p + offset });
            }
        }
        Padding::Omitted => {}
    }

    let data_len = p;

    // Pass 3: data length.
    let residue = data_len % 8;
    if !matches!(residue, 0 | 2 | 4 | 5 | 7) {
        return Err(DecodeError::InvalidLength { offset: n });
    }

    // Pass 4: required padding shape.
    if padding == Padding::Required {
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
    }

    // Pass 5: unused low bits.
    let unused_bits = match residue {
        0 => 0,
        2 => 2,
        4 => 4,
        5 => 1,
        7 => 3,
        _ => unreachable!(),
    };
    if unused_bits > 0 {
        let last = value_of(bytes[data_len - 1], symbols).unwrap();
        if last & ((1u8 << unused_bits) - 1) != 0 {
            return Err(DecodeError::TrailingBits {
                offset: data_len - 1,
            });
        }
    }

    let mut output = Vec::with_capacity(data_len * 5 / 8);
    let mut accumulator: u32 = 0;
    let mut bits = 0u8;

    for &byte in &bytes[..data_len] {
        let value = value_of(byte, symbols).unwrap() as u32;
        accumulator = (accumulator << 5) | value;
        bits += 5;

        while bits >= 8 {
            bits -= 8;
            output.push(((accumulator >> bits) & 0xff) as u8);
        }
    }

    Ok(output)
}