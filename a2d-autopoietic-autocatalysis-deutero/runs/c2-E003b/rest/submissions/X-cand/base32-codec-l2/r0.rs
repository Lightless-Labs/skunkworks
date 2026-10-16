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

pub fn encode(input: &[u8], alphabet: Alphabet, padding: Padding) -> String {
    let symbols = alphabet_bytes(alphabet);
    let mut output = String::new();
    let mut accumulator = 0u32;
    let mut bits = 0u8;

    for &byte in input {
        accumulator = (accumulator << 8) | u32::from(byte);
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
    let symbols = alphabet_bytes(alphabet);

    // Pass 1: validate every original byte lexically.
    for (offset, &byte) in bytes.iter().enumerate() {
        if byte != b'=' && !symbols.contains(&byte) {
            return Err(DecodeError::InvalidByte { offset, byte });
        }
    }

    let n = bytes.len();
    let p = bytes.iter().position(|&byte| byte == b'=').unwrap_or(n);

    // Pass 2: validate padding placement by mode.
    match padding {
        Padding::Omitted if p < n => {
            return Err(DecodeError::Padding { offset: p });
        }
        Padding::Required => {
            if let Some(offset) = (p..n).find(|&offset| bytes[offset] != b'=') {
                return Err(DecodeError::Padding { offset });
            }
        }
        Padding::Omitted => {}
    }

    let data_len = p;
    let residue = data_len % 8;

    // Pass 3: validate data length.
    if !matches!(residue, 0 | 2 | 4 | 5 | 7) {
        return Err(DecodeError::InvalidLength { offset: n });
    }

    // Pass 4: validate required padding shape.
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

    // Pass 5: validate unused low bits.
    let unused_bits = match residue {
        0 => 0,
        2 => 2,
        4 => 4,
        5 => 1,
        7 => 3,
        _ => unreachable!(),
    };
    if unused_bits > 0 {
        let last_value = symbols
            .iter()
            .position(|&symbol| symbol == bytes[data_len - 1])
            .expect("lexical validation guarantees alphabet symbols");
        let mask = (1usize << unused_bits) - 1;
        if last_value & mask != 0 {
            return Err(DecodeError::TrailingBits {
                offset: data_len - 1,
            });
        }
    }

    // Decode the validated data symbols, emitting complete bytes.
    let mut output = Vec::with_capacity(data_len * 5 / 8);
    let mut accumulator = 0u32;
    let mut bits = 0u8;

    for &byte in &bytes[..data_len] {
        let value = symbols
            .iter()
            .position(|&symbol| symbol == byte)
            .expect("lexical validation guarantees alphabet symbols") as u32;
        accumulator = (accumulator << 5) | value;
        bits += 5;

        if bits >= 8 {
            bits -= 8;
            output.push(((accumulator >> bits) & 0xff) as u8);
        }
    }

    Ok(output)
}