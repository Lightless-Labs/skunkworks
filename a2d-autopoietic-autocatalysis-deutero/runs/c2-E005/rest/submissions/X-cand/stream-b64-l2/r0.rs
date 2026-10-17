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

fn encode_group(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(4);
    let a = bytes[0];
    let b = if bytes.len() > 1 { bytes[1] } else { 0 };
    let c = if bytes.len() > 2 { bytes[2] } else { 0 };

    out.push(ALPHABET[(a >> 2) as usize] as char);
    out.push(ALPHABET[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);

    if bytes.len() > 1 {
        out.push(ALPHABET[(((b & 0x0f) << 2) | (c >> 6)) as usize] as char);
    } else {
        out.push('=');
    }

    if bytes.len() > 2 {
        out.push(ALPHABET[(c & 0x3f) as usize] as char);
    } else {
        out.push('=');
    }

    out
}

pub fn encode(input: &[u8]) -> String {
    let mut out = String::new();
    let mut chunks = input.chunks_exact(3);
    for chunk in &mut chunks {
        out.push_str(&encode_group(chunk));
    }
    if !chunks.remainder().is_empty() {
        out.push_str(&encode_group(chunks.remainder()));
    }
    out
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

        let mut data = Vec::with_capacity(self.tail.len() + input.len());
        data.extend_from_slice(&self.tail);
        data.extend_from_slice(input);

        let complete_len = data.len() / 3 * 3;
        let mut out = String::with_capacity(complete_len / 3 * 4);
        for chunk in data[..complete_len].chunks_exact(3) {
            out.push_str(&encode_group(chunk));
        }
        self.tail.clear();
        self.tail.extend_from_slice(&data[complete_len..]);
        Ok(out)
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        self.finished = true;

        let out = if self.tail.is_empty() {
            String::new()
        } else {
            encode_group(&self.tail)
        };
        self.tail.clear();
        Ok(out)
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

        let mut out = Vec::new();

        for &byte in input {
            let here = self.offset;
            self.offset += 1;

            if self.terminal_padding {
                return self.fail(Error::BadPadding { offset: here });
            }

            if self.pending_second_padding.is_some() && byte != b'=' {
                return self.fail(Error::BadPadding { offset: here });
            }

            let pos = self.group.len();
            if byte == b'=' {
                match pos {
                    0 | 1 => return self.fail(Error::BadPadding { offset: here }),
                    2 => {
                        self.group.push(Symbol { byte, offset: here });
                        self.pending_second_padding = Some(here);
                    }
                    3 => {
                        self.group.push(Symbol { byte, offset: here });
                        if self.group[2].byte != b'=' {
                            let a = digit(self.group[0].byte).unwrap();
                            let b = digit(self.group[1].byte).unwrap();
                            out.push((a << 2) | (b >> 4));
                            out.push((b << 4) | (digit(self.group[2].byte).unwrap() >> 2));
                        } else {
                            let a = digit(self.group[0].byte).unwrap();
                            let b = digit(self.group[1].byte).unwrap();
                            out.push((a << 2) | (b >> 4));
                        }
                        self.group.clear();
                        self.pending_second_padding = None;
                        self.terminal_padding = true;
                    }
                    _ => unreachable!(),
                }
                continue;
            }

            if self.pending_second_padding.is_some() {
                // The precedence check above permits only '=' here.
                unreachable!();
            }

            let Some(value) = digit(byte) else {
                return self.fail(Error::InvalidByte { offset: here, byte });
            };

            self.group.push(Symbol { byte, offset: here });
            if self.group.len() == 4 {
                let a = digit(self.group[0].byte).unwrap();
                let b = digit(self.group[1].byte).unwrap();
                let c = digit(self.group[2].byte).unwrap();
                let d = value;
                out.push((a << 2) | (b >> 4));
                out.push((b << 4) | (c >> 2));
                out.push((c << 6) | d);
                self.group.clear();
            }
        }

        Ok(out)
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
                offset: self.group[0].offset,
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

pub fn decode(input: &[u8]) -> Result<Vec<u8>, Error> {
    let mut decoder = Decoder::new();
    let mut out = decoder.push(input)?;
    out.extend(decoder.finish()?);
    Ok(out)
}