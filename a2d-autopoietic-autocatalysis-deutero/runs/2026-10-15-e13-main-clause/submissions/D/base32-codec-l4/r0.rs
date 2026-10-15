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

        let alphabet = alphabet_bytes(self.config.alphabet);
        let mut output = String::new();
        self.pending.extend_from_slice(input);

        while self.pending.len() >= 5 {
            encode_symbols(&self.pending[..5], 8, alphabet, &mut output);
            self.pending.drain(..5);
        }

        Ok(output)
    }

    pub fn finish(&mut self) -> Result<String, StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }

        let alphabet = alphabet_bytes(self.config.alphabet);
        let mut output = String::new();

        if !self.pending.is_empty() {
            let symbol_count = (self.pending.len() * 8 + 4) / 5;
            encode_symbols(&self.pending, symbol_count, alphabet, &mut output);

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

fn alphabet_bytes(alphabet: Alphabet) -> &'static [u8; 32] {
    match alphabet {
        Alphabet::Standard => b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567",
        Alphabet::Hex => b"0123456789ABCDEFGHIJKLMNOPQRSTUV",
    }
}

fn encode_symbols(input: &[u8], count: usize, alphabet: &[u8; 32], output: &mut String) {
    for symbol_index in 0..count {
        let start_bit = symbol_index * 5;
        let mut value = 0u8;

        for bit_index in 0..5 {
            let input_bit = start_bit + bit_index;
            value <<= 1;
            if input_bit < input.len() * 8 {
                let byte = input[input_bit / 8];
                value |= (byte >> (7 - input_bit % 8)) & 1;
            }
        }

        output.push(alphabet[value as usize] as char);
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
            let classified = classify(byte, next.config.alphabet)
                .ok_or(StreamError::InvalidByte { offset, byte })?;

            match classified {
                Classified::Data(value, canonical) => {
                    if next.config.canonical && !canonical {
                        return Err(StreamError::NonCanonical {
                            offset,
                            expected: byte.to_ascii_uppercase(),
                        });
                    }
                    if next.first_padding_offset.is_some() {
                        return Err(StreamError::Padding { offset });
                    }

                    next.data_count += 1;
                    next.last_data_offset = Some(offset);
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
                Classified::Padding => {
                    if next.config.padding == Padding::Omitted {
                        return Err(StreamError::Padding { offset });
                    }

                    if next.first_padding_offset.is_none() {
                        let expected = match expected_padding(next.data_count % 8) {
                            Some(0) | None => return Err(StreamError::Padding { offset }),
                            Some(count) => count,
                        };
                        next.first_padding_offset = Some(offset);
                        next.expected_padding = Some(expected);
                    }

                    next.padding_count += 1;
                    if next.padding_count > next.expected_padding.unwrap_or(0) {
                        return Err(StreamError::Padding { offset });
                    }
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

        if expected.is_none() {
            return Err(StreamError::InvalidLength {
                offset: self.position,
            });
        }

        if self.config.padding == Padding::Required {
            let expected_count = expected.unwrap();
            if self.padding_count != expected_count {
                return Err(StreamError::Padding {
                    offset: self.first_padding_offset.unwrap_or(self.position),
                });
            }
        }

        let unused_bits = match residue {
            0 => 0,
            2 => 2,
            4 => 4,
            5 => 1,
            7 => 3,
            _ => 0,
        };

        if unused_bits > 0 && (self.bit_buffer & ((1u32 << unused_bits) - 1)) != 0 {
            return Err(StreamError::TrailingBits {
                offset: self.last_data_offset.unwrap_or(self.position),
            });
        }

        self.closed = true;
        Ok(Vec::new())
    }
}

enum Classified {
    Data(u8, bool),
    Padding,
}

fn classify(byte: u8, alphabet: Alphabet) -> Option<Classified> {
    if byte == b'=' {
        return Some(Classified::Padding);
    }

    let upper = byte.to_ascii_uppercase();
    let values = alphabet_bytes(alphabet);
    values
        .iter()
        .position(|&candidate| candidate == upper)
        .map(|value| Classified::Data(value as u8, byte == upper))
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

    fn copy_from(&mut self, other: Self) {
        self.position = other.position;
        self.data_count = other.data_count;
        self.bit_buffer = other.bit_buffer;
        self.bit_count = other.bit_count;
        self.last_data_offset = other.last_data_offset;
        self.first_padding_offset = other.first_padding_offset;
        self.padding_count = other.padding_count;
        self.expected_padding = other.expected_padding;
        self.closed = other.closed;
    }
}

fn expected_padding(residue: usize) -> Option<usize> {
    match residue {
        0 => Some(0),
        2 => Some(6),
        4 => Some(4),
        5 => Some(3),
        7 => Some(1),
        _ => None,
    }
}