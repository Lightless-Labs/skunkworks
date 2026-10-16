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

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }

        let mut output = String::with_capacity((self.tail_len + input.len()) / 3 * 4);
        let mut index = 0;

        if self.tail_len > 0 {
            while self.tail_len < 3 && index < input.len() {
                if self.tail_len < 2 {
                    self.tail[self.tail_len] = input[index];
                } else {
                    let a = self.tail[0];
                    let b = self.tail[1];
                    let c = input[index];
                    encode_triplet(a, b, c, &mut output);
                    self.tail_len = 0;
                    index += 1;
                    break;
                }
                self.tail_len += 1;
                index += 1;
            }
        }

        while index + 3 <= input.len() {
            encode_triplet(input[index], input[index + 1], input[index + 2], &mut output);
            index += 3;
        }

        while index < input.len() {
            self.tail[self.tail_len] = input[index];
            self.tail_len += 1;
            index += 1;
        }

        Ok(output)
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }

        self.finished = true;
        let mut output = String::new();
        match self.tail_len {
            1 => {
                let a = self.tail[0];
                output.push(ALPHABET[(a >> 2) as usize] as char);
                output.push(ALPHABET[((a & 0x03) << 4) as usize] as char);
                output.push_str("==");
            }
            2 => {
                let a = self.tail[0];
                let b = self.tail[1];
                output.push(ALPHABET[(a >> 2) as usize] as char);
                output.push(ALPHABET[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);
                output.push(ALPHABET[((b & 0x0f) << 2) as usize] as char);
                output.push('=');
            }
            _ => {}
        }
        self.tail_len = 0;
        Ok(output)
    }
}

fn encode_triplet(a: u8, b: u8, c: u8, output: &mut String) {
    output.push(ALPHABET[(a >> 2) as usize] as char);
    output.push(ALPHABET[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);
    output.push(ALPHABET[(((b & 0x0f) << 2) | (c >> 6)) as usize] as char);
    output.push(ALPHABET[(c & 0x3f) as usize] as char);
}

pub fn encode(input: &[u8]) -> String {
    let mut encoder = Encoder::new();
    let mut output = encoder.push(input).expect("new encoder is open");
    output.push_str(&encoder.finish().expect("new encoder is open"));
    output
}

pub struct Decoder {
    group: [u8; 4],
    offsets: [usize; 4],
    len: usize,
    pending_pad: Option<usize>,
    terminal: bool,
    next_offset: usize,
    finished: bool,
    sticky_error: Option<Error>,
}

impl Decoder {
    pub fn new() -> Self {
        Self {
            group: [0; 4],
            offsets: [0; 4],
            len: 0,
            pending_pad: None,
            terminal: false,
            next_offset: 0,
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

        let mut output = Vec::new();
        for &byte in input {
            let offset = self.next_offset;
            self.next_offset += 1;

            if self.terminal {
                return self.fail(Error::BadPadding { offset });
            }

            if let Some(pad_offset) = self.pending_pad {
                if byte != b'=' {
                    return self.fail(Error::BadPadding { offset });
                }
                self.group[3] = 64;
                self.offsets[3] = offset;
                self.pending_pad = None;
                self.len = 4;
                emit_group(&self.group, &mut output);
                self.len = 0;
                self.terminal = true;
                let _ = pad_offset;
                continue;
            }

            if byte == b'=' {
                match self.len {
                    0 | 1 => return self.fail(Error::BadPadding { offset }),
                    2 => {
                        self.group[2] = 64;
                        self.offsets[2] = offset;
                        self.pending_pad = Some(offset);
                    }
                    3 => {
                        self.group[3] = 64;
                        self.offsets[3] = offset;
                        self.len = 4;
                        emit_group(&self.group, &mut output);
                        self.len = 0;
                        self.terminal = true;
                    }
                    _ => unreachable!(),
                }
                continue;
            }

            let Some(value) = decode_digit(byte) else {
                return self.fail(Error::InvalidByte { offset, byte });
            };

            self.group[self.len] = value;
            self.offsets[self.len] = offset;
            self.len += 1;

            if self.len == 4 {
                emit_group(&self.group, &mut output);
                self.len = 0;
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

        if let Some(offset) = self.pending_pad {
            return self.fail(Error::BadPadding { offset });
        }
        if self.len != 0 {
            return self.fail(Error::Truncated {
                offset: self.offsets[0],
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

fn decode_digit(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

fn emit_group(group: &[u8; 4], output: &mut Vec<u8>) {
    let a = group[0];
    let b = group[1];
    let c = group[2];
    let d = group[3];

    output.push((a << 2) | (b >> 4));
    if c != 64 {
        output.push((b << 4) | (c >> 2));
    }
    if d != 64 {
        output.push((c << 6) | d);
    }
}

pub fn decode(input: &[u8]) -> Result<Vec<u8>, Error> {
    let mut decoder = Decoder::new();
    let mut output = decoder.push(input)?;
    output.extend(decoder.finish()?);
    Ok(output)
}