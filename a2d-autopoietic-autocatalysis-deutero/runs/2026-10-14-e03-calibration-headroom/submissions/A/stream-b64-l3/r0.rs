#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    InvalidByte { offset: usize },
    InvalidPadding { offset: usize },
    InvalidLength { offset: usize },
    TrailingData { offset: usize },
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamError {
    Finished,
}

pub struct Encoder {
    tail: [u8; 2],
    tail_len: usize,
    finished: bool,
}

pub struct Decoder {
    ignore_whitespace: bool,
    quartet: [u8; 4],
    quartet_len: usize,
    offset: usize,
    padded: bool,
    finished: bool,
    error: Option<DecodeError>,
}

const ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn encode_triple(a: u8, b: u8, c: u8, out: &mut String) {
    out.push(ALPHABET[(a >> 2) as usize] as char);
    out.push(ALPHABET[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);
    out.push(ALPHABET[(((b & 0x0f) << 2) | (c >> 6)) as usize] as char);
    out.push(ALPHABET[(c & 0x3f) as usize] as char);
}

fn encode_tail(tail: &[u8], out: &mut String) {
    match tail {
        [] => {}
        [a] => {
            out.push(ALPHABET[(a >> 2) as usize] as char);
            out.push(ALPHABET[((a & 0x03) << 4) as usize] as char);
            out.push('=');
            out.push('=');
        }
        [a, b] => {
            out.push(ALPHABET[(a >> 2) as usize] as char);
            out.push(ALPHABET[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);
            out.push(ALPHABET[((b & 0x0f) << 2) as usize] as char);
            out.push('=');
        }
        _ => unreachable!(),
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

fn is_ignored_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\x0b' | b'\x0c' | b'\r')
}

fn decode_quartet(q: &[u8; 4]) -> Vec<u8> {
    let a = sextet(q[0]).unwrap();
    let b = sextet(q[1]).unwrap();
    let mut out = vec![(a << 2) | (b >> 4)];

    if q[2] != b'=' {
        let c = sextet(q[2]).unwrap();
        out.push((b << 4) | (c >> 2));
        if q[3] != b'=' {
            let d = sextet(q[3]).unwrap();
            out.push((c << 6) | d);
        }
    }
    out
}

pub fn encode(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    let mut chunks = input.chunks_exact(3);
    for chunk in &mut chunks {
        encode_triple(chunk[0], chunk[1], chunk[2], &mut out);
    }
    encode_tail(chunks.remainder(), &mut out);
    out
}

pub fn decode(input: &str, ignore_whitespace: bool) -> Result<Vec<u8>, DecodeError> {
    let mut decoder = Decoder::new(ignore_whitespace);
    let mut out = decoder.push(input)?;
    out.extend(decoder.finish()?);
    Ok(out)
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

        let mut out = String::with_capacity((self.tail_len + input.len()) / 3 * 4);
        let mut index = 0;

        if self.tail_len > 0 {
            while self.tail_len < 3 && index < input.len() {
                if self.tail_len < 2 {
                    self.tail[self.tail_len] = input[index];
                }
                self.tail_len += 1;
                index += 1;
            }

            if self.tail_len == 3 {
                encode_triple(self.tail[0], self.tail[1], input[index - 1], &mut out);
                self.tail_len = 0;
            } else {
                return Ok(out);
            }
        }

        while index + 3 <= input.len() {
            encode_triple(
                input[index],
                input[index + 1],
                input[index + 2],
                &mut out,
            );
            index += 3;
        }

        self.tail_len = input.len() - index;
        for i in 0..self.tail_len {
            self.tail[i] = input[index + i];
        }

        Ok(out)
    }

    pub fn finish(&mut self) -> Result<String, StreamError> {
        if self.finished {
            return Err(StreamError::Finished);
        }
        self.finished = true;
        let mut out = String::new();
        encode_tail(&self.tail[..self.tail_len], &mut out);
        self.tail_len = 0;
        Ok(out)
    }
}

impl Decoder {
    pub fn new(ignore_whitespace: bool) -> Self {
        Self {
            ignore_whitespace,
            quartet: [0; 4],
            quartet_len: 0,
            offset: 0,
            padded: false,
            finished: false,
            error: None,
        }
    }

    pub fn push(&mut self, input: &str) -> Result<Vec<u8>, DecodeError> {
        if let Some(error) = self.error {
            return Err(error);
        }
        if self.finished {
            return Err(DecodeError::Finished);
        }

        let mut out = Vec::new();
        for byte in input.bytes() {
            let current_offset = self.offset;
            self.offset += 1;

            if self.ignore_whitespace && is_ignored_whitespace(byte) {
                continue;
            }

            let error = if self.padded {
                Some(DecodeError::TrailingData {
                    offset: current_offset,
                })
            } else if byte == b'=' {
                if self.quartet_len < 2 {
                    Some(DecodeError::InvalidPadding {
                        offset: current_offset,
                    })
                } else if self.quartet_len == 2 || self.quartet_len == 3 {
                    self.quartet[self.quartet_len] = byte;
                    self.quartet_len += 1;
                    if self.quartet_len == 4 {
                        out.extend(decode_quartet(&self.quartet));
                        self.quartet_len = 0;
                        self.padded = true;
                    }
                    None
                } else {
                    Some(DecodeError::InvalidPadding {
                        offset: current_offset,
                    })
                }
            } else if self.quartet_len == 3 && self.quartet[2] == b'=' {
                Some(DecodeError::InvalidPadding {
                    offset: current_offset,
                })
            } else if let Some(value) = sextet(byte) {
                self.quartet[self.quartet_len] = value;
                self.quartet_len += 1;
                if self.quartet_len == 4 {
                    out.extend(decode_quartet(&self.quartet));
                    self.quartet_len = 0;
                }
                None
            } else {
                Some(DecodeError::InvalidByte {
                    offset: current_offset,
                })
            };

            if let Some(error) = error {
                self.error = Some(error);
                return Err(error);
            }
        }
        Ok(out)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, DecodeError> {
        if let Some(error) = self.error {
            return Err(error);
        }
        if self.finished {
            return Err(DecodeError::Finished);
        }

        if self.quartet_len != 0 {
            let error = DecodeError::InvalidLength {
                offset: self.offset,
            };
            self.error = Some(error);
            return Err(error);
        }

        self.finished = true;
        Ok(Vec::new())
    }
}