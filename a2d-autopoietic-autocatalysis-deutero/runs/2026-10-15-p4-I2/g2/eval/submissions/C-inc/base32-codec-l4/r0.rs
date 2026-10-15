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

        let complete_len = (bytes.len() / 5) * 5;
        let output = encode_bytes(self.config.alphabet, &bytes[..complete_len], false);
        self.pending = bytes[complete_len..].to_vec();
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<String, StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }

        let mut output = encode_bytes(self.config.alphabet, &self.pending, true);
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

fn encode_bytes(alphabet: Alphabet, bytes: &[u8], final_fragment: bool) -> String {
    let symbols = alphabet_bytes(alphabet);
    let mut output = String::new();
    let mut accumulator: u64 = 0;
    let mut bit_count = 0usize;

    for &byte in bytes {
        accumulator = (accumulator << 8) | u64::from(byte);
        bit_count += 8;

        while bit_count >= 5 {
            bit_count -= 5;
            output.push(symbols[((accumulator >> bit_count) & 31) as usize] as char);
        }

        if bit_count == 0 {
            accumulator = 0;
        } else {
            accumulator &= (1u64 << bit_count) - 1;
        }
    }

    if final_fragment && bit_count > 0 {
        output.push(symbols[((accumulator << (5 - bit_count)) & 31) as usize] as char);
    }

    output
}

pub struct Decoder {
    config: Config,
    position: usize,
    data_count: usize,
    accumulator: u8,
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
            accumulator: 0,
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
            let value = classify(byte, next.config.alphabet);
            let is_padding = byte == b'=';

            if value.is_none() && !is_padding {
                return Err(StreamError::InvalidByte { offset, byte });
            }

            if let Some(value) = value {
                if byte.is_ascii_lowercase() && next.config.canonical {
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

                next.accumulator = (next.accumulator << 5) | value;
                next.bit_count += 5;

                while next.bit_count >= 8 {
                    next.bit_count -= 8;
                    output.push(next.accumulator >> next.bit_count);
                }

                if next.bit_count == 0 {
                    next.accumulator = 0;
                } else {
                    next.accumulator &= (1u8 << next.bit_count) - 1;
                }
            } else {
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

                let expected = next.expected_padding.unwrap_or(0);
                if next.padding_count >= expected {
                    return Err(StreamError::Padding { offset });
                }
                next.padding_count += 1;
            }

            next.position += 1;
        }

        self.copy_state_from(next);
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
            if self.accumulator & mask != 0 {
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
            accumulator: self.accumulator,
            bit_count: self.bit_count,
            last_data_offset: self.last_data_offset,
            first_padding_offset: self.first_padding_offset,
            padding_count: self.padding_count,
            expected_padding: self.expected_padding,
            closed: self.closed,
        }
    }

    fn copy_state_from(&mut self, other: Self) {
        self.position = other.position;
        self.data_count = other.data_count;
        self.accumulator = other.accumulator;
        self.bit_count = other.bit_count;
        self.last_data_offset = other.last_data_offset;
        self.first_padding_offset = other.first_padding_offset;
        self.padding_count = other.padding_count;
        self.expected_padding = other.expected_padding;
        self.closed = other.closed;
    }
}

fn classify(byte: u8, alphabet: Alphabet) -> Option<u8> {
    let upper = byte.to_ascii_uppercase();
    alphabet_bytes(alphabet)
        .iter()
        .position(|&symbol| symbol == upper)
        .map(|value| value as u8)
}

fn main() {}