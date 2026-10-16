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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    pub alphabet: Alphabet,
    pub padding: Padding,
    pub canonical: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamError {
    InvalidByte { offset: usize, byte: u8 },
    NonCanonical { offset: usize, expected: u8 },
    Padding { offset: usize },
    InvalidLength { offset: usize },
    TrailingBits { offset: usize },
    Closed,
}

pub struct Encoder {
    config: Config,
    pending: Vec<u8>,
    closed: bool,
}

impl Encoder {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            pending: Vec::new(),
            closed: false,
        }
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }

        let mut data = Vec::with_capacity(self.pending.len() + input.len());
        data.extend_from_slice(&self.pending);
        data.extend_from_slice(input);

        let complete_len = (data.len() / 5) * 5;
        let output = encode_bytes(&data[..complete_len], self.config.alphabet);
        self.pending.clear();
        self.pending.extend_from_slice(&data[complete_len..]);
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<String, StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }

        let mut output = String::new();
        if !self.pending.is_empty() {
            let alphabet = alphabet_bytes(self.config.alphabet);
            let bit_len = self.pending.len() * 8;
            let symbol_count = (bit_len + 4) / 5;
            for symbol_index in 0..symbol_count {
                let mut value = 0u8;
                for bit_index in 0..5 {
                    let source_bit = symbol_index * 5 + bit_index;
                    value <<= 1;
                    if source_bit < bit_len {
                        let byte = self.pending[source_bit / 8];
                        value |= (byte >> (7 - source_bit % 8)) & 1;
                    }
                }
                output.push(alphabet[value as usize] as char);
            }

            if self.config.padding == Padding::Required {
                while output.len() % 8 != 0 {
                    output.push('=');
                }
            }
        }

        self.pending.clear();
        self.closed = true;
        Ok(output)
    }
}

pub struct Decoder {
    config: Config,
    position: usize,
    data_count: usize,
    bit_buffer: u32,
    bit_count: u8,
    last_data_offset: Option<usize>,
    first_padding_offset: Option<usize>,
    padding_count: usize,
    expected_padding: Option<usize>,
    closed: bool,
}

impl Decoder {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            position: 0,
            data_count: 0,
            bit_buffer: 0,
            bit_count: 0,
            last_data_offset: None,
            first_padding_offset: None,
            padding_count: 0,
            expected_padding: None,
            closed: false,
        }
    }

    pub fn push(&mut self, input: &str) -> Result<Vec<u8>, StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }

        let mut next = self.clone_state();
        let mut output = Vec::new();

        for &byte in input.as_bytes() {
            let offset = next.position;
            let is_padding = byte == b'=';
            let (value, canonical_byte) = if is_padding {
                (None, None)
            } else {
                match decode_symbol(byte, next.config.alphabet) {
                    Some((value, uppercase)) => (Some(value), Some(uppercase)),
                    None => {
                        return Err(StreamError::InvalidByte { offset, byte });
                    }
                }
            };

            if let Some(expected) = canonical_byte {
                if next.config.canonical && byte.is_ascii_lowercase() {
                    return Err(StreamError::NonCanonical { offset, expected });
                }
            }

            if is_padding {
                if next.config.padding == Padding::Omitted {
                    return Err(StreamError::Padding { offset });
                }

                if next.first_padding_offset.is_none() {
                    let residue = next.data_count % 8;
                    let expected = match residue {
                        2 => 6,
                        4 => 4,
                        5 => 3,
                        7 => 1,
                        _ => return Err(StreamError::Padding { offset }),
                    };
                    next.first_padding_offset = Some(offset);
                    next.expected_padding = Some(expected);
                }

                let expected = next.expected_padding.unwrap_or(0);
                if next.padding_count >= expected {
                    return Err(StreamError::Padding { offset });
                }
                next.padding_count += 1;
            } else {
                if next.first_padding_offset.is_some() {
                    return Err(StreamError::Padding { offset });
                }

                let value = value.unwrap();
                next.data_count += 1;
                next.last_data_offset = Some(offset);
                next.bit_buffer = (next.bit_buffer << 5) | value as u32;
                next.bit_count += 5;

                while next.bit_count >= 8 {
                    next.bit_count -= 8;
                    output.push((next.bit_buffer >> next.bit_count) as u8);
                    if next.bit_count == 0 {
                        next.bit_buffer = 0;
                    } else {
                        next.bit_buffer &= (1u32 << next.bit_count) - 1;
                    }
                }
            }

            next.position += 1;
        }

        *self = next;
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }

        let residue = self.data_count % 8;
        let expected_padding = match residue {
            0 => 0,
            2 => 6,
            4 => 4,
            5 => 3,
            7 => 1,
            _ => {
                return Err(StreamError::InvalidLength {
                    offset: self.position,
                });
            }
        };

        if self.config.padding == Padding::Required
            && self.padding_count != expected_padding
        {
            return Err(StreamError::Padding {
                offset: self.first_padding_offset.unwrap_or(self.position),
            });
        }

        let unused_bits = match residue {
            0 => 0,
            2 => 2,
            4 => 4,
            5 => 1,
            7 => 3,
            _ => 0,
        };

        if unused_bits != 0 && (self.bit_buffer & ((1u32 << unused_bits) - 1)) != 0 {
            return Err(StreamError::TrailingBits {
                offset: self.last_data_offset.unwrap_or(self.position),
            });
        }

        self.closed = true;
        Ok(Vec::new())
    }
}

impl Decoder {
    fn clone_state(&self) -> Self {
        Self {
            config: self.config,
            position: self.position,
            data_count: self.data_count,
            bit_buffer: self.bit_buffer,
            bit_count: self.bit_count,
            last_data_offset: self.last_data_offset,
            first_padding_offset: self.first_padding_offset,
            padding_count: self.padding_count,
            expected_padding: self.expected_padding,
            closed: self.closed,
        }
    }
}

fn alphabet_bytes(alphabet: Alphabet) -> &'static [u8; 32] {
    match alphabet {
        Alphabet::Standard => b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567",
        Alphabet::Hex => b"0123456789ABCDEFGHIJKLMNOPQRSTUV",
    }
}

fn encode_bytes(input: &[u8], alphabet: Alphabet) -> String {
    let symbols = alphabet_bytes(alphabet);
    let mut output = String::with_capacity(input.len() * 8 / 5);
    let mut buffer = 0u32;
    let mut bits = 0u8;

    for &byte in input {
        buffer = (buffer << 8) | byte as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            let value = ((buffer >> bits) & 0x1f) as usize;
            output.push(symbols[value] as char);
            if bits == 0 {
                buffer = 0;
            } else {
                buffer &= (1u32 << bits) - 1;
            }
        }
    }

    output
}

fn decode_symbol(byte: u8, alphabet: Alphabet) -> Option<(u8, u8)> {
    let uppercase = byte.to_ascii_uppercase();
    let symbols = alphabet_bytes(alphabet);
    symbols
        .iter()
        .position(|&symbol| symbol == uppercase)
        .map(|value| (value as u8, uppercase))
}