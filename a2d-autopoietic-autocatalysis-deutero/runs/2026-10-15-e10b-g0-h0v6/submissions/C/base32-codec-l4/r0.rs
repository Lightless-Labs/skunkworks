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
    buffered: Vec<u8>,
    closed: bool,
}

impl Encoder {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            buffered: Vec::new(),
            closed: false,
        }
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }

        let mut pending = self.buffered.clone();
        let mut output = String::new();
        pending.extend_from_slice(input);

        while pending.len() >= 5 {
            encode_symbols(
                &pending[..5],
                8,
                self.config.alphabet,
                &mut output,
            );
            pending.drain(..5);
        }

        self.buffered = pending;
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<String, StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }

        let mut output = String::new();
        if !self.buffered.is_empty() {
            let symbol_count = (self.buffered.len() * 8 + 4) / 5;
            encode_symbols(
                &self.buffered,
                symbol_count,
                self.config.alphabet,
                &mut output,
            );

            if self.config.padding == Padding::Required {
                while output.len() % 8 != 0 {
                    output.push('=');
                }
            }
        }

        self.buffered.clear();
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

fn encode_symbols(bytes: &[u8], symbol_count: usize, alphabet: Alphabet, out: &mut String) {
    let symbols = alphabet_bytes(alphabet);
    for i in 0..symbol_count {
        let mut value = 0u8;
        for bit in 0..5 {
            let source_bit = i * 5 + bit;
            value <<= 1;
            if source_bit < bytes.len() * 8 {
                let byte = bytes[source_bit / 8];
                value |= (byte >> (7 - (source_bit % 8))) & 1;
            }
        }
        out.push(symbols[value as usize] as char);
    }
}

pub struct Decoder {
    config: Config,
    position: usize,
    data_count: usize,
    pending_bits: u32,
    pending_bit_count: u8,
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
            pending_bits: 0,
            pending_bit_count: 0,
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
            let class = classify(byte, next.config.alphabet);

            match class {
                ByteClass::Invalid => {
                    return Err(StreamError::InvalidByte { offset, byte });
                }
                ByteClass::Data(value, canonical) => {
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
                    next.pending_bits = (next.pending_bits << 5) | value as u32;
                    next.pending_bit_count += 5;

                    while next.pending_bit_count >= 8 {
                        next.pending_bit_count -= 8;
                        output.push(
                            ((next.pending_bits >> next.pending_bit_count) & 0xff) as u8,
                        );
                        if next.pending_bit_count == 0 {
                            next.pending_bits = 0;
                        } else {
                            next.pending_bits &= (1u32 << next.pending_bit_count) - 1;
                        }
                    }
                }
                ByteClass::Padding => {
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

        let residue = self.data_count % 8;
        let expected_padding = match residue {
            0 => 0,
            2 => 6,
            4 => 4,
            5 => 3,
            7 => 1,
            _ => {
                return Err(StreamError::InvalidLength {
                    offset: self.position,
                });
            }
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

        if unused_bits != 0 && self.pending_bits != 0 {
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
            pending_bits: self.pending_bits,
            pending_bit_count: self.pending_bit_count,
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
        self.pending_bits = other.pending_bits;
        self.pending_bit_count = other.pending_bit_count;
        self.last_data_offset = other.last_data_offset;
        self.first_padding_offset = other.first_padding_offset;
        self.padding_count = other.padding_count;
        self.expected_padding = other.expected_padding;
        self.closed = other.closed;
    }
}

enum ByteClass {
    Invalid,
    Data(u8, u8),
    Padding,
}

fn classify(byte: u8, alphabet: Alphabet) -> ByteClass {
    if byte == b'=' {
        return ByteClass::Padding;
    }

    let uppercase = byte.to_ascii_uppercase();
    match alphabet_bytes(alphabet)
        .iter()
        .position(|&symbol| symbol == uppercase)
    {
        Some(value) => ByteClass::Data(value as u8, uppercase),
        None => ByteClass::Invalid,
    }
}

fn main() {}