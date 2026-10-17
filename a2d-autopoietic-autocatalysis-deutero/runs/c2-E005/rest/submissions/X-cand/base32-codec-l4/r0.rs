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
    bits: u64,
    bit_count: u8,
    data_count: usize,
    position: usize,
    last_data_offset: Option<usize>,
    first_padding_offset: Option<usize>,
    padding_count: usize,
    expected_padding: usize,
    closed: bool,
}

fn alphabet_byte(alphabet: Alphabet, value: u8) -> u8 {
    match alphabet {
        Alphabet::Standard => b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"[value as usize],
        Alphabet::Hex => b"0123456789ABCDEFGHIJKLMNOPQRSTUV"[value as usize],
    }
}

fn alphabet_value(alphabet: Alphabet, byte: u8) -> Option<u8> {
    match alphabet {
        Alphabet::Standard => match byte {
            b'A'..=b'Z' => Some(byte - b'A'),
            b'2'..=b'7' => Some(byte - b'2' + 26),
            _ => None,
        },
        Alphabet::Hex => match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'A'..=b'V' => Some(byte - b'A' + 10),
            _ => None,
        },
    }
}

fn residue_padding(residue: usize) -> Option<usize> {
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

        let complete_bytes = combined.len() / 5 * 5;
        let mut output = String::new();
        for chunk in combined[..complete_bytes].chunks_exact(5) {
            let value = ((chunk[0] as u64) << 32)
                | ((chunk[1] as u64) << 24)
                | ((chunk[2] as u64) << 16)
                | ((chunk[3] as u64) << 8)
                | chunk[4] as u64;
            for shift in (0..8).rev() {
                output.push(alphabet_byte(self.config.alphabet, ((value >> (shift * 5)) & 31) as u8) as char);
            }
        }

        self.pending = combined[complete_bytes..].to_vec();
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<String, StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }

        let mut output = String::new();
        if !self.pending.is_empty() {
            let mut value = 0u64;
            for &byte in &self.pending {
                value = (value << 8) | byte as u64;
            }
            let bits = self.pending.len() * 8;
            let symbols = (bits + 4) / 5;
            value <<= symbols * 5 - bits;
            for index in 0..symbols {
                let shift = (symbols - index - 1) * 5;
                output.push(alphabet_byte(self.config.alphabet, ((value >> shift) & 31) as u8) as char);
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
            bits: 0,
            bit_count: 0,
            data_count: 0,
            position: 0,
            last_data_offset: None,
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
            let upper = byte.to_ascii_uppercase();
            let value = if byte == b'=' {
                None
            } else if let Some(value) = alphabet_value(next.config.alphabet, upper) {
                if next.config.canonical && byte != upper {
                    return Err(StreamError::NonCanonical {
                        offset,
                        expected: upper,
                    });
                }
                Some(value)
            } else {
                return Err(StreamError::InvalidByte { offset, byte });
            };

            if let Some(value) = value {
                if next.config.padding == Padding::Required && next.first_padding_offset.is_some() {
                    return Err(StreamError::Padding { offset });
                }
                next.data_count += 1;
                next.last_data_offset = Some(offset);
                next.bits = (next.bits << 5) | value as u64;
                next.bit_count += 5;
                while next.bit_count >= 8 {
                    next.bit_count -= 8;
                    output.push(((next.bits >> next.bit_count) & 0xff) as u8);
                    if next.bit_count == 0 {
                        next.bits = 0;
                    } else {
                        next.bits &= (1u64 << next.bit_count) - 1;
                    }
                }
            } else {
                if next.config.padding == Padding::Omitted {
                    return Err(StreamError::Padding { offset });
                }

                if next.first_padding_offset.is_none() {
                    let expected = match residue_padding(next.data_count % 8) {
                        Some(0) | None => return Err(StreamError::Padding { offset }),
                        Some(count) => count,
                    };
                    next.first_padding_offset = Some(offset);
                    next.expected_padding = expected;
                }

                if next.padding_count >= next.expected_padding {
                    return Err(StreamError::Padding { offset });
                }
                next.padding_count += 1;
            }

            next.position += 1;
        }

        self.assign_state(next);
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }

        let residue = self.data_count % 8;
        let expected = match residue_padding(residue) {
            Some(count) => count,
            None => {
                return Err(StreamError::InvalidLength {
                    offset: self.position,
                })
            }
        };

        if self.config.padding == Padding::Required && self.padding_count != expected {
            return Err(StreamError::Padding {
                offset: self.first_padding_offset.unwrap_or(self.position),
            });
        }

        let unused = match residue {
            2 => 2,
            4 => 4,
            5 => 1,
            7 => 3,
            _ => 0,
        };
        if unused != 0 && self.bits & ((1u64 << unused) - 1) != 0 {
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
            bits: self.bits,
            bit_count: self.bit_count,
            data_count: self.data_count,
            position: self.position,
            last_data_offset: self.last_data_offset,
            first_padding_offset: self.first_padding_offset,
            padding_count: self.padding_count,
            expected_padding: self.expected_padding,
            closed: self.closed,
        }
    }

    fn assign_state(&mut self, other: Self) {
        self.bits = other.bits;
        self.bit_count = other.bit_count;
        self.data_count = other.data_count;
        self.position = other.position;
        self.last_data_offset = other.last_data_offset;
        self.first_padding_offset = other.first_padding_offset;
        self.padding_count = other.padding_count;
        self.expected_padding = other.expected_padding;
        self.closed = other.closed;
    }
}