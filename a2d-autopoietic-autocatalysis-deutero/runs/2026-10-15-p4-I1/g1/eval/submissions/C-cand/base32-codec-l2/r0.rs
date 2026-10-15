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
        .position(|&candidate| candidate == byte)
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

    // Validate every byte before considering padding placement or length.
    for (offset, &byte) in bytes.iter().enumerate() {
        if byte != b'=' && value_of(byte, alphabet).is_none() {
            return Err(DecodeError::InvalidByte { offset, byte });
        }
    }

    let p = bytes.iter().position(|&byte| byte == b'=').unwrap_or(n);

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

    let d = p;
    let residue = d % 8;
    let expected_padding = match residue {
        0 => 0,
        2 => 6,
        4 => 4,
        5 => 3,
        7 => 1,
        _ => return Err(DecodeError::InvalidLength { offset: n }),
    };

    if padding == Padding::Required
        && (n % 8 != 0 || n - p != expected_padding)
    {
        return Err(DecodeError::Padding { offset: p });
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
        let last_value = value_of(bytes[d - 1], alphabet).unwrap();
        let mask = (1u8 << unused_bits) - 1;
        if last_value & mask != 0 {
            return Err(DecodeError::TrailingBits { offset: d - 1 });
        }
    }

    let mut output = Vec::new();
    let mut accumulator = 0u32;
    let mut bits = 0u8;

    for &byte in &bytes[..d] {
        let value = value_of(byte, alphabet).unwrap();
        accumulator = (accumulator << 5) | u32::from(value);
        bits += 5;

        while bits >= 8 {
            bits -= 8;
            output.push(((accumulator >> bits) & 0xff) as u8);
        }
    }

    Ok(output)
}

fn main() {}