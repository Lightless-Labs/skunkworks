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
        if self.tail.is_empty() {
            Ok(String::new())
        } else {
            let out = encode_group(&self.tail);
            self.tail.clear();
            Ok(out)
        }
    }
}

#[derive(Clone, Copy)]
enum Symbol {
    Digit(u8),
    Padding,
}

pub struct Decoder {
    group: Vec<(Symbol, usize)>,
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

    fn fail(&mut self, error: Error) -> Error {
        self.sticky_error = Some(error.clone());
        error
    }

    fn scan_byte(&mut self, byte: u8, offset: usize, output: &mut Vec<u8>) -> Result<(), Error> {
        if self.terminal_padding {
            return Err(Error::BadPadding { offset });
        }

        if let Some(pending_offset) = self.pending_second_padding {
            if byte != b'=' {
                return Err(Error::BadPadding { offset });
            }
            self.pending_second_padding = None;
            self.group.push((Symbol::Padding, offset));
            self.terminal_padding = true;
            self.emit_group(output);
            let _ = pending_offset;
            return Ok(());
        }

        let position = self.group.len();
        if byte == b'=' {
            match position {
                0 | 1 => return Err(Error::BadPadding { offset }),
                2 => {
                    self.group.push((Symbol::Padding, offset));
                    self.pending_second_padding = Some(offset);
                    return Ok(());
                }
                3 => {
                    self.group.push((Symbol::Padding, offset));
                    self.terminal_padding = true;
                    self.emit_group(output);
                    return Ok(());
                }
                _ => unreachable!(),
            }
        }

        let value = digit(byte).ok_or(Error::InvalidByte { offset, byte })?;
        self.group.push((Symbol::Digit(value), offset));

        if self.group.len() == 4 {
            self.emit_group(output);
        }

        Ok(())
    }

    fn emit_group(&mut self, output: &mut Vec<u8>) {
        if self.group.len() != 4 {
            return;
        }

        let a = match self.group[0].0 {
            Symbol::Digit(v) => v,
            Symbol::Padding => unreachable!(),
        };
        let b = match self.group[1].0 {
            Symbol::Digit(v) => v,
            Symbol::Padding => unreachable!(),
        };

        output.push((a << 2) | (b >> 4));

        match (self.group[2].0, self.group[3].0) {
            (Symbol::Digit(c), Symbol::Digit(d)) => {
                output.push((b << 4) | (c >> 2));
                output.push((c << 6) | d);
            }
            (Symbol::Digit(c), Symbol::Padding) => {
                output.push((b << 4) | (c >> 2));
            }
            (Symbol::Padding, Symbol::Padding) => {}
            _ => unreachable!(),
        }

        self.group.clear();
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
            let offset = self.offset;
            self.offset += 1;
            if let Err(error) = self.scan_byte(byte, offset, &mut output) {
                return Err(self.fail(error));
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
            return Err(self.fail(Error::BadPadding { offset }));
        }

        if !self.group.is_empty() {
            let offset = self.group[0].1;
            return Err(self.fail(Error::Truncated { offset }));
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