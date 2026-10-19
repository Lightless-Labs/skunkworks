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
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks_exact(3) {
        out.push_str(&encode_group(chunk));
    }
    let tail = input.chunks_exact(3).remainder();
    if !tail.is_empty() {
        out.push_str(&encode_group(tail));
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
            tail: Vec::with_capacity(2),
            finished: false,
        }
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }

        let mut out = String::with_capacity((self.tail.len() + input.len()) / 3 * 4);
        let mut pos = 0;

        if !self.tail.is_empty() {
            while self.tail.len() < 3 && pos < input.len() {
                self.tail.push(input[pos]);
                pos += 1;
            }
            if self.tail.len() == 3 {
                out.push_str(&encode_group(&self.tail));
                self.tail.clear();
            }
        }

        while pos + 3 <= input.len() {
            out.push_str(&encode_group(&input[pos..pos + 3]));
            pos += 3;
        }

        self.tail.extend_from_slice(&input[pos..]);
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

impl Default for Encoder {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy)]
struct Symbol {
    byte: u8,
    offset: usize,
}

pub struct Decoder {
    group: Vec<Symbol>,
    pending_second_padding: Option<usize>,
    terminal_padding: bool,
    next_offset: usize,
    finished: bool,
    sticky_error: Option<Error>,
}

impl Decoder {
    pub fn new() -> Self {
        Self {
            group: Vec::with_capacity(4),
            pending_second_padding: None,
            terminal_padding: false,
            next_offset: 0,
            finished: false,
            sticky_error: None,
        }
    }

    fn fail(&mut self, error: Error) -> Error {
        self.sticky_error = Some(error.clone());
        error
    }

    fn decode_complete_group(group: &[Symbol]) -> Vec<u8> {
        let a = digit(group[0].byte).unwrap();
        let b = digit(group[1].byte).unwrap();

        if group[2].byte == b'=' {
            return vec![(a << 2) | (b >> 4)];
        }

        let c = digit(group[2].byte).unwrap();
        let first = (a << 2) | (b >> 4);
        let second = (b << 4) | (c >> 2);

        if group[3].byte == b'=' {
            vec![first, second]
        } else {
            let d = digit(group[3].byte).unwrap();
            vec![first, second, (c << 6) | d]
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
            let offset = self.next_offset;
            self.next_offset += 1;

            if self.terminal_padding {
                return Err(self.fail(Error::BadPadding { offset }));
            }

            if let Some(padding_offset) = self.pending_second_padding {
                if byte != b'=' {
                    return Err(self.fail(Error::BadPadding { offset }));
                }
                self.pending_second_padding = None;
                self.group.push(Symbol { byte, offset });
                out.extend(Self::decode_complete_group(&self.group));
                self.group.clear();
                self.terminal_padding = true;
                let _ = padding_offset;
                continue;
            }

            let position = self.group.len();

            if byte == b'=' {
                match position {
                    0 | 1 => return Err(self.fail(Error::BadPadding { offset })),
                    2 => {
                        self.pending_second_padding = Some(offset);
                        self.group.push(Symbol { byte, offset });
                    }
                    3 => {
                        self.group.push(Symbol { byte, offset });
                        out.extend(Self::decode_complete_group(&self.group));
                        self.group.clear();
                        self.terminal_padding = true;
                    }
                    _ => unreachable!(),
                }
                continue;
            }

            let Some(_) = digit(byte) else {
                return Err(self.fail(Error::InvalidByte { offset, byte }));
            };

            self.group.push(Symbol { byte, offset });
            if self.group.len() == 4 {
                out.extend(Self::decode_complete_group(&self.group));
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
            return Err(self.fail(Error::BadPadding { offset }));
        }
        if !self.group.is_empty() {
            let offset = self.group[0].offset;
            return Err(self.fail(Error::Truncated { offset }));
        }

        self.finished = true;
        Ok(Vec::new())
    }
}

impl Default for Decoder {
    fn default() -> Self {
        Self::new()
    }
}

pub fn decode(input: &[u8]) -> Result<Vec<u8>, Error> {
    let mut decoder = Decoder::new();
    let mut output = decoder.push(input)?;
    output.extend(decoder.finish()?);
    Ok(output)
}