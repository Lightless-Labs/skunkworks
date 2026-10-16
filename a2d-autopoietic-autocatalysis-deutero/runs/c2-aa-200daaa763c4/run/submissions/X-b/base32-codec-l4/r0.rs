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

        let mut combined = self.pending.clone();
        combined.extend_from_slice(input);

        let complete_len = (combined.len() / 5) * 5;
        let output = encode_bytes(&combined[..complete_len], self.config.alphabet);
        self.pending = combined[complete_len..].to_vec();

        Ok(output)
    }

    pub fn finish(&mut self) -> Result<String, StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }

        let mut output = encode_bytes(&self.pending, self.config.alphabet);
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

fn encode_bytes(input: &[u8], alphabet: Alphabet) -> String {
    let table = alphabet_bytes(alphabet);
    let mut output = String::new();
    let mut accumulator: u32 = 0;
    let mut bits = 0u8;

    for &byte in input {
        accumulator = (accumulator << 8) | u32::from(byte);
        bits += 8;

        while bits >= 5 {
            bits -= 5;
            let value = ((accumulator >> bits) & 31) as usize;
            output.push(table[value] as char);
        }
    }

    if bits > 0 {
        let value = ((accumulator << (5 - bits)) & 31) as usize;
        output.push(table[value] as char);
    }

    output
}

pub struct Decoder {
    config: Config,
    position: usize,
    data_count: usize,
    bit_buffer: u8,
    bit_count: u8,
    last_data_offset: Option<usize>,
    first_padding_offset: Option<usize>,
    expected_padding: usize,
    padding_count: usize,
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
            expected_padding: 0,
            padding_count: 0,
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

            if byte == b'=' {
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
                    next.expected_padding = expected;
                }

                if next.padding_count >= next.expected_padding {
                    return Err(StreamError::Padding { offset });
                }

                next.padding_count += 1;
                next.position += 1;
                continue;
            }

            let (value, canonical_byte) = match classify(byte, next.config.alphabet) {
                Some(pair) => pair,
                None => return Err(StreamError::InvalidByte { offset, byte }),
            };

            if next.config.canonical && byte != canonical_byte {
                return Err(StreamError::NonCanonical {
                    offset,
                    expected: canonical_byte,
                });
            }

            if next.first_padding_offset.is_some() {
                return Err(StreamError::Padding { offset });
            }

            next.data_count += 1;
            next.last_data_offset = Some(offset);

            next.bit_buffer = (next.bit_buffer << 5) | value;
            next.bit_count += 5;

            while next.bit_count >= 8 {
                next.bit_count -= 8;
                output.push((next.bit_buffer >> next.bit_count) & 0xff);

                if next.bit_count == 0 {
                    next.bit_buffer = 0;
                } else {
                    next.bit_buffer &= (1u8 << next.bit_count) - 1;
                }
            }

            next.position += 1;
        }

        self.copy_state_from(&next);
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

        if unused_bits > 0 && self.bit_buffer & ((1u8 << unused_bits) - 1) != 0 {
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
            first_padding_offset: self.first_padding_offset,
            expected_padding: self.expected_padding,
            padding_count: self.padding_count,
            closed: self.closed,
        }
    }

    fn copy_state_from(&mut self, other: &Self) {
        self.position = other.position;
        self.data_count = other.data_count;
        self.bit_buffer = other.bit_buffer;
        self.bit_count = other.bit_count;
        self.last_data_offset = other.last_data_offset;
        self.first_padding_offset = other.first_padding_offset;
        self.expected_padding = other.expected_padding;
        self.padding_count = other.padding_count;
        self.closed = other.closed;
    }
}

fn classify(byte: u8, alphabet: Alphabet) -> Option<(u8, u8)> {
    let upper = byte.to_ascii_uppercase();
    alphabet_bytes(alphabet)
        .iter()
        .position(|&candidate| candidate == upper)
        .map(|value| (value as u8, upper))
}