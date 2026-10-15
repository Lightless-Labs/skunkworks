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

fn encode_group(bytes: &[u8], output: &mut String) {
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
}

pub fn encode(input: &[u8]) -> String {
    let mut encoder = Encoder::new();
    let mut output = encoder.push(input).unwrap();
    output.push_str(&encoder.finish().unwrap());
    output
}

pub fn decode(input: &[u8]) -> Result<Vec<u8>, Error> {
    let mut decoder = Decoder::new();
    let mut output = decoder.push(input)?;
    output.extend(decoder.finish()?);
    Ok(output)
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

        let mut output = String::new();
        let mut index = 0;

        if !self.tail.is_empty() {
            while self.tail.len() < 3 && index < input.len() {
                self.tail.push(input[index]);
                index += 1;
            }
            if self.tail.len() == 3 {
                encode_group(&self.tail, &mut output);
                self.tail.clear();
            }
        }

        while index + 3 <= input.len() {
            encode_group(&input[index..index + 3], &mut output);
            index += 3;
        }

        self.tail.extend_from_slice(&input[index..]);
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }

        let mut output = String::new();
        if !self.tail.is_empty() {
            encode_group(&self.tail, &mut output);
            self.tail.clear();
        }
        self.finished = true;
        Ok(output)
    }
}

#[derive(Clone, Copy)]
enum Symbol {
    Digit(u8),
    Padding(usize),
}

pub struct Decoder {
    group: Vec<(Symbol, usize)>,
    offset: usize,
    terminal: bool,
    pending_padding: Option<usize>,
    failed: Option<Error>,
    finished: bool,
}

impl Decoder {
    pub fn new() -> Self {
        Self {
            group: Vec::with_capacity(4),
            offset: 0,
            terminal: false,
            pending_padding: None,
            failed: None,
            finished: false,
        }
    }

    fn fail<T>(&mut self, error: Error) -> Result<T, Error> {
        self.failed = Some(error.clone());
        Err(error)
    }

    fn consume(&mut self, byte: u8, offset: usize, output: &mut Vec<u8>) -> Result<(), Error> {
        if self.terminal {
            return self.fail(Error::BadPadding { offset });
        }

        if let Some(padding_offset) = self.pending_padding {
            if byte != b'=' {
                return self.fail(Error::BadPadding { offset });
            }
            self.pending_padding = None;
            self.group.push((Symbol::Padding(padding_offset), padding_offset));
            self.group.push((Symbol::Padding(offset), offset));
            self.emit_group(output);
            self.terminal = true;
            return Ok(());
        }

        let position = self.group.len();

        if byte == b'=' {
            match position {
                0 | 1 => return self.fail(Error::BadPadding { offset }),
                2 => {
                    self.pending_padding = Some(offset);
                    return Ok(());
                }
                3 => {
                    self.group.push((Symbol::Padding(offset), offset));
                    self.emit_group(output);
                    self.terminal = true;
                    return Ok(());
                }
                _ => unreachable!(),
            }
        }

        let Some(value) = digit(byte) else {
            return self.fail(Error::InvalidByte { offset, byte });
        };

        self.group.push((Symbol::Digit(value), offset));
        if self.group.len() == 4 {
            self.emit_group(output);
        }
        Ok(())
    }

    fn emit_group(&mut self, output: &mut Vec<u8>) {
        let values: Vec<u8> = self
            .group
            .iter()
            .map(|(symbol, _)| match symbol {
                Symbol::Digit(value) => *value,
                Symbol::Padding(_) => 0,
            })
            .collect();

        let a = values[0];
        let b = values[1];
        output.push((a << 2) | (b >> 4));

        if !matches!(self.group[2].0, Symbol::Padding(_)) {
            let c = values[2];
            output.push((b << 4) | (c >> 2));
            if !matches!(self.group[3].0, Symbol::Padding(_)) {
                let d = values[3];
                output.push((c << 6) | d);
            }
        }

        self.group.clear();
    }

    pub fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.failed {
            return Err(error.clone());
        }
        if self.finished {
            return Err(Error::AlreadyFinished);
        }

        let mut output = Vec::new();
        for &byte in input {
            let offset = self.offset;
            self.offset += 1;
            if let Err(error) = self.consume(byte, offset, &mut output) {
                return Err(error);
            }
        }
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.failed {
            return Err(error.clone());
        }
        if self.finished {
            return Err(Error::AlreadyFinished);
        }

        if let Some(offset) = self.pending_padding {
            return self.fail(Error::BadPadding { offset });
        }
        if let Some((_, offset)) = self.group.first() {
            return self.fail(Error::Truncated { offset: *offset });
        }

        self.finished = true;
        Ok(Vec::new())
    }
}

fn main() {}