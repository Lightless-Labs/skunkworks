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

        let mut output = String::new();
        let mut bytes = core::mem::take(&mut self.pending);
        bytes.extend_from_slice(input);

        let complete_len = bytes.len() / 5 * 5;
        encode_bytes(
            &bytes[..complete_len],
            self.config.alphabet,
            &mut output,
        );
        self.pending = bytes[complete_len..].to_vec();

        Ok(output)
    }

    pub fn finish(&mut self) -> Result<String, StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }

        let mut output = String::new();
        if !self.pending.is_empty() {
            let symbol_count = (self.pending.len() * 8 + 4) / 5;
            encode_bytes(&self.pending, self.config.alphabet, &mut output);
            if self.config.padding == Padding::Required {
                while output.len() % 8 != 0 {
                    output.push('=');
                }
            }
            debug_assert_eq!(output.len(), if self.config.padding == Padding::Required { 8 } else { symbol_count });
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

fn encode_bytes(bytes: &[u8], alphabet: Alphabet, output: &mut String) {
    let table = alphabet_bytes(alphabet);
    let mut accumulator: u32 = 0;
    let mut bits = 0usize;

    for &byte in bytes {
        accumulator = (accumulator << 8) | byte as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            output.push(table[((accumulator >> bits) & 31) as usize] as char);
        }
    }

    if bits != 0 {
        let value = ((accumulator << (5 - bits)) & 31) as usize;
        output.push(table[value] as char);
    }
}

pub struct Decoder {
    config: Config,
    bit_buffer: u32,
    bit_count: usize,
    data_count: usize,
    position: usize,
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
            bit_buffer: 0,
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
            let classification = classify(byte, next.config.alphabet);

            match classification {
                ByteClass::Invalid => {
                    return Err(StreamError::InvalidByte { offset, byte });
                }
                ByteClass::Data(value, uppercase) => {
                    if next.config.canonical && byte != uppercase {
                        return Err(StreamError::NonCanonical {
                            offset,
                            expected: uppercase,
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
                    }
                    next.bit_buffer &= if next.bit_count == 0 {
                        0
                    } else {
                        (1u32 << next.bit_count) - 1
                    };
                }
                ByteClass::Padding => {
                    if next.config.padding == Padding::Omitted {
                        return Err(StreamError::Padding { offset });
                    }

                    if next.first_padding_offset.is_none() {
                        let residue = next.data_count % 8;
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

        let residue = self.data_count % 8;
        let expected_padding = match residue {
            0 => 0,
            2 => 6,
            4 => 4,
            5 => 3,
            7 => 1,
            _ => return Err(StreamError::InvalidLength { offset: self.position }),
        };

        if self.config.padding == Padding::Required && self.padding_count != expected_padding {
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

        if unused_bits != 0 && self.bit_buffer != 0 {
            return Err(StreamError::TrailingBits {
                offset: self.last_data_offset.unwrap_or(self.position),
            });
        }

        self.closed = true;
        Ok(Vec::new())
    }
}

enum ByteClass {
    Data(u8, u8),
    Padding,
    Invalid,
}

fn classify(byte: u8, alphabet: Alphabet) -> ByteClass {
    if byte == b'=' {
        return ByteClass::Padding;
    }

    let upper = byte.to_ascii_uppercase();
    let table = alphabet_bytes(alphabet);
    if let Some(index) = table.iter().position(|&candidate| candidate == upper) {
        ByteClass::Data(index as u8, upper)
    } else {
        ByteClass::Invalid
    }
}

impl Decoder {
    fn clone_state(&self) -> Self {
        Self {
            config: self.config,
            bit_buffer: self.bit_buffer,
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

    fn copy_state_from(&mut self, other: Self) {
        self.bit_buffer = other.bit_buffer;
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

fn main() {}