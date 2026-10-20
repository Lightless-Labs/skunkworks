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

fn encode_group(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(4);
    let a = bytes[0];
    let b = if bytes.len() > 1 { bytes[1] } else { 0 };
    let c = if bytes.len() > 2 { bytes[2] } else { 0 };

    output.push(ALPHABET[(a >> 2) as usize] as char);
    output.push(ALPHABET[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);

    if bytes.len() > 1 {
        output.push(ALPHABET[(((b & 0x0f) << 2) | (c >> 6)) as usize] as char);
    } else {
        output.push('=');
    }

    if bytes.len() > 2 {
        output.push(ALPHABET[(c & 0x3f) as usize] as char);
    } else {
        output.push('=');
    }

    output
}

pub fn encode(input: &[u8]) -> String {
    let mut output = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks_exact(3) {
        output.push_str(&encode_group(chunk));
    }
    let tail = input.chunks_exact(3).remainder();
    if !tail.is_empty() {
        output.push_str(&encode_group(tail));
    }
    output
}

pub struct Encoder {
    pending: Vec<u8>,
    finished: bool,
}

impl Encoder {
    pub fn new() -> Self {
        Self {
            pending: Vec::new(),
            finished: false,
        }
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }

        let mut data = Vec::with_capacity(self.pending.len() + input.len());
        data.extend_from_slice(&self.pending);
        data.extend_from_slice(input);

        let complete_len = data.len() / 3 * 3;
        let mut output = String::with_capacity(complete_len / 3 * 4);
        for chunk in data[..complete_len].chunks_exact(3) {
            output.push_str(&encode_group(chunk));
        }

        self.pending.clear();
        self.pending.extend_from_slice(&data[complete_len..]);
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        self.finished = true;
        if self.pending.is_empty() {
            Ok(String::new())
        } else {
            Ok(encode_group(&self.pending))
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
    next_offset: usize,
    terminal_padding: bool,
    pending_second_padding: Option<usize>,
    finished: bool,
    sticky_error: Option<Error>,
}

impl Decoder {
    pub fn new() -> Self {
        Self {
            group: Vec::with_capacity(4),
            next_offset: 0,
            terminal_padding: false,
            pending_second_padding: None,
            finished: false,
            sticky_error: None,
        }
    }

    fn fail(&mut self, error: Error) -> Result<Vec<u8>, Error> {
        self.sticky_error = Some(error.clone());
        Err(error)
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
            let offset = self.next_offset;
            self.next_offset += 1;

            if self.terminal_padding {
                return self.fail(Error::BadPadding { offset });
            }

            if let Some(padding_offset) = self.pending_second_padding {
                if byte != b'=' {
                    return self.fail(Error::BadPadding { offset });
                }
                let first = digit(self.group[0].byte).unwrap();
                let second = digit(self.group[1].byte).unwrap();
                output.push((first << 2) | (second >> 4));
                self.group.clear();
                self.pending_second_padding = None;
                self.terminal_padding = true;
                let _ = padding_offset;
                continue;
            }

            let position = self.group.len();
            if byte == b'=' {
                match position {
                    0 | 1 => return self.fail(Error::BadPadding { offset }),
                    2 => {
                        self.group.push(Symbol { byte, offset });
                        self.pending_second_padding = Some(offset);
                    }
                    3 => {
                        let a = digit(self.group[0].byte).unwrap();
                        let b = digit(self.group[1].byte).unwrap();
                        let c = digit(self.group[2].byte).unwrap();
                        output.push((a << 2) | (b >> 4));
                        output.push((b << 4) | (c >> 2));
                        self.group.clear();
                        self.terminal_padding = true;
                    }
                    _ => unreachable!(),
                }
                continue;
            }

            let Some(value) = digit(byte) else {
                return self.fail(Error::InvalidByte { offset, byte });
            };
            self.group.push(Symbol { byte, offset });

            if self.group.len() == 4 {
                let a = digit(self.group[0].byte).unwrap();
                let b = digit(self.group[1].byte).unwrap();
                let c = digit(self.group[2].byte).unwrap();
                let d = value;
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
}

pub fn decode(input: &[u8]) -> Result<Vec<u8>, Error> {
    let mut decoder = Decoder::new();
    let mut output = decoder.push(input)?;
    output.extend(decoder.finish()?);
    Ok(output)
}