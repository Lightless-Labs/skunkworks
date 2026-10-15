#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidByte { offset: usize, byte: u8 },
    BadPadding { offset: usize },
    Truncated { offset: usize },
    AlreadyFinished,
}

const ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn value(byte: u8) -> Option<u8> {
    ALPHABET.iter().position(|&b| b == byte).map(|n| n as u8)
}

pub fn encode(input: &[u8]) -> String {
    let mut encoder = Encoder::new();
    let mut output = encoder.push(input).expect("new encoder is open");
    output.push_str(&encoder.finish().expect("encoder is open"));
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
        let mut output = String::with_capacity(complete_len / 3 * 4);
        for chunk in bytes[..complete_len].chunks_exact(3) {
            let a = chunk[0];
            let b = chunk[1];
            let c = chunk[2];
            output.push(ALPHABET[(a >> 2) as usize] as char);
            output.push(ALPHABET[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);
            output.push(ALPHABET[(((b & 0x0f) << 2) | (c >> 6)) as usize] as char);
            output.push(ALPHABET[(c & 0x3f) as usize] as char);
        }

        self.tail.clear();
        self.tail.extend_from_slice(&bytes[complete_len..]);
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        self.finished = true;

        let mut output = String::new();
        match self.tail.as_slice() {
            [] => {}
            [a] => {
                output.push(ALPHABET[(a >> 2) as usize] as char);
                output.push(ALPHABET[((a & 0x03) << 4) as usize] as char);
                output.push_str("==");
            }
            [a, b] => {
                output.push(ALPHABET[(a >> 2) as usize] as char);
                output.push(ALPHABET[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);
                output.push(ALPHABET[((b & 0x0f) << 2) as usize] as char);
                output.push('=');
            }
            _ => unreachable!(),
        }
        self.tail.clear();
        Ok(output)
    }
}

#[derive(Clone, Copy)]
struct Symbol {
    byte: u8,
    offset: usize,
}

pub struct Decoder {
    group: Vec<Symbol>,
    total_offset: usize,
    padded: bool,
    finished: bool,
    sticky_error: Option<Error>,
}

impl Decoder {
    pub fn new() -> Self {
        Self {
            group: Vec::with_capacity(4),
            total_offset: 0,
            padded: false,
            finished: false,
            sticky_error: None,
        }
    }

    fn fail(&mut self, error: Error) -> Error {
        self.sticky_error = Some(error.clone());
        error
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
            let offset = self.total_offset;
            self.total_offset += 1;

            if self.padded {
                return Err(self.fail(Error::BadPadding { offset }));
            }

            if self.group.len() == 2 && self.group[1].byte == b'=' {
                if byte != b'=' {
                    return Err(self.fail(Error::BadPadding { offset }));
                }
                self.group.push(Symbol { byte, offset });
                self.padded = true;
                output.push((value(self.group[0].byte).unwrap() << 2)
                    | (value(self.group[1].byte).unwrap() >> 4));
                self.group.clear();
                continue;
            }

            let position = self.group.len();
            if byte == b'=' {
                match position {
                    0 | 1 => return Err(self.fail(Error::BadPadding { offset })),
                    2 => self.group.push(Symbol { byte, offset }),
                    3 => {
                        let a = value(self.group[0].byte).unwrap();
                        let b = value(self.group[1].byte).unwrap();
                        let c = value(self.group[2].byte).unwrap();
                        output.push((a << 2) | (b >> 4));
                        output.push((b << 4) | (c >> 2));
                        self.group.clear();
                        self.padded = true;
                    }
                    _ => unreachable!(),
                }
                continue;
            }

            let Some(digit) = value(byte) else {
                return Err(self.fail(Error::InvalidByte { offset, byte }));
            };
            self.group.push(Symbol { byte, offset });

            if self.group.len() == 4 {
                let a = value(self.group[0].byte).unwrap();
                let b = value(self.group[1].byte).unwrap();
                let c = value(self.group[2].byte).unwrap();
                let d = digit;
                output.push((a << 2) | (b >> 4));
                output.push((b << 4) | (c >> 2));
                output.push((c << 6) | d);
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

        if self.group.len() == 1 {
            let error = Error::Truncated {
                offset: self.group[0].offset,
            };
            return Err(self.fail(error));
        }
        if self.group.len() == 2 && self.group[1].byte == b'=' {
            let error = Error::BadPadding {
                offset: self.group[1].offset,
            };
            return Err(self.fail(error));
        }
        if !self.group.is_empty() {
            let error = Error::Truncated {
                offset: self.group[0].offset,
            };
            return Err(self.fail(error));
        }

        self.finished = true;
        Ok(Vec::new())
    }
}

fn main() {}