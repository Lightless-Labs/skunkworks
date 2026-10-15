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

        let mut bytes = Vec::with_capacity(self.pending.len() + input.len());
        bytes.extend_from_slice(&self.pending);
        bytes.extend_from_slice(input);

        let complete_len = bytes.len() / 5 * 5;
        let mut output = String::new();
        for chunk in bytes[..complete_len].chunks_exact(5) {
            encode_bytes(chunk, self.config.alphabet, &mut output);
        }

        self.pending = bytes[complete_len..].to_vec();
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
                for _ in symbols..8 {
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

fn encode_bytes(bytes: &[u8], alphabet: Alphabet, output: &mut String) -> usize {
    let table = alphabet_bytes(alphabet);
    let bit_len = bytes.len() * 8;
    let symbols = (bit_len + 4) / 5;

    for symbol_index in 0..symbols {
        let mut value = 0u8;
        for bit_index in 0..5 {
            let bit_position = symbol_index * 5 + bit_index;
            value <<= 1;
            if bit_position < bit_len {
                let byte = bytes[bit_position / 8];
                let bit = (byte >> (7 - bit_position % 8)) & 1;
                value |= bit;
            }
        }
        output.push(table[value as usize] as char);
    }
    symbols
}

#[derive(Clone)]
struct DecoderState {
    bit_buffer: u64,
    bit_count: u8,
    data_count: usize,
    last_value: u8,
    last_data_offset: Option<usize>,
    first_padding_offset: Option<usize>,
    padding_count: usize,
    expected_padding: usize,
    position: usize,
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
                bit_buffer: 0,
                bit_count: 0,
                data_count: 0,
                last_value: 0,
                last_data_offset: None,
                first_padding_offset: None,
                padding_count: 0,
                expected_padding: 0,
                position: 0,
                closed: false,
            },
        }
    }

    pub fn push(&mut self, input: &str) -> Result<Vec<u8>, StreamError> {
        if self.state.closed {
            return Err(StreamError::Closed);
        }

        let mut state = self.state.clone();
        let mut output = Vec::new();

        for &byte in input.as_bytes() {
            let offset = state.position;
            if byte == b'=' {
                if self.config.padding == Padding::Omitted {
                    return Err(StreamError::Padding { offset });
                }

                if state.first_padding_offset.is_none() {
                    let residue = state.data_count % 8;
                    let Some(expected) = expected_padding(residue) else {
                        return Err(StreamError::Padding { offset });
                    };
                    if expected == 0 {
                        return Err(StreamError::Padding { offset });
                    }
                    state.first_padding_offset = Some(offset);
                    state.expected_padding = expected;
                }

                if state.padding_count >= state.expected_padding {
                    return Err(StreamError::Padding { offset });
                }
                state.padding_count += 1;
            } else {
                let Some((value, canonical_byte)) = decode_symbol(byte, self.config.alphabet) else {
                    return Err(StreamError::InvalidByte { offset, byte });
                };

                if self.config.canonical && byte != canonical_byte {
                    return Err(StreamError::NonCanonical {
                        offset,
                        expected: canonical_byte,
                    });
                }

                if state.first_padding_offset.is_some() {
                    return Err(StreamError::Padding { offset });
                }

                state.data_count = state.data_count.checked_add(1).ok_or(
                    StreamError::InvalidLength { offset },
                )?;
                state.last_value = value;
                state.last_data_offset = Some(offset);

                state.bit_buffer = (state.bit_buffer << 5) | value as u64;
                state.bit_count += 5;
                while state.bit_count >= 8 {
                    state.bit_count -= 8;
                    output.push(((state.bit_buffer >> state.bit_count) & 0xff) as u8);
                    if state.bit_count == 0 {
                        state.bit_buffer = 0;
                    } else {
                        state.bit_buffer &= (1u64 << state.bit_count) - 1;
                    }
                }
            }

            state.position = state.position.checked_add(1).ok_or(
                StreamError::InvalidLength { offset },
            )?;
        }

        self.state = state;
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, StreamError> {
        if self.state.closed {
            return Err(StreamError::Closed);
        }

        let residue = self.state.data_count % 8;
        let Some(expected_padding) = expected_padding(residue) else {
            return Err(StreamError::InvalidLength {
                offset: self.state.position,
            });
        };

        if self.config.padding == Padding::Required
            && self.state.padding_count != expected_padding
        {
            return Err(StreamError::Padding {
                offset: self
                    .state
                    .first_padding_offset
                    .unwrap_or(self.state.position),
            });
        }

        let unused_bits = match residue {
            0 => 0,
            2 => 2,
            4 => 4,
            5 => 1,
            7 => 3,
            _ => unreachable!(),
        };

        if unused_bits != 0 {
            let mask = (1u8 << unused_bits) - 1;
            if self.state.last_value & mask != 0 {
                return Err(StreamError::TrailingBits {
                    offset: self.state.last_data_offset.unwrap_or(self.state.position),
                });
            }
        }

        self.state.closed = true;
        Ok(Vec::new())
    }
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

fn decode_symbol(byte: u8, alphabet: Alphabet) -> Option<(u8, u8)> {
    let upper = byte.to_ascii_uppercase();
    let table = alphabet_bytes(alphabet);
    table
        .iter()
        .position(|&symbol| symbol == upper)
        .map(|value| (value as u8, upper))
}

fn main() {}