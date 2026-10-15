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

fn encode_block(bytes: &[u8]) -> String {
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
    for chunk in input.chunks(3) {
        if chunk.len() == 3 {
            out.push_str(&encode_block(chunk));
        } else {
            out.push_str(&encode_block(chunk));
        }
    }
    out
}

#[derive(Clone)]
struct Group {
    values: [u8; 4],
    offsets: [usize; 4],
    len: usize,
    pending_second_padding: Option<usize>,
    terminal_padding: bool,
}

impl Group {
    fn new() -> Self {
        Self {
            values: [0; 4],
            offsets: [0; 4],
            len: 0,
            pending_second_padding: None,
            terminal_padding: false,
        }
    }

    fn clear(&mut self) {
        self.values = [0; 4];
        self.offsets = [0; 4];
        self.len = 0;
        self.pending_second_padding = None;
    }
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
        let mut bytes = Vec::with_capacity(self.buffer.len() + input.len());
        bytes.extend_from_slice(&self.buffer);
        bytes.extend_from_slice(input);

        let complete_len = bytes.len() / 3 * 3;
        let mut out = String::with_capacity(complete_len / 3 * 4);
        for chunk in bytes[..complete_len].chunks_exact(3) {
            out.push_str(&encode_block(chunk));
        }
        self.buffer.clear();
        self.buffer.extend_from_slice(&bytes[complete_len..]);
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
            let result = encode_block(&self.buffer);
            self.buffer.clear();
            Ok(result)
        }
    }
}

pub struct Decoder {
    group: Group,
    offset: usize,
    finished: bool,
    sticky_error: Option<Error>,
}

impl Decoder {
    pub fn new() -> Self {
        Self {
            group: Group::new(),
            offset: 0,
            finished: false,
            sticky_error: None,
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
            let current_offset = self.offset;
            self.offset += 1;

            if self.group.terminal_padding {
                return self.fail(Error::BadPadding {
                    offset: current_offset,
                });
            }

            if let Some(padding_offset) = self.group.pending_second_padding {
                if byte != b'=' {
                    return self.fail(Error::BadPadding {
                        offset: current_offset,
                    });
                }
                self.group.pending_second_padding = None;
                self.group.values[3] = 0;
                self.group.offsets[3] = current_offset;
                self.group.len = 4;
                self.group.terminal_padding = true;
                out.extend(decode_group(&self.group.values, 1));
                let _ = padding_offset;
                self.group.clear();
                self.group.terminal_padding = true;
                continue;
            }

            let position = self.group.len;
            if byte == b'=' {
                match position {
                    0 | 1 => {
                        return self.fail(Error::BadPadding {
                            offset: current_offset,
                        });
                    }
                    2 => {
                        self.group.pending_second_padding = Some(current_offset);
                        self.group.values[2] = 0;
                        self.group.offsets[2] = current_offset;
                        self.group.len = 3;
                    }
                    3 => {
                        self.group.values[3] = 0;
                        self.group.offsets[3] = current_offset;
                        self.group.len = 4;
                        self.group.terminal_padding = true;
                        out.extend(decode_group(&self.group.values, 2));
                        self.group.clear();
                        self.group.terminal_padding = true;
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

            self.group.values[position] = value;
            self.group.offsets[position] = current_offset;
            self.group.len += 1;
            if self.group.len == 4 {
                out.extend(decode_group(&self.group.values, 3));
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

        if let Some(offset) = self.group.pending_second_padding {
            return self.fail(Error::BadPadding { offset });
        }
        if self.group.len != 0 {
            return self.fail(Error::Truncated {
                offset: self.group.offsets[0],
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

fn decode_group(values: &[u8; 4], byte_count: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(byte_count);
    if byte_count >= 1 {
        out.push((values[0] << 2) | (values[1] >> 4));
    }
    if byte_count >= 2 {
        out.push((values[1] << 4) | (values[2] >> 2));
    }
    if byte_count >= 3 {
        out.push((values[2] << 6) | values[3]);
    }
    out
}

pub fn decode(input: &[u8]) -> Result<Vec<u8>, Error> {
    let mut decoder = Decoder::new();
    let mut out = decoder.push(input)?;
    out.extend(decoder.finish()?);
    Ok(out)
}

fn main() {}