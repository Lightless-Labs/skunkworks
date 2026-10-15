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

pub struct Decoder {
    config: Config,
    position: usize,
    data_count: usize,
    bit_buffer: u32,
    bit_count: u8,
    padding_started: bool,
    first_padding_offset: Option<usize>,
    padding_count: usize,
    expected_padding: Option<usize>,
    last_data_offset: Option<usize>,
    last_data_value: u8,
    closed: bool,
}

fn alphabet_bytes(alphabet: Alphabet) -> &'static [u8; 32] {
    match alphabet {
        Alphabet::Standard => b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567",
        Alphabet::Hex => b"0123456789ABCDEFGHIJKLMNOPQRSTUV",
    }
}

fn symbol_value(alphabet: Alphabet, byte: u8) -> Option<u8> {
    let upper = byte.to_ascii_uppercase();
    alphabet_bytes(alphabet)
        .iter()
        .position(|&b| b == upper)
        .map(|v| v as u8)
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
        let mut output = String::new();
        let alphabet = alphabet_bytes(self.config.alphabet);

        for chunk in combined[..complete_len].chunks_exact(5) {
            let bits = ((chunk[0] as u64) << 32)
                | ((chunk[1] as u64) << 24)
                | ((chunk[2] as u64) << 16)
                | ((chunk[3] as u64) << 8)
                | chunk[4] as u64;

            for i in 0..8 {
                let shift = 35 - i * 5;
                let value = ((bits >> shift) & 0x1f) as usize;
                output.push(alphabet[value] as char);
            }
        }

        self.pending = combined[complete_len..].to_vec();
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<String, StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }

        let alphabet = alphabet_bytes(self.config.alphabet);
        let mut output = String::new();

        if !self.pending.is_empty() {
            let byte_count = self.pending.len();
            let mut bits = 0u64;
            for &byte in &self.pending {
                bits = (bits << 8) | byte as u64;
            }
            bits <<= (5 - byte_count) * 8;

            let symbol_count = (byte_count * 8 + 4) / 5;
            for i in 0..symbol_count {
                let shift = 35 - i * 5;
                let value = ((bits >> shift) & 0x1f) as usize;
                output.push(alphabet[value] as char);
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

impl Decoder {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            position: 0,
            data_count: 0,
            bit_buffer: 0,
            bit_count: 0,
            padding_started: false,
            first_padding_offset: None,
            padding_count: 0,
            expected_padding: None,
            last_data_offset: None,
            last_data_value: 0,
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

                if !next.padding_started {
                    let residue = next.data_count % 8;
                    let Some(expected) = expected_padding(residue) else {
                        return Err(StreamError::Padding { offset });
                    };
                    if expected == 0 {
                        return Err(StreamError::Padding { offset });
                    }
                    next.padding_started = true;
                    next.first_padding_offset = Some(offset);
                    next.expected_padding = Some(expected);
                }

                let expected = next.expected_padding.unwrap_or(0);
                if next.padding_count >= expected {
                    return Err(StreamError::Padding { offset });
                }
                next.padding_count += 1;
            } else {
                let Some(value) = symbol_value(next.config.alphabet, byte) else {
                    return Err(StreamError::InvalidByte { offset, byte });
                };

                if byte.is_ascii_lowercase() && next.config.canonical {
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

            next.position += 1;
        }

        self.apply_state(next);
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }

        let residue = self.data_count % 8;
        let Some(expected) = expected_padding(residue) else {
            return Err(StreamError::InvalidLength {
                offset: self.position,
            });
        };

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

        if unused_bits != 0 {
            let mask = (1u8 << unused_bits) - 1;
            if self.last_data_value & mask != 0 {
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
            padding_started: self.padding_started,
            first_padding_offset: self.first_padding_offset,
            padding_count: self.padding_count,
            expected_padding: self.expected_padding,
            last_data_offset: self.last_data_offset,
            last_data_value: self.last_data_value,
            closed: self.closed,
        }
    }

    fn apply_state(&mut self, other: Self) {
        self.position = other.position;
        self.data_count = other.data_count;
        self.bit_buffer = other.bit_buffer;
        self.bit_count = other.bit_count;
        self.padding_started = other.padding_started;
        self.first_padding_offset = other.first_padding_offset;
        self.padding_count = other.padding_count;
        self.expected_padding = other.expected_padding;
        self.last_data_offset = other.last_data_offset;
        self.last_data_value = other.last_data_value;
        self.closed = other.closed;
    }
}

fn main() {}