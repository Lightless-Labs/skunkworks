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

        let mut bytes = Vec::with_capacity(self.pending.len() + input.len());
        bytes.extend_from_slice(&self.pending);
        bytes.extend_from_slice(input);

        let complete_len = bytes.len() / 5 * 5;
        let output = encode_symbols(&bytes[..complete_len], self.config.alphabet);

        self.pending.clear();
        self.pending.extend_from_slice(&bytes[complete_len..]);

        Ok(output)
    }

    pub fn finish(&mut self) -> Result<String, StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }

        let mut output = encode_symbols(&self.pending, self.config.alphabet);

        if self.config.padding == Padding::Required && !self.pending.is_empty() {
            while output.len() % 8 != 0 {
                output.push('=');
            }
        }

        self.pending.clear();
        self.closed = true;
        Ok(output)
    }
}

fn alphabet_bytes(alphabet: Alphabet) -> &'static [u8; 32] {
    match alphabet {
        Alphabet::Standard => b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567",
        Alphabet::Hex => b"0123456789ABCDEFGHIJKLMNOPQRSTUV",
    }
}

fn encode_symbols(bytes: &[u8], alphabet: Alphabet) -> String {
    let table = alphabet_bytes(alphabet);
    let symbol_count = (bytes.len() * 8 + 4) / 5;
    let mut output = String::with_capacity(symbol_count);

    for symbol_index in 0..symbol_count {
        let bit_start = symbol_index * 5;
        let mut value = 0u8;
        for bit in 0..5 {
            value <<= 1;
            let absolute_bit = bit_start + bit;
            if absolute_bit < bytes.len() * 8 {
                let byte = bytes[absolute_bit / 8];
                value |= (byte >> (7 - absolute_bit % 8)) & 1;
            }
        }
        output.push(table[value as usize] as char);
    }

    output
}

pub struct Decoder {
    config: Config,
    position: usize,
    data_count: usize,
    bit_buffer: u32,
    bit_count: u8,
    last_data_offset: Option<usize>,
    last_data_value: u8,
    first_padding_offset: Option<usize>,
    padding_count: usize,
    expected_padding: usize,
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
            last_data_value: 0,
            first_padding_offset: None,
            padding_count: 0,
            expected_padding: 0,
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
            let recognized = recognize(byte, next.config.alphabet);
            match recognized {
                Recognized::Invalid => {
                    return Err(StreamError::InvalidByte { offset, byte });
                }
                Recognized::Data { value, canonical } => {
                    if next.config.canonical && byte != canonical {
                        return Err(StreamError::NonCanonical {
                            offset,
                            expected: canonical,
                        });
                    }
                    if next.first_padding_offset.is_some() {
                        return Err(StreamError::Padding { offset });
                    }

                    next.data_count += 1;
                    next.last_data_offset = Some(offset);
                    next.last_data_value = value;
                    next.bit_buffer = (next.bit_buffer << 5) | value as u32;
                    next.bit_count += 5;

                    while next.bit_count >= 8 {
                        next.bit_count -= 8;
                        output.push(((next.bit_buffer >> next.bit_count) & 0xff) as u8);
                        if next.bit_count == 0 {
                            next.bit_buffer = 0;
                        } else {
                            next.bit_buffer &= (1u32 << next.bit_count) - 1;
                        }
                    }
                }
                Recognized::Padding => {
                    if next.config.padding == Padding::Omitted {
                        return Err(StreamError::Padding { offset });
                    }

                    if next.first_padding_offset.is_none() {
                        let residue = next.data_count % 8;
                        let expected = expected_padding(residue);
                        if residue == 0 || expected == 0 {
                            return Err(StreamError::Padding { offset });
                        }
                        next.first_padding_offset = Some(offset);
                        next.expected_padding = expected;
                    }

                    if next.padding_count >= next.expected_padding {
                        return Err(StreamError::Padding { offset });
                    }
                    next.padding_count += 1;
                }
            }

            next.position += 1;
        }

        self.copy_from(next);
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }

        let residue = self.data_count % 8;
        let expected = expected_padding(residue);

        if expected == usize::MAX {
            return Err(StreamError::InvalidLength {
                offset: self.position,
            });
        }

        if self.config.padding == Padding::Required && self.padding_count != expected {
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

        if unused_bits > 0 && (self.last_data_value & ((1 << unused_bits) - 1)) != 0 {
            return Err(StreamError::TrailingBits {
                offset: self.last_data_offset.unwrap_or(self.position),
            });
        }

        self.closed = true;
        Ok(Vec::new())
    }

    fn clone_state(&self) -> Self {
        Self {
            config: self.config,
            position: self.position,
            data_count: self.data_count,
            bit_buffer: self.bit_buffer,
            bit_count: self.bit_count,
            last_data_offset: self.last_data_offset,
            last_data_value: self.last_data_value,
            first_padding_offset: self.first_padding_offset,
            padding_count: self.padding_count,
            expected_padding: self.expected_padding,
            closed: self.closed,
        }
    }

    fn copy_from(&mut self, other: Self) {
        self.position = other.position;
        self.data_count = other.data_count;
        self.bit_buffer = other.bit_buffer;
        self.bit_count = other.bit_count;
        self.last_data_offset = other.last_data_offset;
        self.last_data_value = other.last_data_value;
        self.first_padding_offset = other.first_padding_offset;
        self.padding_count = other.padding_count;
        self.expected_padding = other.expected_padding;
        self.closed = other.closed;
    }
}

enum Recognized {
    Invalid,
    Data { value: u8, canonical: u8 },
    Padding,
}

fn recognize(byte: u8, alphabet: Alphabet) -> Recognized {
    if byte == b'=' {
        return Recognized::Padding;
    }

    let uppercase = byte.to_ascii_uppercase();
    let table = alphabet_bytes(alphabet);
    if let Some(index) = table.iter().position(|&b| b == uppercase) {
        Recognized::Data {
            value: index as u8,
            canonical: uppercase,
        }
    } else {
        Recognized::Invalid
    }
}

fn expected_padding(residue: usize) -> usize {
    match residue {
        0 => 0,
        2 => 6,
        4 => 4,
        5 => 3,
        7 => 1,
        _ => usize::MAX,
    }
}