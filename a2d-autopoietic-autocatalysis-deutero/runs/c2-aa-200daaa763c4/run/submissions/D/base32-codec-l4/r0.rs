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

fn alphabet_bytes(alphabet: Alphabet) -> &'static [u8; 32] {
    match alphabet {
        Alphabet::Standard => b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567",
        Alphabet::Hex => b"0123456789ABCDEFGHIJKLMNOPQRSTUV",
    }
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

        let mut bytes = std::mem::take(&mut self.pending);
        bytes.extend_from_slice(input);

        let complete_len = (bytes.len() / 5) * 5;
        let mut output = String::new();
        let alphabet = alphabet_bytes(self.config.alphabet);

        for chunk in bytes[..complete_len].chunks_exact(5) {
            let n = ((chunk[0] as u64) << 32)
                | ((chunk[1] as u64) << 24)
                | ((chunk[2] as u64) << 16)
                | ((chunk[3] as u64) << 8)
                | chunk[4] as u64;

            for shift in (0..8).rev() {
                output.push(alphabet[((n >> (shift * 5)) & 31) as usize] as char);
            }
        }

        self.pending.extend_from_slice(&bytes[complete_len..]);
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<String, StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }

        let mut output = String::new();
        let alphabet = alphabet_bytes(self.config.alphabet);

        if !self.pending.is_empty() {
            let mut n = 0u64;
            for &byte in &self.pending {
                n = (n << 8) | byte as u64;
            }

            let bit_len = self.pending.len() * 8;
            let symbols = (bit_len + 4) / 5;
            n <<= 40 - bit_len;

            for index in 0..symbols {
                let shift = 35 - index * 5;
                output.push(alphabet[((n >> shift) & 31) as usize] as char);
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

#[derive(Clone)]
struct DecoderState {
    position: usize,
    data_count: usize,
    accepted_padding: usize,
    first_padding_offset: Option<usize>,
    bit_buffer: u64,
    bit_count: usize,
    last_data_offset: Option<usize>,
    closed: bool,
}

pub struct Decoder {
    config: Config,
    state: DecoderState,
}

impl Decoder {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            state: DecoderState {
                position: 0,
                data_count: 0,
                accepted_padding: 0,
                first_padding_offset: None,
                bit_buffer: 0,
                bit_count: 0,
                last_data_offset: None,
                closed: false,
            },
        }
    }

    pub fn push(&mut self, input: &str) -> Result<Vec<u8>, StreamError> {
        if self.state.closed {
            return Err(StreamError::Closed);
        }

        let mut next = self.state.clone();
        let mut output = Vec::new();

        for &byte in input.as_bytes() {
            let offset = next.position;

            if byte == b'=' {
                if self.config.padding == Padding::Omitted {
                    return Err(StreamError::Padding { offset });
                }

                let expected_padding = match next.data_count % 8 {
                    2 => 6,
                    4 => 4,
                    5 => 3,
                    7 => 1,
                    _ => return Err(StreamError::Padding { offset }),
                };

                if next.first_padding_offset.is_none() {
                    next.first_padding_offset = Some(offset);
                    next.accepted_padding = 1;
                } else if next.accepted_padding >= expected_padding {
                    return Err(StreamError::Padding { offset });
                } else {
                    next.accepted_padding += 1;
                }
            } else {
                let upper = byte.to_ascii_uppercase();
                let value = alphabet_bytes(self.config.alphabet)
                    .iter()
                    .position(|&symbol| symbol == upper)
                    .ok_or(StreamError::InvalidByte { offset, byte })?
                    as u8;

                if self.config.canonical && byte != upper {
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
                    output.push(((next.bit_buffer >> next.bit_count) & 0xff) as u8);
                    if next.bit_count == 0 {
                        next.bit_buffer = 0;
                    } else {
                        next.bit_buffer &= (1u64 << next.bit_count) - 1;
                    }
                }
            }

            next.position += 1;
        }

        self.state = next;
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, StreamError> {
        if self.state.closed {
            return Err(StreamError::Closed);
        }

        let residue = self.state.data_count % 8;
        let expected_padding = match residue {
            0 => Some(0),
            2 => Some(6),
            4 => Some(4),
            5 => Some(3),
            7 => Some(1),
            _ => None,
        };

        let Some(expected_padding) = expected_padding else {
            return Err(StreamError::InvalidLength {
                offset: self.state.position,
            });
        };

        if self.config.padding == Padding::Required
            && self.state.accepted_padding != expected_padding
        {
            return Err(StreamError::Padding {
                offset: self
                    .state
                    .first_padding_offset
                    .unwrap_or(self.state.position),
            });
        }

        let unused_bits = match residue {
            2 => 2,
            4 => 4,
            5 => 1,
            7 => 3,
            _ => 0,
        };

        if unused_bits != 0
            && self.state.bit_buffer & ((1u64 << unused_bits) - 1) != 0
        {
            return Err(StreamError::TrailingBits {
                offset: self
                    .state
                    .last_data_offset
                    .unwrap_or(self.state.position),
            });
        }

        self.state.closed = true;
        Ok(Vec::new())
    }
}