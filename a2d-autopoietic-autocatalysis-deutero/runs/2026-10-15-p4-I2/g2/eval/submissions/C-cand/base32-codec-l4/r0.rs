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

        let complete_len = (self.buffer.len() + input.len()) / 5 * 5;
        let mut bytes = Vec::with_capacity(self.buffer.len() + input.len());
        bytes.extend_from_slice(&self.buffer);
        bytes.extend_from_slice(input);

        let mut output = String::new();
        for chunk in bytes[..complete_len].chunks_exact(5) {
            encode_block(chunk, self.config.alphabet, &mut output);
        }

        self.buffer.clear();
        self.buffer.extend_from_slice(&bytes[complete_len..]);
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<String, StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }

        let mut output = String::new();
        if !self.buffer.is_empty() {
            encode_partial(&self.buffer, self.config.alphabet, &mut output);
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

fn alphabet_byte(alphabet: Alphabet, value: u8) -> u8 {
    match alphabet {
        Alphabet::Standard => b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"[value as usize],
        Alphabet::Hex => b"0123456789ABCDEFGHIJKLMNOPQRSTUV"[value as usize],
    }
}

fn encode_block(bytes: &[u8], alphabet: Alphabet, output: &mut String) {
    for symbol in 0..8 {
        let mut value = 0u8;
        for bit in 0..5 {
            let source = symbol * 5 + bit;
            value <<= 1;
            if source < 40 {
                value |= (bytes[source / 8] >> (7 - source % 8)) & 1;
            }
        }
        output.push(alphabet_byte(alphabet, value) as char);
    }
}

fn encode_partial(bytes: &[u8], alphabet: Alphabet, output: &mut String) {
    let bit_count = bytes.len() * 8;
    for symbol in 0..(bit_count + 4) / 5 {
        let mut value = 0u8;
        for bit in 0..5 {
            let source = symbol * 5 + bit;
            value <<= 1;
            if source < bit_count {
                value |= (bytes[source / 8] >> (7 - source % 8)) & 1;
            }
        }
        output.push(alphabet_byte(alphabet, value) as char);
    }
}

#[derive(Clone)]
pub struct Decoder {
    config: Config,
    bit_buffer: u64,
    bit_count: u8,
    data_count: usize,
    total_consumed: usize,
    last_data_offset: Option<usize>,
    tail_value: u8,
    accepted_padding: usize,
    first_padding_offset: Option<usize>,
    expected_padding: Option<usize>,
    closed: bool,
}

impl Decoder {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            bit_buffer: 0,
            bit_count: 0,
            data_count: 0,
            total_consumed: 0,
            last_data_offset: None,
            tail_value: 0,
            accepted_padding: 0,
            first_padding_offset: None,
            expected_padding: None,
            closed: false,
        }
    }

    pub fn push(&mut self, input: &str) -> Result<Vec<u8>, StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }

        let mut next = self.clone();
        let mut output = Vec::new();

        for &byte in input.as_bytes() {
            let offset = next.total_consumed;
            match classify(byte, next.config.alphabet) {
                Classified::Invalid => {
                    return Err(StreamError::InvalidByte { offset, byte });
                }
                Classified::Data { value, canonical } => {
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
                    next.tail_value = value;
                    next.bit_buffer = (next.bit_buffer << 5) | value as u64;
                    next.bit_count += 5;

                    while next.bit_count >= 8 {
                        next.bit_count -= 8;
                        output.push((next.bit_buffer >> next.bit_count) as u8);
                        if next.bit_count == 0 {
                            next.bit_buffer = 0;
                        } else {
                            next.bit_buffer &= (1u64 << next.bit_count) - 1;
                        }
                    }
                }
                Classified::Padding => {
                    if next.config.padding == Padding::Omitted {
                        return Err(StreamError::Padding { offset });
                    }

                    if next.first_padding_offset.is_none() {
                        let expected = match next.data_count % 8 {
                            2 => 6,
                            4 => 4,
                            5 => 3,
                            7 => 1,
                            _ => return Err(StreamError::Padding { offset }),
                        };
                        next.first_padding_offset = Some(offset);
                        next.expected_padding = Some(expected);
                    }

                    let expected = next.expected_padding.unwrap();
                    if next.accepted_padding >= expected {
                        return Err(StreamError::Padding { offset });
                    }
                    next.accepted_padding += 1;
                }
            }
            next.total_consumed += 1;
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
                    offset: self.total_consumed,
                });
            }
        };

        if self.config.padding == Padding::Required
            && self.accepted_padding != expected_padding
        {
            return Err(StreamError::Padding {
                offset: self.first_padding_offset.unwrap_or(self.total_consumed),
            });
        }

        let unused_bits = match residue {
            0 => 0,
            2 => 2,
            4 => 4,
            5 => 1,
            7 => 3,
            _ => unreachable!(),
        };

        if unused_bits != 0 {
            let mask = (1u8 << unused_bits) - 1;
            if self.tail_value & mask != 0 {
                return Err(StreamError::TrailingBits {
                    offset: self.last_data_offset.unwrap_or(self.total_consumed),
                });
            }
        }

        self.closed = true;
        Ok(Vec::new())
    }
}

enum Classified {
    Invalid,
    Padding,
    Data { value: u8, canonical: u8 },
}

fn classify(byte: u8, alphabet: Alphabet) -> Classified {
    if byte == b'=' {
        return Classified::Padding;
    }

    let upper = byte.to_ascii_uppercase();
    let values: &[u8] = match alphabet {
        Alphabet::Standard => b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567",
        Alphabet::Hex => b"0123456789ABCDEFGHIJKLMNOPQRSTUV",
    };

    match values.iter().position(|&candidate| candidate == upper) {
        Some(value) => Classified::Data {
            value: value as u8,
            canonical: upper,
        },
        None => Classified::Invalid,
    }
}

fn main() {}