#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    InvalidByte,
    InvalidPadding,
    InvalidLength,
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamError {
    Finished,
}

const ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn encode_triple(input: &[u8], output: &mut String) {
    let a = input[0];
    let b = input.get(1).copied().unwrap_or(0);
    let c = input.get(2).copied().unwrap_or(0);

    output.push(ALPHABET[(a >> 2) as usize] as char);
    output.push(ALPHABET[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);

    if input.len() > 1 {
        output.push(ALPHABET[(((b & 0x0f) << 2) | (c >> 6)) as usize] as char);
    } else {
        output.push('=');
    }

    if input.len() > 2 {
        output.push(ALPHABET[(c & 0x3f) as usize] as char);
    } else {
        output.push('=');
    }
}

fn sextet(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

fn decode_quartet(q: &[u8; 4], output: &mut Vec<u8>) {
    let a = sextet(q[0]).unwrap();
    let b = sextet(q[1]).unwrap();

    output.push((a << 2) | (b >> 4));

    if q[2] != b'=' {
        let c = sextet(q[2]).unwrap();
        output.push((b << 4) | (c >> 2));

        if q[3] != b'=' {
            let d = sextet(q[3]).unwrap();
            output.push((c << 6) | d);
        }
    }
}

pub fn encode(input: &[u8]) -> String {
    let mut output = String::with_capacity(input.len().div_ceil(3) * 4);
    let mut chunks = input.chunks_exact(3);

    for chunk in &mut chunks {
        encode_triple(chunk, &mut output);
    }

    if !chunks.remainder().is_empty() {
        encode_triple(chunks.remainder(), &mut output);
    }

    output
}

pub fn decode(input: &str) -> Result<Vec<u8>, DecodeError> {
    let mut decoder = Decoder::new();
    let mut output = decoder.push(input)?;
    output.extend(decoder.finish()?);
    Ok(output)
}

pub struct Encoder {
    tail: [u8; 2],
    tail_len: usize,
    finished: bool,
}

impl Encoder {
    pub fn new() -> Self {
        Self {
            tail: [0; 2],
            tail_len: 0,
            finished: false,
        }
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, StreamError> {
        if self.finished {
            return Err(StreamError::Finished);
        }

        let mut output = String::with_capacity((input.len() + self.tail_len) / 3 * 4);
        let mut index = 0;

        if self.tail_len > 0 {
            while self.tail_len < 2 && index < input.len() {
                self.tail[self.tail_len] = input[index];
                self.tail_len += 1;
                index += 1;
            }

            if self.tail_len == 2 && index < input.len() {
                let triple = [self.tail[0], self.tail[1], input[index]];
                encode_triple(&triple, &mut output);
                self.tail_len = 0;
                index += 1;
            }
        }

        while index + 3 <= input.len() {
            encode_triple(&input[index..index + 3], &mut output);
            index += 3;
        }

        while index < input.len() {
            self.tail[self.tail_len] = input[index];
            self.tail_len += 1;
            index += 1;
        }

        Ok(output)
    }

    pub fn finish(&mut self) -> Result<String, StreamError> {
        if self.finished {
            return Err(StreamError::Finished);
        }

        self.finished = true;
        let mut output = String::new();
        if self.tail_len > 0 {
            encode_triple(&self.tail[..self.tail_len], &mut output);
            self.tail_len = 0;
        }
        Ok(output)
    }
}

impl Default for Encoder {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Decoder {
    quartet: [u8; 4],
    quartet_len: usize,
    padded: bool,
    finished: bool,
    error: Option<DecodeError>,
}

impl Decoder {
    pub fn new() -> Self {
        Self {
            quartet: [0; 4],
            quartet_len: 0,
            padded: false,
            finished: false,
            error: None,
        }
    }

    fn fail<T>(&mut self, error: DecodeError) -> Result<T, DecodeError> {
        self.error = Some(error);
        Err(error)
    }

    pub fn push(&mut self, input: &str) -> Result<Vec<u8>, DecodeError> {
        if let Some(error) = self.error {
            return Err(error);
        }
        if self.finished {
            return Err(DecodeError::Finished);
        }

        let mut output = Vec::with_capacity(input.len() / 4 * 3);

        for byte in input.bytes() {
            if byte.is_ascii_whitespace() {
                continue;
            }

            if self.padded {
                return self.fail(DecodeError::InvalidPadding);
            }

            let position = self.quartet_len;
            if byte == b'=' {
                if position < 2 {
                    return self.fail(DecodeError::InvalidPadding);
                }
                if position == 2 || position == 3 {
                    self.quartet[position] = byte;
                    self.quartet_len += 1;
                }
            } else {
                if position == 3 && self.quartet[2] == b'=' {
                    return self.fail(DecodeError::InvalidPadding);
                }
                let Some(_) = sextet(byte) else {
                    return self.fail(DecodeError::InvalidByte);
                };
                self.quartet[position] = byte;
                self.quartet_len += 1;
            }

            if self.quartet_len == 4 {
                if self.quartet[2] == b'=' && self.quartet[3] != b'=' {
                    return self.fail(DecodeError::InvalidPadding);
                }
                decode_quartet(&self.quartet, &mut output);
                self.padded = self.quartet[2] == b'=' || self.quartet[3] == b'=';
                self.quartet_len = 0;
            }
        }

        Ok(output)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, DecodeError> {
        if let Some(error) = self.error {
            return Err(error);
        }
        if self.finished {
            return Err(DecodeError::Finished);
        }

        if self.quartet_len != 0 {
            return self.fail(DecodeError::InvalidLength);
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