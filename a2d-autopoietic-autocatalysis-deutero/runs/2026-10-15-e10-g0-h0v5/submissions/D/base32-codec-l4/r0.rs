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

        let alphabet = alphabet_bytes(self.config.alphabet);
        let mut combined = Vec::with_capacity(self.pending.len() + input.len());
        combined.extend_from_slice(&self.pending);
        combined.extend_from_slice(input);

        let complete_len = (combined.len() / 5) * 5;
        let mut output = String::with_capacity((complete_len / 5) * 8);

        for block in combined[..complete_len].chunks_exact(5) {
            encode_bytes(block, alphabet, &mut output);
        }

        self.pending.clear();
        self.pending.extend_from_slice(&combined[complete_len..]);
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<String, StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }

        let alphabet = alphabet_bytes(self.config.alphabet);
        let mut output = String::new();

        if !self.pending.is_empty() {
            let data_symbols = (self.pending.len() * 8 + 4) / 5;
            encode_bytes(&self.pending, alphabet, &mut output);
            output.truncate(data_symbols);

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

pub struct Decoder {
    config: Config,
    position: usize,
    data_symbols: usize,
    accepted_padding: usize,
    first_padding_offset: Option<usize>,
    expected_padding: Option<usize>,
    bits: u32,
    bit_count: u8,
    last_data_offset: Option<usize>,
    closed: bool,
}

impl Decoder {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            position: 0,
            data_symbols: 0,
            accepted_padding: 0,
            first_padding_offset: None,
            expected_padding: None,
            bits: 0,
            bit_count: 0,
            last_data_offset: None,
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

                let expected = next.expected_padding.unwrap();
                if next.accepted_padding >= expected {
                    return Err(StreamError::Padding { offset });
                }
                next.accepted_padding += 1;
                next.position += 1;
                continue;
            }

            let value = match classify_data(byte, next.config.alphabet) {
                Some(value) => value,
                None => return Err(StreamError::InvalidByte { offset, byte }),
            };

            if byte.is_ascii_lowercase() && next.config.canonical {
                return Err(StreamError::NonCanonical {
                    offset,
                    expected: byte.to_ascii_uppercase(),
                });
            }

            if next.first_padding_offset.is_some() {
                return Err(StreamError::Padding { offset });
            }

            next.data_symbols += 1;
            next.last_data_offset = Some(offset);
            next.bits = (next.bits << 5) | value as u32;
            next.bit_count += 5;

            while next.bit_count >= 8 {
                next.bit_count -= 8;
                output.push(((next.bits >> next.bit_count) & 0xff) as u8);
                if next.bit_count == 0 {
                    next.bits = 0;
                } else {
                    next.bits &= (1u32 << next.bit_count) - 1;
                }
            }

            next.position += 1;
        }

        *self = next;
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
            if self.accepted_padding != expected {
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

        if unused_bits != 0 && self.bits & ((1u32 << unused_bits) - 1) != 0 {
            return Err(StreamError::TrailingBits {
                offset: self.last_data_offset.unwrap_or(self.position),
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
            position: self.position,
            data_symbols: self.data_symbols,
            accepted_padding: self.accepted_padding,
            first_padding_offset: self.first_padding_offset,
            expected_padding: self.expected_padding,
            bits: self.bits,
            bit_count: self.bit_count,
            last_data_offset: self.last_data_offset,
            closed: self.closed,
        }
    }
}

fn alphabet_bytes(alphabet: Alphabet) -> &'static [u8; 32] {
    match alphabet {
        Alphabet::Standard => b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567",
        Alphabet::Hex => b"0123456789ABCDEFGHIJKLMNOPQRSTUV",
    }
}

fn classify_data(byte: u8, alphabet: Alphabet) -> Option<u8> {
    let canonical = byte.to_ascii_uppercase();
    alphabet_bytes(alphabet)
        .iter()
        .position(|&candidate| candidate == canonical)
        .map(|value| value as u8)
}

fn encode_bytes(bytes: &[u8], alphabet: &[u8; 32], output: &mut String) {
    let mut accumulator: u32 = 0;
    let mut bits = 0u8;

    for &byte in bytes {
        accumulator = (accumulator << 8) | byte as u32;
        bits += 8;

        while bits >= 5 {
            bits -= 5;
            output.push(alphabet[((accumulator >> bits) & 0x1f) as usize] as char);
        }
    }

    if bits != 0 {
        let value = (accumulator << (5 - bits)) & 0x1f;
        output.push(alphabet[value as usize] as char);
    }
}