#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidByte { offset: usize, byte: u8 },
    BadPadding { offset: usize },
    Truncated { offset: usize },
    AlreadyFinished,
}

pub struct Encoder {
    tail: Vec<u8>,
    finished: bool,
}

pub struct Decoder {
    group: Vec<(u8, usize)>,
    offset: usize,
    terminal_padding: bool,
    pending_second_padding: Option<usize>,
    finished: bool,
    sticky_error: Option<Error>,
}

const ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn digit(byte: u8) -> Option<u8> {
    ALPHABET.iter().position(|&b| b == byte).map(|i| i as u8)
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
    let mut out = String::new();
    let mut chunks = input.chunks_exact(3);
    for chunk in &mut chunks {
        let encoded = encode_group(chunk[0], chunk[1], chunk[2]);
        out.push_str(std::str::from_utf8(&encoded).unwrap());
    }
    match chunks.remainder() {
        [] => {}
        [a] => {
            out.push(ALPHABET[(a >> 2) as usize] as char);
            out.push(ALPHABET[((a & 0x03) << 4) as usize] as char);
            out.push_str("==");
        }
        [a, b] => {
            out.push(ALPHABET[(a >> 2) as usize] as char);
            out.push(ALPHABET[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);
            out.push(ALPHABET[((b & 0x0f) << 2) as usize] as char);
            out.push('=');
        }
        _ => unreachable!(),
    }
    out
}

fn decode_group(group: &[(u8, usize)]) -> Vec<u8> {
    let a = digit(group[0].0).unwrap();
    let b = digit(group[1].0).unwrap();
    let mut out = vec![(a << 2) | (b >> 4)];

    if group[2].0 != b'=' {
        let c = digit(group[2].0).unwrap();
        out.push((b << 4) | (c >> 2));
        if group[3].0 != b'=' {
            let d = digit(group[3].0).unwrap();
            out.push((c << 6) | d);
        }
    }
    out
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

        let mut bytes = std::mem::take(&mut self.tail);
        bytes.extend_from_slice(input);
        let complete_len = bytes.len() / 3 * 3;
        let mut out = String::with_capacity(complete_len / 3 * 4);

        for chunk in bytes[..complete_len].chunks_exact(3) {
            let encoded = encode_group(chunk[0], chunk[1], chunk[2]);
            out.push_str(std::str::from_utf8(&encoded).unwrap());
        }
        self.tail.extend_from_slice(&bytes[complete_len..]);
        Ok(out)
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        self.finished = true;
        let out = match self.tail.as_slice() {
            [] => String::new(),
            [a] => format!(
                "{}{}==",
                ALPHABET[(a >> 2) as usize] as char,
                ALPHABET[((a & 0x03) << 4) as usize] as char
            ),
            [a, b] => format!(
                "{}{}{}=",
                ALPHABET[(a >> 2) as usize] as char,
                ALPHABET[(((a & 0x03) << 4) | (b >> 4)) as usize] as char,
                ALPHABET[((b & 0x0f) << 2) as usize] as char
            ),
            _ => unreachable!(),
        };
        self.tail.clear();
        Ok(out)
    }
}

impl Decoder {
    pub fn new() -> Self {
        Self {
            group: Vec::new(),
            offset: 0,
            terminal_padding: false,
            pending_second_padding: None,
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

            if let Some(padding_offset) = self.pending_second_padding {
                if byte != b'=' {
                    return Err(self.fail(Error::BadPadding {
                        offset: current_offset,
                    }));
                }
                self.pending_second_padding = None;
                self.group.push((byte, current_offset));
                out.extend(decode_group(&self.group));
                self.group.clear();
                self.terminal_padding = true;
                let _ = padding_offset;
                continue;
            }

            let position = self.group.len();
            if byte == b'=' {
                match position {
                    0 | 1 => {
                        return Err(self.fail(Error::BadPadding {
                            offset: current_offset,
                        }));
                    }
                    2 => {
                        self.group.push((byte, current_offset));
                        self.pending_second_padding = Some(current_offset);
                    }
                    3 => {
                        self.group.push((byte, current_offset));
                        out.extend(decode_group(&self.group));
                        self.group.clear();
                        self.terminal_padding = true;
                    }
                    _ => unreachable!(),
                }
                continue;
            }

            let Some(_) = digit(byte) else {
                return Err(self.fail(Error::InvalidByte {
                    offset: current_offset,
                    byte,
                }));
            };

            self.group.push((byte, current_offset));
            if self.group.len() == 4 {
                out.extend(decode_group(&self.group));
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
            let error = Error::BadPadding { offset };
            return Err(self.fail(error));
        }
        if !self.group.is_empty() {
            let error = Error::Truncated {
                offset: self.group[0].1,
            };
            return Err(self.fail(error));
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