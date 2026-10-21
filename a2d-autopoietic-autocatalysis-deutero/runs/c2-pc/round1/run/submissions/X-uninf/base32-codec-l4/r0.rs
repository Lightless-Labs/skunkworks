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

const STANDARD: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
const HEX: &[u8; 32] = b"0123456789ABCDEFGHIJKLMNOPQRSTUV";

fn alphabet_bytes(alphabet: Alphabet) -> &'static [u8; 32] {
    match alphabet {
        Alphabet::Standard => STANDARD,
        Alphabet::Hex => HEX,
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

        let mut combined = self.pending.clone();
        combined.extend_from_slice(input);
        let complete_len = (combined.len() / 5) * 5;
        let alphabet = alphabet_bytes(self.config.alphabet);
        let mut output = String::with_capacity((complete_len / 5) * 8);

        for block in combined[..complete_len].chunks_exact(5) {
            let mut acc = 0u64;
            for &byte in block {
                acc = (acc << 8) | u64::from(byte);
            }
            for shift in (0..8).rev() {
                let value = ((acc >> (shift * 5)) & 31) as usize;
                output.push(alphabet[value] as char);
            }
        }

        self.pending = combined[complete_len..].to_vec();
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<String, StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }

        let alphabet = alphabet_bytes(self.config.alphabet);
        let mut output = String::new();

        if !self.pending.is_empty() {
            let bit_count = self.pending.len() * 8;
            let symbol_count = (bit_count + 4) / 5;
            let mut acc = 0u64;
            for &byte in &self.pending {
                acc = (acc << 8) | u64::from(byte);
            }
            acc <<= symbol_count * 5 - bit_count;

            for index in 0..symbol_count {
                let shift = (symbol_count - 1 - index) * 5;
                let value = ((acc >> shift) & 31) as usize;
                output.push(alphabet[value] as char);
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
    bit_buffer: u64,
    bit_count: u8,
    data_count: usize,
    last_data_offset: Option<usize>,
    total_consumed: usize,
    first_pad_offset: Option<usize>,
    pad_count: usize,
    expected_pads: usize,
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
                last_data_offset: None,
                total_consumed: 0,
                first_pad_offset: None,
                pad_count: 0,
                expected_pads: 0,
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
        let alphabet = alphabet_bytes(self.config.alphabet);

        for &byte in input.as_bytes() {
            let offset = state.total_consumed;
            let recognized = if byte == b'=' {
                Some(None)
            } else {
                let upper = byte.to_ascii_uppercase();
                alphabet
                    .iter()
                    .position(|&symbol| symbol == upper)
                    .map(|value| Some((value as u8, upper)))
            };

            let recognized = match recognized {
                Some(value) => value,
                None => {
                    return Err(StreamError::InvalidByte { offset, byte });
                }
            };

            if let Some((value, upper)) = recognized {
                if byte != upper && self.config.canonical {
                    return Err(StreamError::NonCanonical {
                        offset,
                        expected: upper,
                    });
                }

                if state.first_pad_offset.is_some() {
                    return Err(StreamError::Padding { offset });
                }

                state.data_count = state.data_count.checked_add(1).ok_or(
                    StreamError::InvalidLength {
                        offset: state.total_consumed,
                    },
                )?;
                state.last_data_offset = Some(offset);
                state.bit_buffer = (state.bit_buffer << 5) | u64::from(value);
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
            } else {
                if self.config.padding == Padding::Omitted {
                    return Err(StreamError::Padding { offset });
                }

                if state.first_pad_offset.is_none() {
                    let residue = state.data_count % 8;
                    state.expected_pads = match residue {
                        2 => 6,
                        4 => 4,
                        5 => 3,
                        7 => 1,
                        _ => return Err(StreamError::Padding { offset }),
                    };
                    state.first_pad_offset = Some(offset);
                }

                if state.pad_count >= state.expected_pads {
                    return Err(StreamError::Padding { offset });
                }
                state.pad_count += 1;
            }

            state.total_consumed = state.total_consumed.checked_add(1).ok_or(
                StreamError::InvalidLength {
                    offset: state.total_consumed,
                },
            )?;
        }

        self.state = state;
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, StreamError> {
        if self.state.closed {
            return Err(StreamError::Closed);
        }

        let state = &self.state;
        let residue = state.data_count % 8;
        if !matches!(residue, 0 | 2 | 4 | 5 | 7) {
            return Err(StreamError::InvalidLength {
                offset: state.total_consumed,
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
            if state.pad_count != expected {
                return Err(StreamError::Padding {
                    offset: state.first_pad_offset.unwrap_or(state.total_consumed),
                });
            }
        }

        let unused = match residue {
            0 => 0,
            2 => 2,
            4 => 4,
            5 => 1,
            7 => 3,
            _ => unreachable!(),
        };
        if unused != 0 && state.bit_buffer & ((1u64 << unused) - 1) != 0 {
            return Err(StreamError::TrailingBits {
                offset: state
                    .last_data_offset
                    .unwrap_or(state.total_consumed),
            });
        }

        self.state.closed = true;
        Ok(Vec::new())
    }
}