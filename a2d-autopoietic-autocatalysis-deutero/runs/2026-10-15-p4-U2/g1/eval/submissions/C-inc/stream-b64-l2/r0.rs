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
    output.push_str(&encoder.finish().expect("new encoder is open"));
    output
}

pub fn decode(input: &[u8]) -> Result<Vec<u8>, Error> {
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

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }

        let mut output = String::new();
        let mut index = 0;

        if self.tail_len != 0 {
            while self.tail_len < 3 && index < input.len() {
                if self.tail_len < 2 {
                    self.tail[self.tail_len] = input[index];
                }
                self.tail_len += 1;
                index += 1;

                if self.tail_len == 3 {
                    encode_three(
                        self.tail[0],
                        self.tail[1],
                        input[index - 1],
                        &mut output,
                    );
                    self.tail_len = 0;
                }
            }
        }

        while index + 3 <= input.len() {
            encode_three(
                input[index],
                input[index + 1],
                input[index + 2],
                &mut output,
            );
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
            0 => {}
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
            _ => unreachable!(),
        }

        self.tail_len = 0;
        Ok(output)
    }
}

fn encode_three(a: u8, b: u8, c: u8, output: &mut String) {
    output.push(ALPHABET[(a >> 2) as usize] as char);
    output.push(ALPHABET[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);
    output.push(ALPHABET[(((b & 0x0f) << 2) | (c >> 6)) as usize] as char);
    output.push(ALPHABET[(c & 0x3f) as usize] as char);
}

pub struct Decoder {
    group: [u8; 4],
    offsets: [usize; 4],
    group_len: usize,
    pending_padding: Option<usize>,
    terminal_padding: bool,
    next_offset: usize,
    finished: bool,
    sticky_error: Option<Error>,
}

impl Decoder {
    pub fn new() -> Self {
        Self {
            group: [0; 4],
            offsets: [0; 4],
            group_len: 0,
            pending_padding: None,
            terminal_padding: false,
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

            if self.terminal_padding {
                return self.fail(Error::BadPadding { offset });
            }

            if self.pending_padding.is_some() && byte != b'=' {
                return self.fail(Error::BadPadding { offset });
            }

            let position = self.group_len;
            if byte == b'=' {
                if position == 0 || position == 1 {
                    return self.fail(Error::BadPadding { offset });
                }
                if position == 2 {
                    self.pending_padding = Some(offset);
                } else if position == 3 {
                    self.group[3] = 64;
                    self.offsets[3] = offset;
                    self.group_len = 4;
                    decode_group(&self.group, &mut output);
                    self.group_len = 0;
                    self.terminal_padding = true;
                    continue;
                }
            } else {
                let Some(value) = alphabet_value(byte) else {
                    return self.fail(Error::InvalidByte { offset, byte });
                };
                self.group[position] = value;
                self.offsets[position] = offset;
            }

            self.group_len += 1;
            if self.group_len == 4 {
                decode_group(&self.group, &mut output);
                self.group_len = 0;
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

        if let Some(offset) = self.pending_padding {
            return self.fail(Error::BadPadding { offset });
        }

        if self.group_len != 0 {
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

fn alphabet_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

fn decode_group(group: &[u8; 4], output: &mut Vec<u8>) {
    let a = group[0];
    let b = group[1];

    output.push((a << 2) | (b >> 4));

    if group[2] == 64 {
        return;
    }

    let c = group[2];
    output.push((b << 4) | (c >> 2));

    if group[3] == 64 {
        return;
    }

    output.push((c << 6) | group[3]);
}

fn main() {}