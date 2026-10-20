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
    let mut accumulator: u16 = 0;
    let mut bits = 0usize;

    for &byte in input {
        accumulator = (accumulator << 8) | u16::from(byte);
        bits += 8;

        while bits >= 5 {
            bits -= 5;
            let value = ((accumulator >> bits) & 0x1f) as usize;
            output.push(symbols[value] as char);
        }

        accumulator &= (1u16 << bits) - 1;
    }

    if bits != 0 {
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

    // Lexical validation covers the complete input before any other checks.
    for (offset, &byte) in bytes.iter().enumerate() {
        if byte != b'=' && !symbols.contains(&byte) {
            return Err(DecodeError::InvalidByte { offset, byte });
        }
    }

    let n = bytes.len();
    let p = bytes.iter().position(|&byte| byte == b'=').unwrap_or(n);

    match padding {
        Padding::Omitted if p != n => {
            return Err(DecodeError::Padding { offset: p });
        }
        Padding::Required => {
            if let Some(offset) = bytes[p..].iter().position(|&byte| byte != b'=') {
                return Err(DecodeError::Padding { offset: p + offset });
            }
        }
        Padding::Omitted => {}
    }

    let data_len = p;
    let residue = data_len % 8;
    let required_padding = match residue {
        0 => 0,
        2 => 6,
        4 => 4,
        5 => 3,
        7 => 1,
        _ => return Err(DecodeError::InvalidLength { offset: n }),
    };

    if padding == Padding::Required
        && (n % 8 != 0 || n - p != required_padding)
    {
        return Err(DecodeError::Padding { offset: p });
    }

    if residue != 0 {
        let value = symbols
            .iter()
            .position(|&symbol| symbol == bytes[data_len - 1])
            .expect("lexically validated alphabet byte");

        let unused_bits = match residue {
            2 => 2,
            4 => 4,
            5 => 1,
            7 => 3,
            _ => 0,
        };

        if value & ((1usize << unused_bits) - 1) != 0 {
            return Err(DecodeError::TrailingBits {
                offset: data_len - 1,
            });
        }
    }

    let mut output = Vec::with_capacity(data_len * 5 / 8);
    let mut accumulator: u16 = 0;
    let mut bits = 0usize;

    for &byte in &bytes[..data_len] {
        let value = symbols
            .iter()
            .position(|&symbol| symbol == byte)
            .expect("lexically validated alphabet byte") as u16;

        accumulator = (accumulator << 5) | value;
        bits += 5;

        while bits >= 8 {
            bits -= 8;
            output.push(((accumulator >> bits) & 0xff) as u8);
        }

        accumulator &= (1u16 << bits) - 1;
    }

    Ok(output)
}