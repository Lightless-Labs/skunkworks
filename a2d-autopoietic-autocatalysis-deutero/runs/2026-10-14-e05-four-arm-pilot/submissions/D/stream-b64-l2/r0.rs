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
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

fn encode_triplet(bytes: &[u8]) -> String {
    let a = bytes[0];
    let b = if bytes.len() > 1 { bytes[1] } else { 0 };
    let c = if bytes.len() > 2 { bytes[2] } else { 0 };

    let mut out = String::with_capacity(4);
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
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    let mut chunks = input.chunks_exact(3);

    for chunk in &mut chunks {
        out.push_str(&encode_triplet(chunk));
    }

    let tail = chunks.remainder();
    if !tail.is_empty() {
        out.push_str(&encode_triplet(tail));
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
            out.push_str(&encode_triplet(chunk));
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
        if self.tail.is_empty() {
            Ok(String::new())
        } else {
            let out = encode_triplet(&self.tail);
            self.tail.clear();
            Ok(out)
        }
    }
}

#[derive(Clone, Copy)]
struct Symbol {
    value: u8,
    offset: usize,
}

pub struct Decoder {
    group: Vec<Symbol>,
    offset: usize,
    terminal_padding: bool,
    finished: bool,
    sticky_error: Option<Error>,
}

impl Decoder {
    pub fn new() -> Self {
        Self {
            group: Vec::with_capacity(4),
            offset: 0,
            terminal_padding: false,
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

        let mut out = Vec::new();

        for &byte in input {
            let current_offset = self.offset;
            self.offset += 1;

            if self.terminal_padding {
                return Err(self.fail(Error::BadPadding {
                    offset: current_offset,
                }));
            }

            if self.group.len() == 3
                && self.group[2].value == 64
                && byte != b'='
            {
                return Err(self.fail(Error::BadPadding {
                    offset: current_offset,
                }));
            }

            let position = self.group.len();
            if byte == b'=' {
                if position < 2 {
                    return Err(self.fail(Error::BadPadding {
                        offset: current_offset,
                    }));
                }
                self.group.push(Symbol {
                    value: 64,
                    offset: current_offset,
                });
            } else if let Some(value) = digit(byte) {
                self.group.push(Symbol {
                    value,
                    offset: current_offset,
                });
            } else {
                return Err(self.fail(Error::InvalidByte {
                    offset: current_offset,
                    byte,
                }));
            }

            if self.group.len() == 4 {
                let s = &self.group;
                let a = s[0].value;
                let b = s[1].value;
                let c = s[2].value;
                let d = s[3].value;

                if c == 64 {
                    // The scan rules guarantee d is also padding here.
                    out.push((a << 2) | (b >> 4));
                    self.terminal_padding = true;
                } else if d == 64 {
                    out.push((a << 2) | (b >> 4));
                    out.push((b << 4) | (c >> 2));
                    self.terminal_padding = true;
                } else {
                    out.push((a << 2) | (b >> 4));
                    out.push((b << 4) | (c >> 2));
                    out.push((c << 6) | d);
                }
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

        if self.group.len() == 3 && self.group[2].value == 64 {
            let error = Error::BadPadding {
                offset: self.group[2].offset,
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

pub fn decode(input: &[u8]) -> Result<Vec<u8>, Error> {
    let mut decoder = Decoder::new();
    let mut out = decoder.push(input)?;
    out.extend(decoder.finish()?);
    Ok(out)
}