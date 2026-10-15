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

        let mut out = String::new();
        let mut combined = Vec::with_capacity(self.pending.len() + input.len());
        combined.extend_from_slice(&self.pending);
        combined.extend_from_slice(input);

        let complete_len = combined.len() / 5 * 5;
        for chunk in combined[..complete_len].chunks_exact(5) {
            encode_symbols(chunk, self.config.alphabet, &mut out);
        }

        self.pending.clear();
        self.pending.extend_from_slice(&combined[complete_len..]);
        Ok(out)
    }

    pub fn finish(&mut self) -> Result<String, StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }

        let mut out = String::new();
        if !self.pending.is_empty() {
            let symbols = encode_symbols(&self.pending, self.config.alphabet, &mut out);
            if self.config.padding == Padding::Required {
                let pad_count = (8 - symbols % 8) % 8;
                out.extend(std::iter::repeat_n('=', pad_count));
            }
        }

        self.pending.clear();
        self.closed = true;
        Ok(out)
    }
}

fn alphabet_byte(alphabet: Alphabet, value: u8) -> u8 {
    match alphabet {
        Alphabet::Standard => b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"[value as usize],
        Alphabet::Hex => b"0123456789ABCDEFGHIJKLMNOPQRSTUV"[value as usize],
    }
}

fn encode_symbols(bytes: &[u8], alphabet: Alphabet, out: &mut String) -> usize {
    let bit_count = bytes.len() * 8;
    let symbol_count = bit_count.div_ceil(5);
    let mut accumulator = 0u32;
    let mut bits = 0usize;

    for &byte in bytes {
        accumulator = (accumulator << 8) | byte as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            let value = ((accumulator >> bits) & 31) as u8;
            out.push(alphabet_byte(alphabet, value) as char);
        }
    }

    if bits != 0 {
        let value = ((accumulator << (5 - bits)) & 31) as u8;
        out.push(alphabet_byte(alphabet, value) as char);
    }

    symbol_count
}

#[derive(Clone)]
struct DecoderState {
    position: usize,
    accumulator: u32,
    pending_bits: u8,
    data_count: usize,
    last_data_offset: Option<usize>,
    first_padding_offset: Option<usize>,
    padding_count: usize,
    expected_padding: Option<usize>,
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
                accumulator: 0,
                pending_bits: 0,
                data_count: 0,
                last_data_offset: None,
                first_padding_offset: None,
                padding_count: 0,
                expected_padding: None,
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
                    let expected = match residue {
                        2 => 6,
                        4 => 4,
                        5 => 3,
                        7 => 1,
                        _ => return Err(StreamError::Padding { offset }),
                    };
                    state.first_padding_offset = Some(offset);
                    state.expected_padding = Some(expected);
                }

                if state.padding_count >= state.expected_padding.unwrap_or(0) {
                    return Err(StreamError::Padding { offset });
                }
                state.padding_count += 1;
                state.position += 1;
                continue;
            }

            let (value, canonical_byte) = match classify_data(self.config.alphabet, byte) {
                Some(v) => v,
                None => return Err(StreamError::InvalidByte { offset, byte }),
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

            state.accumulator = (state.accumulator << 5) | value as u32;
            state.pending_bits += 5;
            state.data_count += 1;
            state.last_data_offset = Some(offset);

            if state.pending_bits >= 8 {
                state.pending_bits -= 8;
                output.push(((state.accumulator >> state.pending_bits) & 0xff) as u8);
                if state.pending_bits == 0 {
                    state.accumulator = 0;
                } else {
                    state.accumulator &= (1u32 << state.pending_bits) - 1;
                }
            }

            state.position += 1;
        }

        self.state = state;
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, StreamError> {
        if self.state.closed {
            return Err(StreamError::Closed);
        }

        let residue = self.state.data_count % 8;
        if !matches!(residue, 0 | 2 | 4 | 5 | 7) {
            return Err(StreamError::InvalidLength {
                offset: self.state.position,
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
            if self.state.padding_count != expected {
                return Err(StreamError::Padding {
                    offset: self
                        .state
                        .first_padding_offset
                        .unwrap_or(self.state.position),
                });
            }
        }

        let unused_bits = match residue {
            2 => 2,
            4 => 4,
            5 => 1,
            7 => 3,
            _ => 0,
        };
        if unused_bits != 0 && self.state.accumulator & ((1u32 << unused_bits) - 1) != 0 {
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

fn classify_data(alphabet: Alphabet, byte: u8) -> Option<(u8, u8)> {
    let upper = byte.to_ascii_uppercase();
    let value = match alphabet {
        Alphabet::Standard => match upper {
            b'A'..=b'Z' => upper - b'A',
            b'2'..=b'7' => upper - b'2' + 26,
            _ => return None,
        },
        Alphabet::Hex => match upper {
            b'0'..=b'9' => upper - b'0',
            b'A'..=b'V' => upper - b'A' + 10,
            _ => return None,
        },
    };
    Some((value, upper))
}

fn main() {}