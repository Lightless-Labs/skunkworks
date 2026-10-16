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

        let mut combined = Vec::with_capacity(self.pending.len() + input.len());
        combined.extend_from_slice(&self.pending);
        combined.extend_from_slice(input);

        let blocks = combined.len() / 5;
        let consumed = blocks * 5;
        let mut output = String::with_capacity(blocks * 8);
        for block in combined[..consumed].chunks_exact(5) {
            encode_bytes(block, self.config.alphabet, &mut output);
        }

        self.pending.clear();
        self.pending.extend_from_slice(&combined[consumed..]);
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<String, StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }

        let mut output = String::new();
        if !self.pending.is_empty() {
            encode_bytes(&self.pending, self.config.alphabet, &mut output);
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

fn alphabet_byte(alphabet: Alphabet, value: u8) -> u8 {
    match alphabet {
        Alphabet::Standard => b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"[value as usize],
        Alphabet::Hex => b"0123456789ABCDEFGHIJKLMNOPQRSTUV"[value as usize],
    }
}

fn encode_bytes(bytes: &[u8], alphabet: Alphabet, output: &mut String) {
    let mut accumulator: u32 = 0;
    let mut bits = 0u32;

    for &byte in bytes {
        accumulator = (accumulator << 8) | u32::from(byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            let value = ((accumulator >> bits) & 0x1f) as u8;
            output.push(alphabet_byte(alphabet, value) as char);
        }
    }

    if bits > 0 {
        let value = ((accumulator << (5 - bits)) & 0x1f) as u8;
        output.push(alphabet_byte(alphabet, value) as char);
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
            let symbol = classify(byte, next.config.alphabet)
                .ok_or(StreamError::InvalidByte { offset, byte })?;

            match symbol {
                Symbol::Data(value, canonical) => {
                    if next.config.canonical && byte != canonical {
                        return Err(StreamError::NonCanonical {
                            offset,
                            expected: canonical,
                        });
                    }
                    if next.first_padding_offset.is_some() {
                        return Err(StreamError::Padding { offset });
                    }

                    next.data_count = next.data_count.wrapping_add(1);
                    next.last_data_offset = Some(offset);
                    next.bit_buffer = (next.bit_buffer << 5) | u32::from(value);
                    next.bit_count += 5;

                    while next.bit_count >= 8 {
                        next.bit_count -= 8;
                        output.push(((next.bit_buffer >> next.bit_count) & 0xff) as u8);
                    }
                    if next.bit_count == 0 {
                        next.bit_buffer = 0;
                    } else {
                        next.bit_buffer &= (1u32 << next.bit_count) - 1;
                    }
                }
                Symbol::Padding => match next.config.padding {
                    Padding::Omitted => return Err(StreamError::Padding { offset }),
                    Padding::Required => {
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
                    }
                },
            }

            next.position = next.position.wrapping_add(1);
        }

        self.assign_state(next);
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }

        let residue = self.data_count % 8;
        let unused_bits = match residue {
            0 => 0,
            2 => 2,
            4 => 4,
            5 => 1,
            7 => 3,
            _ => {
                return Err(StreamError::InvalidLength {
                    offset: self.position,
                })
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

        if unused_bits > 0 && self.bit_buffer != 0 {
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
            padding_count: self.padding_count,
            expected_padding: self.expected_padding,
            closed: self.closed,
        }
    }

    fn assign_state(&mut self, other: Self) {
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

enum Symbol {
    Data(u8, u8),
    Padding,
}

fn classify(byte: u8, alphabet: Alphabet) -> Option<Symbol> {
    if byte == b'=' {
        return Some(Symbol::Padding);
    }

    let upper = byte.to_ascii_uppercase();
    let values = match alphabet {
        Alphabet::Standard => b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567".as_slice(),
        Alphabet::Hex => b"0123456789ABCDEFGHIJKLMNOPQRSTUV".as_slice(),
    };
    values
        .iter()
        .position(|&candidate| candidate == upper)
        .map(|value| Symbol::Data(value as u8, upper))
}