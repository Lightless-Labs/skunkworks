const ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidByte { offset: usize, byte: u8 },
    BadPadding { offset: usize },
    Truncated { offset: usize },
    AlreadyFinished,
}

pub struct Encoder {
    buffered: Vec<u8>,
    closed: bool,
}

impl Encoder {
    pub fn new() -> Self {
        Self {
            buffered: Vec::new(),
            closed: false,
        }
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }

        let mut out = String::new();
        let mut i = 0;
        if !self.buffered.is_empty() {
            while self.buffered.len() < 3 && i < input.len() {
                self.buffered.push(input[i]);
                i += 1;
            }
            if self.buffered.len() == 3 {
                encode_three(
                    self.buffered[0],
                    self.buffered[1],
                    self.buffered[2],
                    &mut out,
                );
                self.buffered.clear();
            }
        }
        while i + 3 <= input.len() {
            encode_three(input[i], input[i + 1], input[i + 2], &mut out);
            i += 3;
        }
        self.buffered.extend_from_slice(&input[i..]);
        Ok(out)
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        self.closed = true;

        let mut out = String::new();
        match self.buffered.as_slice() {
            [a] => {
                out.push(ALPHABET[(a >> 2) as usize] as char);
                out.push(ALPHABET[((a & 3) << 4) as usize] as char);
                out.push_str("==");
            }
            [a, b] => {
                out.push(ALPHABET[(a >> 2) as usize] as char);
                out.push(ALPHABET[(((a & 3) << 4) | (b >> 4)) as usize] as char);
                out.push(ALPHABET[((b & 15) << 2) as usize] as char);
                out.push('=');
            }
            _ => {}
        }
        self.buffered.clear();
        Ok(out)
    }
}

fn encode_three(a: u8, b: u8, c: u8, out: &mut String) {
    out.push(ALPHABET[(a >> 2) as usize] as char);
    out.push(ALPHABET[(((a & 3) << 4) | (b >> 4)) as usize] as char);
    out.push(ALPHABET[(((b & 15) << 2) | (c >> 6)) as usize] as char);
    out.push(ALPHABET[(c & 63) as usize] as char);
}

pub fn encode(input: &[u8]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i + 3 <= input.len() {
        encode_three(input[i], input[i + 1], input[i + 2], &mut out);
        i += 3;
    }
    match &input[i..] {
        [a] => {
            out.push(ALPHABET[(a >> 2) as usize] as char);
            out.push(ALPHABET[((a & 3) << 4) as usize] as char);
            out.push_str("==");
        }
        [a, b] => {
            out.push(ALPHABET[(a >> 2) as usize] as char);
            out.push(ALPHABET[(((a & 3) << 4) | (b >> 4)) as usize] as char);
            out.push(ALPHABET[((b & 15) << 2) as usize] as char);
            out.push('=');
        }
        _ => {}
    }
    out
}

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

pub struct Decoder {
    vals: [u8; 4],
    starts: [usize; 4],
    count: usize,
    second_pad: Option<usize>,
    padded_end: bool,
    absolute: usize,
    closed: bool,
    failed: Option<Error>,
}

impl Decoder {
    pub fn new() -> Self {
        Self {
            vals: [0; 4],
            starts: [0; 4],
            count: 0,
            second_pad: None,
            padded_end: false,
            absolute: 0,
            closed: false,
            failed: None,
        }
    }

    fn error(&mut self, error: Error) -> Result<Vec<u8>, Error> {
        self.failed = Some(error.clone());
        Err(error)
    }

    pub fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.failed {
            return Err(error.clone());
        }
        if self.closed {
            return Err(Error::AlreadyFinished);
        }

        let mut output = Vec::new();
        for &byte in input {
            let here = self.absolute;
            self.absolute += 1;

            if self.padded_end {
                return self.error(Error::BadPadding { offset: here });
            }
            if self.second_pad.is_some() {
                if byte == b'=' {
                    output.push((self.vals[0] << 2) | (self.vals[1] >> 4));
                    self.second_pad = None;
                    self.count = 0;
                    self.padded_end = true;
                    continue;
                }
                return self.error(Error::BadPadding { offset: here });
            }

            if self.count < 2 && byte == b'=' {
                return self.error(Error::BadPadding { offset: here });
            }
            if self.count == 2 && byte == b'=' {
                self.starts[2] = here;
                self.count = 3;
                self.second_pad = Some(here);
                continue;
            }
            if self.count == 3 && byte == b'=' {
                output.push((self.vals[0] << 2) | (self.vals[1] >> 4));
                output.push((self.vals[1] << 4) | (self.vals[2] >> 2));
                self.count = 0;
                self.padded_end = true;
                continue;
            }

            let Some(value) = digit(byte) else {
                return self.error(Error::InvalidByte { offset: here, byte });
            };

            self.vals[self.count] = value;
            self.starts[self.count] = here;
            self.count += 1;
            if self.count == 4 {
                output.push((self.vals[0] << 2) | (self.vals[1] >> 4));
                output.push((self.vals[1] << 4) | (self.vals[2] >> 2));
                output.push((self.vals[2] << 6) | self.vals[3]);
                self.count = 0;
            }
        }
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.failed {
            return Err(error.clone());
        }
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        if let Some(at) = self.second_pad {
            return self.error(Error::BadPadding { offset: at });
        }
        if self.count != 0 {
            return self.error(Error::Truncated {
                offset: self.starts[0],
            });
        }
        self.closed = true;
        Ok(Vec::new())
    }
}

pub fn decode(input: &[u8]) -> Result<Vec<u8>, Error> {
    let mut decoder = Decoder::new();
    let mut output = decoder.push(input)?;
    output.extend(decoder.finish()?);
    Ok(output)
}
