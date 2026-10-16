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

fn encode_group(bytes: &[u8], out: &mut String) {
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
}

pub fn encode(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    let mut chunks = input.chunks_exact(3);

    for group in &mut chunks {
        encode_group(group, &mut out);
    }
    if !chunks.remainder().is_empty() {
        encode_group(chunks.remainder(), &mut out);
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
        for group in data[..complete_len].chunks_exact(3) {
            encode_group(group, &mut out);
        }

        self.tail.clear();
        self.tail.extend_from_slice(&data[complete_len..]);
        Ok(out)
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }

        let mut out = String::new();
        if !self.tail.is_empty() {
            encode_group(&self.tail, &mut out);
        }
        self.tail.clear();
        self.finished = true;
        Ok(out)
    }
}

impl Default for Encoder {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Decoder {
    group: Vec<(u8, usize)>,
    offset: usize,
    terminal_padding: bool,
    pending_second_padding: Option<usize>,
    finished: bool,
    error: Option<Error>,
}

impl Decoder {
    pub fn new() -> Self {
        Self {
            group: Vec::with_capacity(4),
            offset: 0,
            terminal_padding: false,
            pending_second_padding: None,
            finished: false,
            error: None,
        }
    }

    fn fail(&mut self, error: Error) -> Error {
        self.error = Some(error.clone());
        error
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
                self.finish_padded_group(&mut output);
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
                        self.pending_second_padding = Some(current_offset);
                        self.group.push((byte, current_offset));
                    }
                    3 => {
                        self.group.push((byte, current_offset));
                        self.finish_padded_group(&mut output);
                    }
                    _ => unreachable!(),
                }
                continue;
            }

            let Some(value) = digit(byte) else {
                return Err(self.fail(Error::InvalidByte {
                    offset: current_offset,
                    byte,
                }));
            };

            self.group.push((value, current_offset));
            if self.group.len() == 4 {
                output.push((self.group[0].0 << 2) | (self.group[1].0 >> 4));
                output.push((self.group[1].0 << 4) | (self.group[2].0 >> 2));
                output.push((self.group[2].0 << 6) | self.group[3].0);
                self.group.clear();
            }
        }

        Ok(output)
    }

    fn finish_padded_group(&mut self, output: &mut Vec<u8>) {
        let a = self.group[0].0;
        let b = self.group[1].0;
        output.push((a << 2) | (b >> 4));

        if self.group[2].0 != b'=' {
            let c = self.group[2].0;
            output.push((b << 4) | (c >> 2));
        }

        self.group.clear();
        self.terminal_padding = true;
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        if self.finished {
            return Err(Error::AlreadyFinished);
        }

        if let Some(offset) = self.pending_second_padding {
            return Err(self.fail(Error::BadPadding { offset }));
        }

        if !self.group.is_empty() {
            return Err(self.fail(Error::Truncated {
                offset: self.group[0].1,
            }));
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