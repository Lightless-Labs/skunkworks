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
    buffer: [u8; 5],
    buffered: usize,
    closed: bool,
}

impl Encoder {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            buffer: [0; 5],
            buffered: 0,
            closed: false,
        }
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }

        let mut output = String::new();
        for &byte in input {
            self.buffer[self.buffered] = byte;
            self.buffered += 1;
            if self.buffered == 5 {
                encode_block(&self.buffer, &self.config, &mut output);
                self.buffered = 0;
            }
        }
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<String, StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }

        let mut output = String::new();
        if self.buffered != 0 {
            let mut block = [0u8; 5];
            block[..self.buffered].copy_from_slice(&self.buffer[..self.buffered]);
            let symbols = (self.buffered * 8 + 4) / 5;
            encode_symbols(&block, symbols, &self.config, &mut output);
            if self.config.padding == Padding::Required {
                while output.len() % 8 != 0 {
                    output.push('=');
                }
            }
        }

        self.buffered = 0;
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

fn encode_block(block: &[u8; 5], config: &Config, output: &mut String) {
    encode_symbols(block, 8, config, output);
}

fn encode_symbols(block: &[u8; 5], count: usize, config: &Config, output: &mut String) {
    for i in 0..count {
        let bit = i * 5;
        let byte_index = bit / 8;
        let shift = bit % 8;
        let mut value = (block[byte_index] << shift) & 0xF8;
        if shift > 3 && byte_index + 1 < 5 {
            value |= block[byte_index + 1] >> (8 - shift);
        }
        output.push(alphabet_byte(config.alphabet, value >> 3) as char);
    }
}

pub struct Decoder {
    config: Config,
    bit_buffer: u64,
    bit_count: usize,
    data_count: usize,
    total_consumed: usize,
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
            bit_buffer: 0,
            bit_count: 0,
            data_count: 0,
            total_consumed: 0,
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
            let offset = next.total_consumed;
            if byte == b'=' {
                if next.config.padding == Padding::Omitted {
                    return Err(StreamError::Padding { offset });
                }

                if next.first_padding_offset.is_none() {
                    let residue = next.data_count % 8;
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

                let expected = next.expected_padding.unwrap_or(0);
                if next.padding_count >= expected {
                    return Err(StreamError::Padding { offset });
                }
                next.padding_count += 1;
            } else {
                let classified = classify(byte, next.config.alphabet);
                let value = match classified {
                    Some(value) => value,
                    None => return Err(StreamError::InvalidByte { offset, byte }),
                };

                let upper = byte.to_ascii_uppercase();
                if next.config.canonical && byte != upper {
                    return Err(StreamError::NonCanonical {
                        offset,
                        expected: upper,
                    });
                }

                if next.first_padding_offset.is_some() {
                    return Err(StreamError::Padding { offset });
                }

                next.data_count += 1;
                next.last_data_offset = Some(offset);
                next.bit_buffer = (next.bit_buffer << 5) | value as u64;
                next.bit_count += 5;
                while next.bit_count >= 8 {
                    next.bit_count -= 8;
                    output.push((next.bit_buffer >> next.bit_count) as u8);
                    next.bit_buffer &= (1u64 << next.bit_count).wrapping_sub(1);
                }
            }

            next.total_consumed += 1;
        }

        self.apply_state(next);
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }

        let residue = self.data_count % 8;
        let expected = match residue {
            0 => 0,
            2 => 6,
            4 => 4,
            5 => 3,
            7 => 1,
            _ => return Err(StreamError::InvalidLength { offset: self.total_consumed }),
        };

        if self.config.padding == Padding::Required && self.padding_count != expected {
            return Err(StreamError::Padding {
                offset: self.first_padding_offset.unwrap_or(self.total_consumed),
            });
        }

        let unused = match residue {
            0 => 0,
            2 => 2,
            4 => 4,
            5 => 1,
            7 => 3,
            _ => 0,
        };
        if unused != 0 && self.bit_buffer & ((1u64 << unused) - 1) != 0 {
            return Err(StreamError::TrailingBits {
                offset: self.last_data_offset.unwrap_or(self.total_consumed),
            });
        }

        self.closed = true;
        Ok(Vec::new())
    }
}

impl Decoder {
    fn clone_state(&self) -> Self {
        Self {
            config: self.config,
            bit_buffer: self.bit_buffer,
            bit_count: self.bit_count,
            data_count: self.data_count,
            total_consumed: self.total_consumed,
            last_data_offset: self.last_data_offset,
            first_padding_offset: self.first_padding_offset,
            padding_count: self.padding_count,
            expected_padding: self.expected_padding,
            closed: self.closed,
        }
    }

    fn apply_state(&mut self, other: Self) {
        self.bit_buffer = other.bit_buffer;
        self.bit_count = other.bit_count;
        self.data_count = other.data_count;
        self.total_consumed = other.total_consumed;
        self.last_data_offset = other.last_data_offset;
        self.first_padding_offset = other.first_padding_offset;
        self.padding_count = other.padding_count;
        self.expected_padding = other.expected_padding;
        self.closed = other.closed;
    }
}

fn classify(byte: u8, alphabet: Alphabet) -> Option<u8> {
    let upper = byte.to_ascii_uppercase();
    match alphabet {
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
    }
}