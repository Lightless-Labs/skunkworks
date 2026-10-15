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

        let mut all = self.pending.clone();
        all.extend_from_slice(input);

        let complete_len = (all.len() / 5) * 5;
        let mut output = String::new();
        encode_bytes(
            &all[..complete_len],
            self.config.alphabet,
            &mut output,
        );

        self.pending = all[complete_len..].to_vec();
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<String, StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }

        let mut output = String::new();
        if !self.pending.is_empty() {
            let data_symbols = (self.pending.len() * 8 + 4) / 5;
            encode_partial(&self.pending, self.config.alphabet, &mut output);

            if self.config.padding == Padding::Required {
                while output.len() % 8 != 0 {
                    output.push('=');
                }
            }

            debug_assert!(data_symbols <= output.len());
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
    for chunk in bytes.chunks_exact(5) {
        let b0 = chunk[0] as u32;
        let b1 = chunk[1] as u32;
        let b2 = chunk[2] as u32;
        let b3 = chunk[3] as u32;
        let b4 = chunk[4] as u32;

        let values = [
            (b0 >> 3) & 31,
            ((b0 << 2) | (b1 >> 6)) & 31,
            (b1 >> 1) & 31,
            ((b1 << 4) | (b2 >> 4)) & 31,
            ((b2 << 1) | (b3 >> 7)) & 31,
            (b3 >> 2) & 31,
            ((b3 << 3) | (b4 >> 5)) & 31,
            b4 & 31,
        ];

        for value in values {
            output.push(table[value as usize] as char);
        }
    }
}

fn encode_partial(bytes: &[u8], alphabet: Alphabet, output: &mut String) {
    let table = alphabet_bytes(alphabet);
    let mut acc: u32 = 0;
    for &byte in bytes {
        acc = (acc << 8) | byte as u32;
    }
    let bit_len = bytes.len() * 8;
    let symbol_count = (bit_len + 4) / 5;

    for i in 0..symbol_count {
        let shift = bit_len as isize - ((i + 1) * 5) as isize;
        let value = if shift >= 0 {
            (acc >> shift as usize) & 31
        } else {
            (acc << (-shift) as usize) & 31
        };
        output.push(table[value as usize] as char);
    }
}

pub struct Decoder {
    config: Config,
    position: usize,
    data_count: usize,
    pending_bits: u32,
    bit_count: u8,
    last_data_offset: Option<usize>,
    first_padding_offset: Option<usize>,
    accepted_padding: usize,
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
            bit_count: 0,
            last_data_offset: None,
            first_padding_offset: None,
            accepted_padding: 0,
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

                let expected = next.expected_padding.unwrap();
                if next.accepted_padding >= expected {
                    return Err(StreamError::Padding { offset });
                }
                next.accepted_padding += 1;
                next.position += 1;
                continue;
            }

            let (value, canonical) = match classify_data(byte, next.config.alphabet) {
                Some(v) => v,
                None => {
                    return Err(StreamError::InvalidByte { offset, byte });
                }
            };

            if next.config.canonical && !canonical {
                return Err(StreamError::NonCanonical {
                    offset,
                    expected: byte.to_ascii_uppercase(),
                });
            }

            if next.first_padding_offset.is_some() {
                return Err(StreamError::Padding { offset });
            }

            next.data_count += 1;
            next.last_data_offset = Some(offset);
            next.pending_bits = (next.pending_bits << 5) | value as u32;
            next.bit_count += 5;
            while next.bit_count >= 8 {
                next.bit_count -= 8;
                output.push(((next.pending_bits >> next.bit_count) & 0xff) as u8);
                if next.bit_count == 0 {
                    next.pending_bits = 0;
                } else {
                    next.pending_bits &= (1u32 << next.bit_count) - 1;
                }
            }
            next.position += 1;
        }

        *self = next.into_decoder();
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, StreamError> {
        if self.closed {
            return Err(StreamError::Closed);
        }

        let residue = self.data_count % 8;
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

        let unused = match residue {
            0 => 0,
            2 => 2,
            4 => 4,
            5 => 1,
            7 => 3,
            _ => unreachable!(),
        };
        if unused != 0 && self.pending_bits != 0 {
            return Err(StreamError::TrailingBits {
                offset: self.last_data_offset.unwrap_or(self.position),
            });
        }

        self.closed = true;
        Ok(Vec::new())
    }
}

#[derive(Clone)]
struct DecoderState {
    config: Config,
    position: usize,
    data_count: usize,
    pending_bits: u32,
    bit_count: u8,
    last_data_offset: Option<usize>,
    first_padding_offset: Option<usize>,
    accepted_padding: usize,
    expected_padding: Option<usize>,
    closed: bool,
}

impl Decoder {
    fn clone_state(&self) -> DecoderState {
        DecoderState {
            config: self.config,
            position: self.position,
            data_count: self.data_count,
            pending_bits: self.pending_bits,
            bit_count: self.bit_count,
            last_data_offset: self.last_data_offset,
            first_padding_offset: self.first_padding_offset,
            accepted_padding: self.accepted_padding,
            expected_padding: self.expected_padding,
            closed: self.closed,
        }
    }
}

impl DecoderState {
    fn into_decoder(self) -> Decoder {
        Decoder {
            config: self.config,
            position: self.position,
            data_count: self.data_count,
            pending_bits: self.pending_bits,
            bit_count: self.bit_count,
            last_data_offset: self.last_data_offset,
            first_padding_offset: self.first_padding_offset,
            accepted_padding: self.accepted_padding,
            expected_padding: self.expected_padding,
            closed: self.closed,
        }
    }
}

fn classify_data(byte: u8, alphabet: Alphabet) -> Option<(u8, bool)> {
    let upper = byte.to_ascii_uppercase();
    let canonical = byte == upper;
    let table = alphabet_bytes(alphabet);
    table
        .iter()
        .position(|&candidate| candidate == upper)
        .map(|value| (value as u8, canonical))
}

fn main() {}