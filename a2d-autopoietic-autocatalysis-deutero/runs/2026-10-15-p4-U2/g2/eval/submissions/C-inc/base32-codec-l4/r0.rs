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
    buffer: Vec<u8>,
    closed: bool,
}

impl Encoder {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            buffer: Vec::new(),
            closed: false,
        }
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }

        let mut combined = Vec::with_capacity(self.buffer.len() + input.len());
        combined.extend_from_slice(&self.buffer);
        combined.extend_from_slice(input);

        let complete_len = (combined.len() / 5) * 5;
        let mut output = String::new();
        for chunk in combined[..complete_len].chunks_exact(5) {
            encode_symbols(chunk, self.config.alphabet, &mut output);
        }

        self.buffer.clear();
        self.buffer.extend_from_slice(&combined[complete_len..]);
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<String, StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }

        let mut output = String::new();
        if !self.buffer.is_empty() {
            encode_symbols(&self.buffer, self.config.alphabet, &mut output);
            if self.config.padding == Padding::Required {
                while output.len() % 8 != 0 {
                    output.push('=');
                }
            }
        }

        self.buffer.clear();
        self.closed = true;
        Ok(output)
    }
}

fn encode_symbols(bytes: &[u8], alphabet: Alphabet, output: &mut String) {
    let chars = match alphabet {
        Alphabet::Standard => b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567",
        Alphabet::Hex => b"0123456789ABCDEFGHIJKLMNOPQRSTUV",
    };

    // Read each input bit MSB first and collect it into five-bit symbols.
    let bit_len = bytes.len() * 8;
    let symbol_count = (bit_len + 4) / 5;
    for symbol_index in 0..symbol_count {
        let mut value = 0u8;
        for bit_in_symbol in 0..5 {
            let bit_index = symbol_index * 5 + bit_in_symbol;
            value <<= 1;
            if bit_index < bit_len {
                let byte = bytes[bit_index / 8];
                value |= (byte >> (7 - bit_index % 8)) & 1;
            }
        }
        output.push(chars[value as usize] as char);
    }
}

pub struct Decoder {
    config: Config,
    position: usize,
    data_count: usize,
    bit_buffer: u8,
    bit_count: u8,
    last_data_offset: Option<usize>,
    last_symbol_value: u8,
    padding_started: bool,
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
            last_symbol_value: 0,
            padding_started: false,
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

        // Work on a copy so a failed call leaves the stream untouched.
        let mut next = self.clone_state();
        let mut output = Vec::new();

        for &byte in input.as_bytes() {
            let offset = next.position;

            match classify(byte, next.config.alphabet) {
                ByteClass::Invalid => {
                    return Err(StreamError::InvalidByte { offset, byte });
                }
                ByteClass::Data(value, was_lowercase) => {
                    if was_lowercase && next.config.canonical {
                        return Err(StreamError::NonCanonical {
                            offset,
                            expected: byte.to_ascii_uppercase(),
                        });
                    }
                    if next.padding_started {
                        return Err(StreamError::Padding { offset });
                    }

                    next.data_count += 1;
                    next.last_data_offset = Some(offset);
                    next.last_symbol_value = value;

                    // Retain unresolved low bits while emitting complete bytes.
                    next.bit_buffer = (next.bit_buffer << 5) | value;
                    next.bit_count += 5;
                    while next.bit_count >= 8 {
                        next.bit_count -= 8;
                        output.push(next.bit_buffer >> next.bit_count);
                        if next.bit_count == 0 {
                            next.bit_buffer = 0;
                        } else {
                            next.bit_buffer &= (1u8 << next.bit_count) - 1;
                        }
                    }
                }
                ByteClass::Padding => {
                    if next.config.padding == Padding::Omitted {
                        return Err(StreamError::Padding { offset });
                    }

                    if !next.padding_started {
                        let expected = match next.data_count % 8 {
                            2 => 6,
                            4 => 4,
                            5 => 3,
                            7 => 1,
                            _ => return Err(StreamError::Padding { offset }),
                        };
                        next.padding_started = true;
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
        let expected_padding = match residue {
            0 => 0,
            2 => 6,
            4 => 4,
            5 => 3,
            7 => 1,
            _ => return Err(StreamError::InvalidLength { offset: self.position }),
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

        if unused_bits > 0 {
            let mask = (1u8 << unused_bits) - 1;
            if self.last_symbol_value & mask != 0 {
                return Err(StreamError::TrailingBits {
                    offset: self.last_data_offset.unwrap_or(self.position),
                });
            }
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
            last_symbol_value: self.last_symbol_value,
            padding_started: self.padding_started,
            first_padding_offset: self.first_padding_offset,
            padding_count: self.padding_count,
            expected_padding: self.expected_padding,
            closed: self.closed,
        }
    }

    fn copy_from(&mut self, other: Self) {
        self.config = other.config;
        self.position = other.position;
        self.data_count = other.data_count;
        self.bit_buffer = other.bit_buffer;
        self.bit_count = other.bit_count;
        self.last_data_offset = other.last_data_offset;
        self.last_symbol_value = other.last_symbol_value;
        self.padding_started = other.padding_started;
        self.first_padding_offset = other.first_padding_offset;
        self.padding_count = other.padding_count;
        self.expected_padding = other.expected_padding;
        self.closed = other.closed;
    }
}

enum ByteClass {
    Data(u8, bool),
    Padding,
    Invalid,
}

fn classify(byte: u8, alphabet: Alphabet) -> ByteClass {
    if byte == b'=' {
        return ByteClass::Padding;
    }

    let upper = byte.to_ascii_uppercase();
    let value = match alphabet {
        Alphabet::Standard => {
            if (b'A'..=b'Z').contains(&upper) {
                Some(upper - b'A')
            } else if (b'2'..=b'7').contains(&upper) {
                Some(upper - b'2' + 26)
            } else {
                None
            }
        }
        Alphabet::Hex => {
            if (b'0'..=b'9').contains(&upper) {
                Some(upper - b'0')
            } else if (b'A'..=b'V').contains(&upper) {
                Some(upper - b'A' + 10)
            } else {
                None
            }
        }
    };

    match value {
        Some(value) => ByteClass::Data(value, byte.is_ascii_lowercase()),
        None => ByteClass::Invalid,
    }
}

fn main() {}