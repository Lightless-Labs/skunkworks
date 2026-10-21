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

fn value(byte: u8, alphabet: Alphabet) -> Option<u8> {
    alphabet_bytes(alphabet)
        .iter()
        .position(|&candidate| candidate == byte)
        .map(|index| index as u8)
}

pub fn encode(input: &[u8], alphabet: Alphabet, padding: Padding) -> String {
    let symbols = alphabet_bytes(alphabet);
    let mut output = String::new();
    let mut accumulator = 0u16;
    let mut bits = 0u8;

    for &byte in input {
        accumulator = (accumulator << 8) | byte as u16;
        bits += 8;

        while bits >= 5 {
            bits -= 5;
            let index = ((accumulator >> bits) & 0x1f) as usize;
            output.push(symbols[index] as char);
        }

        accumulator &= (1u16 << bits) - 1;
    }

    if bits > 0 {
        let index = ((accumulator << (5 - bits)) & 0x1f) as usize;
        output.push(symbols[index] as char);
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
        if byte != b'=' && value(byte, alphabet).is_none() {
            return Err(DecodeError::InvalidByte { offset, byte });
        }
    }

    // Pass 2: validate padding placement and mode.
    let p = bytes.iter().position(|&byte| byte == b'=').unwrap_or(n);

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

        if n % 8 != 0 || n - p != expected_padding {
            return Err(DecodeError::Padding { offset: p });
        }
    }

    // Pass 5: validate unused trailing bits.
    let unused_bits = match residue {
        0 => 0,
        2 => 2,
        4 => 4,
        5 => 1,
        7 => 3,
        _ => unreachable!(),
    };

    if unused_bits > 0 {
        let last = value(bytes[d - 1], alphabet).unwrap();
        if last & ((1u8 << unused_bits) - 1) != 0 {
            return Err(DecodeError::TrailingBits { offset: d - 1 });
        }
    }

    let mut output = Vec::with_capacity(d * 5 / 8);
    let mut accumulator = 0u16;
    let mut bits = 0u8;

    for &byte in &bytes[..d] {
        accumulator = (accumulator << 5) | value(byte, alphabet).unwrap() as u16;
        bits += 5;

        while bits >= 8 {
            bits -= 8;
            output.push(((accumulator >> bits) & 0xff) as u8);
        }

        accumulator &= (1u16 << bits) - 1;
    }

    Ok(output)
}