#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidByte { offset: usize, byte: u8 },
    BadPadding { offset: usize },
    Truncated { offset: usize },
    AlreadyFinished,
}

const ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn digit(byte: u8) -> Option<u8> {
    ALPHABET.iter().position(|&b| b == byte).map(|n| n as u8)
}

fn encode_group(a: u8, b: u8, c: u8) -> [u8; 4] {
    [
        ALPHABET[(a >> 2) as usize],
        ALPHABET[(((a & 0x03) << 4) | (b >> 4)) as usize],
        ALPHABET[(((b & 0x0f) << 2) | (c >> 6)) as usize],
        ALPHABET[(c & 0x3f) as usize],
    ]
}

pub fn encode(input: &[u8]) -> String {
    let mut encoder = Encoder::new();
    let mut output = encoder.push(input).expect("new encoder is open");
    output.push_str(&encoder.finish().expect("new encoder is open"));
    output
}

pub fn decode(input: &[u8]) -> Result<Vec<u8>, Error> {
    let mut decoder = Decoder::new();
    let mut output = decoder.push(input)?;
    output.extend(decoder.finish()?);
    Ok(output)
}

pub struct Encoder {
    tail: Vec<u8>,
    finished: bool,
}

impl Encoder {
    pub fn new() -> Self {
        Self {
            tail: Vec::new(),
            finished: false,
        }
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }

        let mut bytes = Vec::with_capacity(self.tail.len() + input.len());
        bytes.extend_from_slice(&self.tail);
        bytes.extend_from_slice(input);

        let complete_len = bytes.len() / 3 * 3;
        let mut output = Vec::with_capacity(complete_len / 3 * 4);
        for chunk in bytes[..complete_len].chunks_exact(3) {
            output.extend_from_slice(&encode_group(chunk[0], chunk[1], chunk[2]));
        }

        self.tail.clear();
        self.tail.extend_from_slice(&bytes[complete_len..]);
        Ok(String::from_utf8(output).expect("alphabet is ASCII"))
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        self.finished = true;

        let output = match self.tail.as_slice() {
            [] => Vec::new(),
            [a] => vec![
                ALPHABET[(a >> 2) as usize],
                ALPHABET[((a & 0x03) << 4) as usize],
                b'=',
                b'=',
            ],
            [a, b] => vec![
                ALPHABET[(a >> 2) as usize],
                ALPHABET[(((a & 0x03) << 4) | (b >> 4)) as usize],
                ALPHABET[((b & 0x0f) << 2) as usize],
                b'=',
            ],
            _ => unreachable!(),
        };
        self.tail.clear();
        Ok(String::from_utf8(output).expect("alphabet is ASCII"))
    }
}

pub struct Decoder {
    group: Vec<(u8, usize)>,
    offset: usize,
    terminal_padding: bool,
    pending_second_padding: Option<usize>,
    sticky_error: Option<Error>,
    finished: bool,
}

impl Decoder {
    pub fn new() -> Self {
        Self {
            group: Vec::with_capacity(4),
            offset: 0,
            terminal_padding: false,
            pending_second_padding: None,
            sticky_error: None,
            finished: false,
        }
    }

    pub fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky_error {
            return Err(error.clone());
        }
        if self.finished {
            return Err(Error::AlreadyFinished);
        }

        let mut output = Vec::new();

        for &byte in input {
            let current_offset = self.offset;
            self.offset += 1;

            if self.terminal_padding {
                return self.fail(Error::BadPadding {
                    offset: current_offset,
                });
            }

            if self.pending_second_padding.is_some() && byte != b'=' {
                return self.fail(Error::BadPadding {
                    offset: current_offset,
                });
            }

            let position = self.group.len();
            if byte == b'=' {
                match position {
                    0 | 1 => {
                        return self.fail(Error::BadPadding {
                            offset: current_offset,
                        });
                    }
                    2 => {
                        self.pending_second_padding = Some(current_offset);
                        self.group.push((byte, current_offset));
                    }
                    3 => {
                        self.group.push((byte, current_offset));
                        output.extend(decode_group(&self.group));
                        self.group.clear();
                        self.pending_second_padding = None;
                        self.terminal_padding = true;
                    }
                    _ => unreachable!(),
                }
                continue;
            }

            let Some(value) = digit(byte) else {
                return self.fail(Error::InvalidByte {
                    offset: current_offset,
                    byte,
                });
            };

            self.group.push((value, current_offset));
            if self.group.len() == 4 {
                output.extend(decode_group(&self.group));
                self.group.clear();
            }
        }

        Ok(output)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky_error {
            return Err(error.clone());
        }
        if self.finished {
            return Err(Error::AlreadyFinished);
        }

        if let Some(offset) = self.pending_second_padding {
            return self.fail(Error::BadPadding { offset });
        }
        if !self.group.is_empty() {
            return self.fail(Error::Truncated {
                offset: self.group[0].1,
            });
        }

        self.finished = true;
        Ok(Vec::new())
    }

    fn fail<T>(&mut self, error: Error) -> Result<T, Error> {
        self.sticky_error = Some(error.clone());
        Err(error)
    }
}

fn decode_group(group: &[(u8, usize)]) -> Vec<u8> {
    match group {
        [(a, _), (b, _), (c, _), (d, _)] if *c == b'=' && *d == b'=' => {
            vec![(a << 2) | (b >> 4)]
        }
        [(a, _), (b, _), (c, _), (d, _)] if *d == b'=' => {
            vec![(a << 2) | (b >> 4), (b << 4) | (c >> 2)]
        }
        [(a, _), (b, _), (c, _), (d, _)] => vec![
            (a << 2) | (b >> 4),
            (b << 4) | (c >> 2),
            (c << 6) | d,
        ],
        _ => unreachable!(),
    }
}

fn main() {}