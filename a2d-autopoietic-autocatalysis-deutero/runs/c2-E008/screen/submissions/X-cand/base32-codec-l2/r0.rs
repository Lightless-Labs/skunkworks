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

fn value_of(byte: u8, alphabet: Alphabet) -> Option<u8> {
    alphabet_bytes(alphabet)
        .iter()
        .position(|&symbol| symbol == byte)
        .map(|value| value as u8)
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
            output.push(char::from(symbols[value]));
        }

        accumulator &= (1u32 << bits).wrapping_sub(1);
    }

    if bits > 0 {
        let value = ((accumulator << (5 - bits)) & 0x1f) as usize;
        output.push(char::from(symbols[value]));
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

    // Validate all original UTF-8 bytes before checking padding or length.
    for (offset, &byte) in bytes.iter().enumerate() {
        if byte != b'=' && value_of(byte, alphabet).is_none() {
            return Err(DecodeError::InvalidByte { offset, byte });
        }
    }

    let p = bytes.iter().position(|&byte| byte == b'=').unwrap_or(n);

    // Validate padding mode and placement.
    match padding {
        Padding::Omitted if p < n => return Err(DecodeError::Padding { offset: p }),
        Padding::Required => {
            if let Some(relative) = bytes[p..].iter().position(|&byte| byte != b'=') {
                return Err(DecodeError::Padding {
                    offset: p + relative,
                });
            }
        }
        Padding::Omitted => {}
    }

    let data_len = p;
    let residue = data_len % 8;

    // Validate data length.
    if !matches!(residue, 0 | 2 | 4 | 5 | 7) {
        return Err(DecodeError::InvalidLength { offset: n });
    }

    // Validate required padding shape.
    if padding == Padding::Required {
        let expected_padding = match residue {
            0 => 0,
            2 => 6,
            4 => 4,
            5 => 3,
            7 => 1,
            _ => unreachable!(),
        };

        if n % 8 != 0 || n - data_len != expected_padding {
            return Err(DecodeError::Padding { offset: p });
        }
    }

    // Validate unused low bits in the final data symbol.
    let unused_bits = match residue {
        0 => 0,
        2 => 2,
        4 => 4,
        5 => 1,
        7 => 3,
        _ => unreachable!(),
    };

    if unused_bits > 0 {
        let last_value = value_of(bytes[data_len - 1], alphabet).unwrap();
        if last_value & ((1u8 << unused_bits) - 1) != 0 {
            return Err(DecodeError::TrailingBits {
                offset: data_len - 1,
            });
        }
    }

    let mut output = Vec::with_capacity(data_len * 5 / 8);
    let mut accumulator = 0u32;
    let mut bits = 0u8;

    for &byte in &bytes[..data_len] {
        let value = u32::from(value_of(byte, alphabet).unwrap());
        accumulator = (accumulator << 5) | value;
        bits += 5;

        while bits >= 8 {
            bits -= 8;
            output.push(((accumulator >> bits) & 0xff) as u8);
        }

        accumulator &= (1u32 << bits).wrapping_sub(1);
    }

    Ok(output)
}