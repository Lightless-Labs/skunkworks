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
    let alphabet = alphabet_bytes(alphabet);
    let symbol_count = input.len().saturating_mul(8).div_ceil(5);
    let mut output = String::with_capacity(symbol_count + 7);

    let mut accumulator = 0u32;
    let mut bits = 0usize;

    for &byte in input {
        accumulator = (accumulator << 8) | u32::from(byte);
        bits += 8;

        while bits >= 5 {
            bits -= 5;
            let value = ((accumulator >> bits) & 0x1f) as usize;
            output.push(char::from(alphabet[value]));
        }

        accumulator &= (1u32 << bits).wrapping_sub(1);
    }

    if bits > 0 {
        let value = ((accumulator << (5 - bits)) & 0x1f) as usize;
        output.push(char::from(alphabet[value]));
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
    let alphabet = alphabet_bytes(alphabet);

    for (offset, &byte) in bytes.iter().enumerate() {
        if byte != b'=' && !alphabet.contains(&byte) {
            return Err(DecodeError::InvalidByte { offset, byte });
        }
    }

    let n = bytes.len();
    let p = bytes.iter().position(|&byte| byte == b'=').unwrap_or(n);

    match padding {
        Padding::Omitted => {
            if p != n {
                return Err(DecodeError::Padding { offset: p });
            }
        }
        Padding::Required => {
            if let Some(offset) = (p..n).find(|&offset| bytes[offset] != b'=') {
                return Err(DecodeError::Padding { offset });
            }
        }
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

    if padding == Padding::Required {
        let actual_padding = n - p;
        if actual_padding != required_padding || n % 8 != 0 {
            return Err(DecodeError::Padding { offset: p });
        }
    }

    let unused_bits = match residue {
        0 => 0,
        2 => 2,
        4 => 4,
        5 => 1,
        7 => 3,
        _ => unreachable!(),
    };
    if unused_bits > 0 {
        let last_value = alphabet
            .iter()
            .position(|&byte| byte == bytes[data_len - 1])
            .expect("lexical validation checked the alphabet");
        if last_value & ((1usize << unused_bits) - 1) != 0 {
            return Err(DecodeError::TrailingBits {
                offset: data_len - 1,
            });
        }
    }

    let output_len = (data_len / 8) * 5 + (residue * 5 / 8);
    let mut output = Vec::with_capacity(output_len);
    let mut accumulator = 0u32;
    let mut bits = 0usize;

    for &byte in &bytes[..data_len] {
        let value = alphabet
            .iter()
            .position(|&symbol| symbol == byte)
            .expect("lexical validation checked the alphabet") as u32;
        accumulator = (accumulator << 5) | value;
        bits += 5;

        if bits >= 8 {
            bits -= 8;
            output.push(((accumulator >> bits) & 0xff) as u8);
        }

        accumulator &= (1u32 << bits).wrapping_sub(1);
    }

    Ok(output)
}