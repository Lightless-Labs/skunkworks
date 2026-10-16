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

fn encode_triplet(input: &[u8]) -> String {
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
    for chunk in input.chunks_exact(3) {
        out.push_str(&encode_triplet(chunk));
    }
    let tail = input.chunks_exact(3).remainder();
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

        let mut combined = Vec::with_capacity(self.tail.len() + input.len());
        combined.extend_from_slice(&self.tail);
        combined.extend_from_slice(input);

        let complete_len = combined.len() / 3 * 3;
        let mut out = String::with_capacity(complete_len / 3 * 4);
        for chunk in combined[..complete_len].chunks_exact(3) {
            out.push_str(&encode_triplet(chunk));
        }

        self.tail.clear();
        self.tail.extend_from_slice(&combined[complete_len..]);
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
            encode_triplet(&self.tail)
        };
        self.tail.clear();
        Ok(out)
    }
}

#[derive(Clone, Copy)]
struct Symbol {
    byte: u8,
    value: u8,
    offset: usize,
}

pub struct Decoder {
    group: Vec<Symbol>,
    offset: usize,
    padded: bool,
    finished: bool,
    sticky_error: Option<Error>,
}

impl Decoder {
    pub fn new() -> Self {
        Self {
            group: Vec::with_capacity(4),
            offset: 0,
            padded: false,
            finished: false,
            sticky_error: None,
        }
    }

    fn fail<T>(&mut self, error: Error) -> Result<T, Error> {
        self.sticky_error = Some(error.clone());
        Err(error)
    }

    fn decode_group(group: &[Symbol]) -> Vec<u8> {
        let a = group[0].value;
        let b = group[1].value;
        let mut out = Vec::with_capacity(3);
        out.push((a << 2) | (b >> 4));

        if group[2].byte != b'=' {
            let c = group[2].value;
            out.push((b << 4) | (c >> 2));

            if group[3].byte != b'=' {
                let d = group[3].value;
                out.push((c << 6) | d);
            }
        }
        out
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

            if self.padded {
                return self.fail(Error::BadPadding {
                    offset: current_offset,
                });
            }

            if self.group.len() == 3 && self.group[2].byte == b'=' {
                if byte != b'=' {
                    return self.fail(Error::BadPadding {
                        offset: current_offset,
                    });
                }
                self.group.push(Symbol {
                    byte,
                    value: 0,
                    offset: current_offset,
                });
                out.extend(Self::decode_group(&self.group));
                self.group.clear();
                self.padded = true;
                continue;
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
                        self.group.push(Symbol {
                            byte,
                            value: 0,
                            offset: current_offset,
                        });
                    }
                    3 => {
                        self.group.push(Symbol {
                            byte,
                            value: 0,
                            offset: current_offset,
                        });
                        out.extend(Self::decode_group(&self.group));
                        self.group.clear();
                        self.padded = true;
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

            self.group.push(Symbol {
                byte,
                value,
                offset: current_offset,
            });

            if self.group.len() == 4 {
                out.extend(Self::decode_group(&self.group));
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

        if self.group.len() == 3 && self.group[2].byte == b'=' {
            return self.fail(Error::BadPadding {
                offset: self.group[2].offset,
            });
        }

        if !self.group.is_empty() {
            return self.fail(Error::Truncated {
                offset: self.group[0].offset,
            });
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