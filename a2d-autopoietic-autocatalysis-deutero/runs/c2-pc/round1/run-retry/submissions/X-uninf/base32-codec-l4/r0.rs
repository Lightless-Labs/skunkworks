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
        let full_len = (combined.len() / 5) * 5;
        let mut output = String::new();

        for chunk in combined[..full_len].chunks_exact(5) {
            encode_bytes(chunk, self.config.alphabet, &mut output);
        }

        self.pending = combined[full_len..].to_vec();
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<String, StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }

        let mut output = String::new();
        if !self.pending.is_empty() {
            let symbols = encode_bytes(&self.pending, self.config.alphabet, &mut output);
            if self.config.padding == Padding::Required {
                let padded_len = ((symbols + 7) / 8) * 8;
                for _ in symbols..padded_len {
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

fn encode_bytes(input: &[u8], alphabet: Alphabet, output: &mut String) -> usize {
    let table = alphabet_bytes(alphabet);
    let mut accumulator: u32 = 0;
    let mut bits = 0;
    let mut count = 0;

    for &byte in input {
        accumulator = (accumulator << 8) | byte as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            output.push(table[((accumulator >> bits) & 31) as usize] as char);
            count += 1;
        }
    }

    if bits > 0 {
        output.push(table[((accumulator << (5 - bits)) & 31) as usize] as char);
        count += 1;
    }
    count
}

pub struct Decoder {
    config: Config,
    data_symbols: usize,
    pending_bits: u32,
    bit_count: u8,
    last_data_offset: Option<usize>,
    position: usize,
    first_padding_offset: Option<usize>,
    padding_count: usize,
    expected_padding: Option<usize>,
    closed: bool,
}

impl Decoder {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            data_symbols: 0,
            pending_bits: 0,
            bit_count: 0,
            last_data_offset: None,
            position: 0,
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
            let classified = classify(byte, next.config.alphabet);

            match classified {
                Classified::Invalid => {
                    return Err(StreamError::InvalidByte { offset, byte });
                }
                Classified::Data(value, canonical_byte, is_lowercase) => {
                    if is_lowercase && next.config.canonical {
                        return Err(StreamError::NonCanonical {
                            offset,
                            expected: canonical_byte,
                        });
                    }
                    if next.first_padding_offset.is_some() {
                        return Err(StreamError::Padding { offset });
                    }

                    next.data_symbols += 1;
                    next.last_data_offset = Some(offset);
                    next.pending_bits = (next.pending_bits << 5) | value as u32;
                    next.bit_count += 5;
                    while next.bit_count >= 8 {
                        next.bit_count -= 8;
                        output.push(((next.pending_bits >> next.bit_count) & 0xff) as u8);
                    }
                    if next.bit_count == 0 {
                        next.pending_bits = 0;
                    } else {
                        next.pending_bits &= (1u32 << next.bit_count) - 1;
                    }
                }
                Classified::Padding => {
                    if next.config.padding == Padding::Omitted {
                        return Err(StreamError::Padding { offset });
                    }

                    if next.first_padding_offset.is_none() {
                        let residue = next.data_symbols % 8;
                        let expected = match residue {
                            0 => return Err(StreamError::Padding { offset }),
                            2 => 6,
                            4 => 4,
                            5 => 3,
                            7 => 1,
                            _ => return Err(StreamError::Padding { offset }),
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

        self.copy_state_from(next);
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }

        let residue = self.data_symbols % 8;
        let unused = match residue {
            0 => 0,
            2 => 2,
            4 => 4,
            5 => 1,
            7 => 3,
            _ => {
                return Err(StreamError::InvalidLength {
                    offset: self.position,
                });
            }
        };

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

        if unused > 0 && self.pending_bits != 0 {
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
            data_symbols: self.data_symbols,
            pending_bits: self.pending_bits,
            bit_count: self.bit_count,
            last_data_offset: self.last_data_offset,
            position: self.position,
            first_padding_offset: self.first_padding_offset,
            padding_count: self.padding_count,
            expected_padding: self.expected_padding,
            closed: self.closed,
        }
    }

    fn copy_state_from(&mut self, other: Self) {
        self.data_symbols = other.data_symbols;
        self.pending_bits = other.pending_bits;
        self.bit_count = other.bit_count;
        self.last_data_offset = other.last_data_offset;
        self.position = other.position;
        self.first_padding_offset = other.first_padding_offset;
        self.padding_count = other.padding_count;
        self.expected_padding = other.expected_padding;
        self.closed = other.closed;
    }
}

enum Classified {
    Invalid,
    Data(u8, u8, bool),
    Padding,
}

fn classify(byte: u8, alphabet: Alphabet) -> Classified {
    if byte == b'=' {
        return Classified::Padding;
    }

    let upper = byte.to_ascii_uppercase();
    let value = match alphabet {
        Alphabet::Standard => match upper {
            b'A'..=b'Z' => Some(upper - b'A'),
            b'2'..=b'7' => Some(upper - b'2' + 26),
            _ => None,
        },
        Alphabet::Hex => match upper {
            b'0'..=b'9' => Some(upper - b'0'),
            b'A'..=b'V' => Some(upper - b'A' + 10),
            _ => None,
        },
    };

    match value {
        Some(value) => Classified::Data(value, upper, byte.is_ascii_lowercase()),
        None => Classified::Invalid,
    }
}