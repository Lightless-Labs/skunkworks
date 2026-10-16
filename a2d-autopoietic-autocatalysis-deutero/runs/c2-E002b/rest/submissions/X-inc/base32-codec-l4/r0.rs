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
        let mut output = String::new();
        for chunk in combined[..complete_len].chunks_exact(5) {
            encode_symbols(chunk, &mut output, self.config.alphabet);
        }

        self.pending = combined[complete_len..].to_vec();
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<String, StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }

        let mut output = String::new();
        if !self.pending.is_empty() {
            let symbol_count = (self.pending.len() * 8 + 4) / 5;
            encode_symbols(&self.pending, &mut output, self.config.alphabet);
            if self.config.padding == Padding::Required {
                while output.len() % 8 != 0 {
                    output.push('=');
                }
            }
            debug_assert_eq!(symbol_count, (self.pending.len() * 8 + 4) / 5);
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

fn encode_symbols(input: &[u8], output: &mut String, alphabet: Alphabet) {
    let table = alphabet_bytes(alphabet);
    let bit_count = input.len() * 8;
    let symbol_count = (bit_count + 4) / 5;

    for symbol_index in 0..symbol_count {
        let mut value = 0u8;
        for bit_offset in 0..5 {
            let bit_index = symbol_index * 5 + bit_offset;
            value <<= 1;
            if bit_index < bit_count {
                let byte = input[bit_index / 8];
                value |= (byte >> (7 - bit_index % 8)) & 1;
            }
        }
        output.push(table[value as usize] as char);
    }
}

pub struct Decoder {
    config: Config,
    position: usize,
    data_symbols: usize,
    pending_value: u32,
    pending_bits: u8,
    last_data_offset: Option<usize>,
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
            data_symbols: 0,
            pending_value: 0,
            pending_bits: 0,
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
            if byte == b'=' {
                if next.config.padding == Padding::Omitted {
                    return Err(StreamError::Padding { offset });
                }

                if next.first_padding_offset.is_none() {
                    let residue = next.data_symbols % 8;
                    let expected = match residue {
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
            } else {
                let value = match classify_data(byte, next.config.alphabet) {
                    Some(value) => value,
                    None => return Err(StreamError::InvalidByte { offset, byte }),
                };

                let upper = byte.to_ascii_uppercase();
                if byte != upper && next.config.canonical {
                    return Err(StreamError::NonCanonical {
                        offset,
                        expected: upper,
                    });
                }

                if next.first_padding_offset.is_some() {
                    return Err(StreamError::Padding { offset });
                }

                next.data_symbols += 1;
                next.last_data_offset = Some(offset);
                next.pending_value = (next.pending_value << 5) | value as u32;
                next.pending_bits += 5;

                while next.pending_bits >= 8 {
                    next.pending_bits -= 8;
                    output.push((next.pending_value >> next.pending_bits) as u8);
                    if next.pending_bits == 0 {
                        next.pending_value = 0;
                    } else {
                        next.pending_value &= (1u32 << next.pending_bits) - 1;
                    }
                }
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

        let residue = self.data_symbols % 8;
        if !matches!(residue, 0 | 2 | 4 | 5 | 7) {
            return Err(StreamError::InvalidLength {
                offset: self.position,
            });
        }

        if self.config.padding == Padding::Required {
            let expected = match residue {
                0 => 0,
                2 => 6,
                4 => 4,
                5 => 3,
                7 => 1,
                _ => unreachable!(),
            };
            if self.padding_count != expected {
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
            _ => unreachable!(),
        };
        if unused_bits != 0 && self.pending_value != 0 {
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
            data_symbols: self.data_symbols,
            pending_value: self.pending_value,
            pending_bits: self.pending_bits,
            last_data_offset: self.last_data_offset,
            first_padding_offset: self.first_padding_offset,
            padding_count: self.padding_count,
            expected_padding: self.expected_padding,
            closed: self.closed,
        }
    }

    fn copy_state_from(&mut self, other: Self) {
        self.position = other.position;
        self.data_symbols = other.data_symbols;
        self.pending_value = other.pending_value;
        self.pending_bits = other.pending_bits;
        self.last_data_offset = other.last_data_offset;
        self.first_padding_offset = other.first_padding_offset;
        self.padding_count = other.padding_count;
        self.expected_padding = other.expected_padding;
        self.closed = other.closed;
    }
}

fn classify_data(byte: u8, alphabet: Alphabet) -> Option<u8> {
    let upper = byte.to_ascii_uppercase();
    alphabet_bytes(alphabet)
        .iter()
        .position(|&candidate| candidate == upper)
        .map(|value| value as u8)
}