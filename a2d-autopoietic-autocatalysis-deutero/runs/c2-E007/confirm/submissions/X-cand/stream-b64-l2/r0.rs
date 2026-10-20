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

        let mut output = String::with_capacity((input.len() + self.tail_len) / 3 * 4);
        let mut index = 0;

        if self.tail_len > 0 {
            while self.tail_len < 2 && index < input.len() {
                self.tail[self.tail_len] = input[index];
                self.tail_len += 1;
                index += 1;
            }
            if self.tail_len == 2 && index < input.len() {
                encode_three(
                    &mut output,
                    self.tail[0],
                    self.tail[1],
                    input[index],
                );
                self.tail_len = 0;
                index += 1;
            }
        }

        while index + 3 <= input.len() {
            encode_three(
                &mut output,
                input[index],
                input[index + 1],
                input[index + 2],
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

fn encode_three(output: &mut String, a: u8, b: u8, c: u8) {
    output.push(ALPHABET[(a >> 2) as usize] as char);
    output.push(ALPHABET[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);
    output.push(ALPHABET[(((b & 0x0f) << 2) | (c >> 6)) as usize] as char);
    output.push(ALPHABET[(c & 0x3f) as usize] as char);
}

pub struct Decoder {
    group: [u8; 4],
    offsets: [usize; 4],
    group_len: usize,
    pending_second_pad: Option<usize>,
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
            pending_second_pad: None,
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

        let mut output = Vec::with_capacity(input.len() / 4 * 3);
        for &byte in input {
            let offset = self.next_offset;
            self.next_offset = self.next_offset.saturating_add(1);

            let result = self.scan_byte(byte, offset, &mut output);
            if let Err(error) = result {
                self.sticky_error = Some(error.clone());
                return Err(error);
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

        if let Some(offset) = self.pending_second_pad {
            let error = Error::BadPadding { offset };
            self.sticky_error = Some(error.clone());
            return Err(error);
        }
        if self.group_len != 0 {
            let error = Error::Truncated {
                offset: self.offsets[0],
            };
            self.sticky_error = Some(error.clone());
            return Err(error);
        }

        self.finished = true;
        Ok(Vec::new())
    }

    fn scan_byte(
        &mut self,
        byte: u8,
        offset: usize,
        output: &mut Vec<u8>,
    ) -> Result<(), Error> {
        if self.terminal_padding {
            return Err(Error::BadPadding { offset });
        }

        if let Some(pad_offset) = self.pending_second_pad {
            if byte != b'=' {
                return Err(Error::BadPadding { offset });
            }
            self.pending_second_pad = None;
            self.terminal_padding = true;
            self.group[3] = b'=';
            self.offsets[3] = offset;
            self.emit_padded_group(output, true);
            let _ = pad_offset;
            return Ok(());
        }

        if byte == b'=' {
            match self.group_len {
                0 | 1 => return Err(Error::BadPadding { offset }),
                2 => {
                    self.pending_second_pad = Some(offset);
                    self.group[2] = b'=';
                    self.offsets[2] = offset;
                    self.group_len = 3;
                    return Ok(());
                }
                3 => {
                    self.group[3] = b'=';
                    self.offsets[3] = offset;
                    self.group_len = 4;
                    self.terminal_padding = true;
                    self.emit_padded_group(output, false);
                    return Ok(());
                }
                _ => unreachable!(),
            }
        }

        let value = alphabet_value(byte).ok_or(Error::InvalidByte { offset, byte })?;
        self.group[self.group_len] = value;
        self.offsets[self.group_len] = offset;
        self.group_len += 1;

        if self.group_len == 4 {
            output.push((self.group[0] << 2) | (self.group[1] >> 4));
            output.push((self.group[1] << 4) | (self.group[2] >> 2));
            output.push((self.group[2] << 6) | self.group[3]);
            self.group_len = 0;
        }
        Ok(())
    }

    fn emit_padded_group(&mut self, output: &mut Vec<u8>, double_pad: bool) {
        output.push((self.group[0] << 2) | (self.group[1] >> 4));
        if !double_pad {
            output.push((self.group[1] << 4) | (self.group[2] >> 2));
        }
        self.group_len = 0;
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