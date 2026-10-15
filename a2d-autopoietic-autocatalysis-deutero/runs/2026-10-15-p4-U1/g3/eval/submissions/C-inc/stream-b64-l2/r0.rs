#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidByte { offset: usize, byte: u8 },
    BadPadding { offset: usize },
    Truncated { offset: usize },
    AlreadyFinished,
}

const ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn encode(input: &[u8]) -> String {
    let mut encoder = Encoder::new();
    let mut output = encoder.push(input).expect("new encoder is open");
    output.push_str(&encoder.finish().expect("encoder is open"));
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

        let mut bytes = Vec::with_capacity(self.tail.len() + input.len());
        bytes.extend_from_slice(&self.tail);
        bytes.extend_from_slice(input);

        let complete_len = bytes.len() / 3 * 3;
        let mut output = String::with_capacity(complete_len / 3 * 4);

        for chunk in bytes[..complete_len].chunks_exact(3) {
            let a = chunk[0];
            let b = chunk[1];
            let c = chunk[2];
            output.push(ALPHABET[(a >> 2) as usize] as char);
            output.push(ALPHABET[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);
            output.push(ALPHABET[(((b & 0x0f) << 2) | (c >> 6)) as usize] as char);
            output.push(ALPHABET[(c & 0x3f) as usize] as char);
        }

        self.tail.clear();
        self.tail.extend_from_slice(&bytes[complete_len..]);
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        self.finished = true;

        let mut output = String::new();
        match self.tail.as_slice() {
            [] => {}
            [a] => {
                output.push(ALPHABET[(a >> 2) as usize] as char);
                output.push(ALPHABET[((a & 0x03) << 4) as usize] as char);
                output.push_str("==");
            }
            [a, b] => {
                output.push(ALPHABET[(a >> 2) as usize] as char);
                output.push(ALPHABET[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);
                output.push(ALPHABET[((b & 0x0f) << 2) as usize] as char);
                output.push('=');
            }
            _ => unreachable!(),
        }
        self.tail.clear();
        Ok(output)
    }
}

pub struct Decoder {
    group: Vec<(u8, usize)>,
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

    pub fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky_error {
            return Err(error.clone());
        }
        if self.finished {
            return Err(Error::AlreadyFinished);
        }

        let mut output = Vec::new();

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
                let values = [
                    (self.group[0].0, self.group[0].1),
                    (self.group[1].0, self.group[1].1),
                    (self.group[2].0, self.group[2].1),
                    (0, at),
                ];
                output.extend(decode_group(&values, true, true));
                self.group.clear();
                self.pending_second_padding = None;
                self.terminal_padding = true;
                let _ = padding_offset;
                continue;
            }

            let position = self.group.len();
            if byte == b'=' {
                match position {
                    0 | 1 => return self.fail(Error::BadPadding { offset: at }),
                    2 => {
                        self.group.push((byte, at));
                        self.pending_second_padding = Some(at);
                    }
                    3 => {
                        let values = [
                            (self.group[0].0, self.group[0].1),
                            (self.group[1].0, self.group[1].1),
                            (self.group[2].0, self.group[2].1),
                            (byte, at),
                        ];
                        output.extend(decode_group(&values, false, true));
                        self.group.clear();
                        self.terminal_padding = true;
                    }
                    _ => unreachable!(),
                }
                continue;
            }

            let Some(value) = digit(byte) else {
                return self.fail(Error::InvalidByte { offset: at, byte });
            };
            self.group.push((value, at));

            if self.group.len() == 4 {
                let values = [
                    self.group[0],
                    self.group[1],
                    self.group[2],
                    self.group[3],
                ];
                output.extend(decode_group(&values, false, false));
                self.group.clear();
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
            return self.fail(Error::BadPadding { offset });
        }
        if !self.group.is_empty() {
            return self.fail(Error::Truncated {
                offset: self.group[0].1,
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

fn decode_group(values: &[(u8, usize); 4], double_padding: bool, _padding: bool) -> Vec<u8> {
    let a = values[0].0;
    let b = values[1].0;
    let c = values[2].0;
    let d = values[3].0;

    let mut output = Vec::with_capacity(if double_padding { 1 } else if _padding { 2 } else { 3 });
    output.push((a << 2) | (b >> 4));
    if !double_padding {
        output.push((b << 4) | (c >> 2));
        if !_padding {
            output.push((c << 6) | d);
        }
    }
    output
}

fn main() {}