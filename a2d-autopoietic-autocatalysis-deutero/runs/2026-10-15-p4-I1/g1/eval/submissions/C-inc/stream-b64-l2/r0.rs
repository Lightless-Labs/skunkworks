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

fn encode_full(a: u8, b: u8, c: u8, out: &mut String) {
    out.push(ALPHABET[(a >> 2) as usize] as char);
    out.push(ALPHABET[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);
    out.push(ALPHABET[(((b & 0x0f) << 2) | (c >> 6)) as usize] as char);
    out.push(ALPHABET[(c & 0x3f) as usize] as char);
}

pub fn encode(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    let mut chunks = input.chunks_exact(3);
    for chunk in &mut chunks {
        encode_full(chunk[0], chunk[1], chunk[2], &mut out);
    }
    match chunks.remainder() {
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
        _ => {}
    }
    out
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
        let mut out = String::with_capacity(complete_len / 3 * 4);
        for chunk in data[..complete_len].chunks_exact(3) {
            encode_full(chunk[0], chunk[1], chunk[2], &mut out);
        }

        self.pending.clear();
        self.pending.extend_from_slice(&data[complete_len..]);
        Ok(out)
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        let out = encode(&self.pending);
        self.pending.clear();
        self.finished = true;
        Ok(out)
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
    pending_second_padding: Option<usize>,
    finished: bool,
    sticky_error: Option<Error>,
}

impl Decoder {
    pub fn new() -> Self {
        Self {
            group: Vec::with_capacity(4),
            offset: 0,
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

    fn emit_group(&mut self, out: &mut Vec<u8>) {
        let a = self.group[0].value;
        let b = self.group[1].value;
        let c = self.group[2].value;
        let d = self.group[3].value;

        out.push((a << 2) | (b >> 4));
        if c != 64 {
            out.push((b << 4) | (c >> 2));
        }
        if d != 64 {
            out.push((c << 6) | d);
        }

        if c == 64 || d == 64 {
            self.terminal_padding = true;
        }
        self.group.clear();
        self.pending_second_padding = None;
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
            let at = self.offset;
            self.offset += 1;

            if self.terminal_padding {
                return self.fail(Error::BadPadding { offset: at });
            }

            if let Some(padding_offset) = self.pending_second_padding {
                if byte != b'=' {
                    return self.fail(Error::BadPadding { offset: at });
                }
                self.group.push(Symbol {
                    value: 64,
                    offset: at,
                });
                self.pending_second_padding = None;
                self.emit_group(&mut out);
                let _ = padding_offset;
                continue;
            }

            let position = self.group.len();
            if byte == b'=' {
                match position {
                    0 | 1 => return self.fail(Error::BadPadding { offset: at }),
                    2 => {
                        self.group.push(Symbol {
                            value: 64,
                            offset: at,
                        });
                        self.pending_second_padding = Some(at);
                    }
                    3 => {
                        self.group.push(Symbol {
                            value: 64,
                            offset: at,
                        });
                        self.emit_group(&mut out);
                    }
                    _ => unreachable!(),
                }
                continue;
            }

            let Some(value) = digit(byte) else {
                return self.fail(Error::InvalidByte { offset: at, byte });
            };
            self.group.push(Symbol { value, offset: at });
            if self.group.len() == 4 {
                self.emit_group(&mut out);
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
}

pub fn decode(input: &[u8]) -> Result<Vec<u8>, Error> {
    let mut decoder = Decoder::new();
    let mut out = decoder.push(input)?;
    out.extend(decoder.finish()?);
    Ok(out)
}

fn main() {}