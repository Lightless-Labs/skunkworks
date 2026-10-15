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
    ALPHABET.iter().position(|&b| b == byte).map(|i| i as u8)
}

fn encode_group(input: &[u8]) -> String {
    let a = input[0];
    let b = if input.len() > 1 { input[1] } else { 0 };
    let c = if input.len() > 2 { input[2] } else { 0 };

    let mut out = String::with_capacity(4);
    out.push(ALPHABET[(a >> 2) as usize] as char);
    out.push(ALPHABET[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);
    if input.len() > 1 {
        out.push(ALPHABET[(((b & 0x0f) << 2) | (c >> 6)) as usize] as char);
    } else {
        out.push('=');
    }
    if input.len() > 2 {
        out.push(ALPHABET[(c & 0x3f) as usize] as char);
    } else {
        out.push('=');
    }
    out
}

pub fn encode(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    let mut chunks = input.chunks_exact(3);
    for chunk in &mut chunks {
        out.push_str(&encode_group(chunk));
    }
    let tail = chunks.remainder();
    if !tail.is_empty() {
        out.push_str(&encode_group(tail));
    }
    out
}

pub struct Encoder {
    buffer: Vec<u8>,
    finished: bool,
}

impl Encoder {
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            finished: false,
        }
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        self.buffer.extend_from_slice(input);
        let complete_len = self.buffer.len() / 3 * 3;
        let mut out = String::with_capacity(complete_len / 3 * 4);
        for chunk in self.buffer[..complete_len].chunks_exact(3) {
            out.push_str(&encode_group(chunk));
        }
        self.buffer.drain(..complete_len);
        Ok(out)
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        self.finished = true;
        if self.buffer.is_empty() {
            Ok(String::new())
        } else {
            Ok(encode_group(&self.buffer))
        }
    }
}

#[derive(Clone, Copy)]
struct Symbol {
    byte: u8,
    offset: usize,
}

pub struct Decoder {
    group: Vec<Symbol>,
    offset: usize,
    padded: bool,
    finished: bool,
    error: Option<Error>,
}

impl Decoder {
    pub fn new() -> Self {
        Self {
            group: Vec::with_capacity(4),
            offset: 0,
            padded: false,
            finished: false,
            error: None,
        }
    }

    fn fail(&mut self, error: Error) -> Result<Vec<u8>, Error> {
        self.error = Some(error.clone());
        Err(error)
    }

    pub fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        if self.finished {
            return Err(Error::AlreadyFinished);
        }

        let mut output = Vec::new();
        for &byte in input {
            let current_offset = self.offset;
            self.offset += 1;

            if self.padded {
                return self.fail(Error::BadPadding {
                    offset: current_offset,
                });
            }
            if self.group.len() == 3 && self.group[2].byte == b'=' && byte != b'=' {
                return self.fail(Error::BadPadding {
                    offset: current_offset,
                });
            }

            let position = self.group.len();
            if byte == b'=' {
                if position < 2 {
                    return self.fail(Error::BadPadding {
                        offset: current_offset,
                    });
                }
            } else if digit(byte).is_none() {
                return self.fail(Error::InvalidByte {
                    offset: current_offset,
                    byte,
                });
            }

            self.group.push(Symbol {
                byte,
                offset: current_offset,
            });
            if self.group.len() == 4 {
                let a = digit(self.group[0].byte).unwrap();
                let b = digit(self.group[1].byte).unwrap();
                output.push((a << 2) | (b >> 4));

                match (self.group[2].byte, self.group[3].byte) {
                    (b'=', b'=') => {
                        self.padded = true;
                    }
                    (b'=', _) => {
                        return self.fail(Error::BadPadding {
                            offset: self.group[3].offset,
                        });
                    }
                    (third, b'=') => {
                        let c = digit(third).unwrap();
                        output.push((b << 4) | (c >> 2));
                        self.padded = true;
                    }
                    (third, fourth) => {
                        let c = digit(third).unwrap();
                        let d = digit(fourth).unwrap();
                        output.push((b << 4) | (c >> 2));
                        output.push((c << 6) | d);
                    }
                }
                self.group.clear();
            }
        }
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        if self.group.len() == 3 && self.group[2].byte == b'=' {
            return self.fail(Error::BadPadding {
                offset: self.group[2].offset,
            });
        }
        if !self.group.is_empty() {
            let error = Error::Truncated {
                offset: self.group[0].offset,
            };
            return self.fail(error);
        }
        self.finished = true;
        Ok(Vec::new())
    }
}

pub fn decode(input: &[u8]) -> Result<Vec<u8>, Error> {
    let mut decoder = Decoder::new();
    let mut output = decoder.push(input)?;
    output.extend(decoder.finish()?);
    Ok(output)
}

fn main() {}