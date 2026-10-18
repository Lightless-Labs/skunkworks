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

fn symbol_value(byte: u8, alphabet: Alphabet) -> Option<u8> {
    alphabet_bytes(alphabet)
        .iter()
        .position(|&candidate| candidate == byte)
        .map(|value| value as u8)
}

pub fn encode(input: &[u8], alphabet: Alphabet, padding: Padding) -> String {
    let symbols = alphabet_bytes(alphabet);
    let mut output = String::with_capacity(input.len().saturating_mul(8) / 5 + 8);
    let mut accumulator = 0u32;
    let mut bit_count = 0usize;

    for &byte in input {
        accumulator = (accumulator << 8) | u32::from(byte);
        bit_count += 8;

        while bit_count >= 5 {
            bit_count -= 5;
            output.push(symbols[((accumulator >> bit_count) & 0x1f) as usize] as char);
        }

        if bit_count == 0 {
            accumulator = 0;
        } else {
            accumulator &= (1u32 << bit_count) - 1;
        }
    }

    if bit_count != 0 {
        output.push(symbols[((accumulator << (5 - bit_count)) & 0x1f) as usize] as char);
    }

    if padding == Padding::Required && !output.is_empty() {
        let remainder = output.len() % 8;
        if remainder != 0 {
            output.extend(std::iter::repeat_n('=', 8 - remainder));
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

    // Pass 1: validate every original byte lexically.
    for (offset, &byte) in bytes.iter().enumerate() {
        if byte != b'=' && symbol_value(byte, alphabet).is_none() {
            return Err(DecodeError::InvalidByte { offset, byte });
        }
    }

    let p = bytes.iter().position(|&byte| byte == b'=').unwrap_or(n);

    // Pass 2: validate padding placement and mode.
    match padding {
        Padding::Omitted if p < n => return Err(DecodeError::Padding { offset: p }),
        Padding::Required if p < n => {
            if let Some(offset) = (p + 1..n).find(|&i| bytes[i] != b'=') {
                return Err(DecodeError::Padding { offset });
            }
        }
        _ => {}
    }

    // Pass 3: validate data length.
    let residue = p % 8;
    if !matches!(residue, 0 | 2 | 4 | 5 | 7) {
        return Err(DecodeError::InvalidLength { offset: n });
    }

    // Pass 4: validate Required padding shape.
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
        let last_value = symbol_value(bytes[p - 1], alphabet).unwrap();
        let mask = (1u8 << unused_bits) - 1;
        if last_value & mask != 0 {
            return Err(DecodeError::TrailingBits { offset: p - 1 });
        }
    }

    let mut output = Vec::with_capacity(p.saturating_mul(5) / 8);
    let mut accumulator = 0u32;
    let mut bit_count = 0usize;

    for &byte in &bytes[..p] {
        accumulator = (accumulator << 5) | u32::from(symbol_value(byte, alphabet).unwrap());
        bit_count += 5;

        if bit_count >= 8 {
            bit_count -= 8;
            output.push(((accumulator >> bit_count) & 0xff) as u8);
            if bit_count == 0 {
                accumulator = 0;
            } else {
                accumulator &= (1u32 << bit_count) - 1;
            }
        }
    }

    Ok(output)
}