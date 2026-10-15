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
    let mut buffer: u32 = 0;
    let mut bits = 0u32;

    for &byte in input {
        buffer = (buffer << 8) | u32::from(byte);
        bits += 8;

        while bits >= 5 {
            bits -= 5;
            let value = ((buffer >> bits) & 0x1f) as usize;
            output.push(char::from(symbols[value]));
        }
    }

    if bits > 0 {
        let value = ((buffer << (5 - bits)) & 0x1f) as usize;
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
    let symbols = alphabet_bytes(alphabet);

    // Lexical validation is complete before checking padding placement.
    for (offset, &byte) in bytes.iter().enumerate() {
        if byte != b'=' && !symbols.contains(&byte) {
            return Err(DecodeError::InvalidByte { offset, byte });
        }
    }

    let n = bytes.len();
    let p = bytes.iter().position(|&byte| byte == b'=').unwrap_or(n);

    if padding == Padding::Omitted {
        if p < n {
            return Err(DecodeError::Padding { offset: p });
        }
    } else if p < n {
        if let Some(relative) = bytes[p..].iter().position(|&byte| byte != b'=') {
            return Err(DecodeError::Padding { offset: p + relative });
        }
    }

    let data_len = p;
    let residue = data_len % 8;
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

    let trailing_bits = match residue {
        0 => 0,
        2 => 2,
        4 => 4,
        5 => 1,
        7 => 3,
        _ => unreachable!(),
    };

    if trailing_bits > 0 {
        let last_value = symbols
            .iter()
            .position(|&symbol| symbol == bytes[data_len - 1])
            .expect("lexical validation guarantees every data byte is in the alphabet");
        let unused_mask = (1usize << trailing_bits) - 1;
        if last_value & unused_mask != 0 {
            return Err(DecodeError::TrailingBits {
                offset: data_len - 1,
            });
        }
    }

    let mut output = Vec::with_capacity(data_len.saturating_mul(5) / 8);
    let mut buffer: u32 = 0;
    let mut bits = 0u32;

    for &byte in &bytes[..data_len] {
        let value = symbols
            .iter()
            .position(|&symbol| symbol == byte)
            .expect("lexical validation guarantees every data byte is in the alphabet")
            as u32;
        buffer = (buffer << 5) | value;
        bits += 5;

        if bits >= 8 {
            bits -= 8;
            output.push(((buffer >> bits) & 0xff) as u8);
        }
    }

    Ok(output)
}

fn main() {}