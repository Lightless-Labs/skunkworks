#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidByte { offset: usize, byte: u8 },
    BadPadding { offset: usize },
    Truncated { offset: usize },
    AlreadyFinished,
}

const ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn digit_value(byte: u8) -> Option<u8> {
    ALPHABET.iter().position(|&b| b == byte).map(|n| n as u8)
}

fn encode_group(a: u8, b: u8, c: u8) -> [u8; 4] {
    [
        ALPHABET[(a >> 2) as usize],
        ALPHABET[(((a & 0x03) << 4) | (b >> 4)) as usize],
        ALPHABET[(((b & 0x0f) << 2) | (c >> 6)) as usize],
        ALPHABET[(c & 0x3f) as usize],
    ]
}

fn encode_tail(tail: &[u8]) -> String {
    let mut output = String::new();
    match tail {
        [] => {}
        [a] => {
            output.push(char::from(ALPHABET[(a >> 2) as usize]));
            output.push(char::from(ALPHABET[((a & 0x03) << 4) as usize]));
            output.push_str("==");
        }
        [a, b] => {
            output.push(char::from(ALPHABET[(a >> 2) as usize]));
            output.push(char::from(
                ALPHABET[(((a & 0x03) << 4) | (b >> 4)) as usize],
            ));
            output.push(char::from(ALPHABET[((b & 0x0f) << 2) as usize]));
            output.push('=');
        }
        _ => unreachable!(),
    }
    output
}

pub fn encode(input: &[u8]) -> String {
    let mut output = String::with_capacity(input.len().div_ceil(3) * 4);
    let mut chunks = input.chunks_exact(3);

    for chunk in &mut chunks {
        output.extend(encode_group(chunk[0], chunk[1], chunk[2]).map(char::from));
    }

    output.push_str(&encode_tail(chunks.remainder()));
    output
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DecoderState {
    group: [u8; 4],
    len: usize,
    pending_position_two_padding: Option<usize>,
    group_start: usize,
    terminal_padding: bool,
    next_offset: usize,
}

impl DecoderState {
    fn new() -> Self {
        Self {
            group: [0; 4],
            len: 0,
            pending_position_two_padding: None,
            group_start: 0,
            terminal_padding: false,
            next_offset: 0,
        }
    }

    fn scan_byte(&mut self, byte: u8, offset: usize, output: &mut Vec<u8>) -> Result<(), Error> {
        if self.terminal_padding {
            return Err(Error::BadPadding { offset });
        }

        if let Some(padding_offset) = self.pending_position_two_padding {
            if byte != b'=' {
                return Err(Error::BadPadding { offset });
            }
            self.group[2] = b'=';
            self.group[3] = b'=';
            self.len = 4;
            self.pending_position_two_padding = None;
            self.emit_group(output);
            self.terminal_padding = true;
            let _ = padding_offset;
            return Ok(());
        }

        if byte == b'=' {
            match self.len {
                0 | 1 => return Err(Error::BadPadding { offset }),
                2 => {
                    self.pending_position_two_padding = Some(offset);
                    return Ok(());
                }
                3 => {
                    self.group[3] = b'=';
                    self.len = 4;
                    self.emit_group(output);
                    self.terminal_padding = true;
                    return Ok(());
                }
                _ => unreachable!(),
            }
        }

        let value = digit_value(byte).ok_or(Error::InvalidByte { offset, byte })?;
        if self.len == 0 {
            self.group_start = offset;
        }
        self.group[self.len] = value;
        self.len += 1;

        if self.len == 4 {
            self.emit_group(output);
            self.len = 0;
        }

        Ok(())
    }

    fn emit_group(&self, output: &mut Vec<u8>) {
        let a = self.group[0];
        let b = self.group[1];
        output.push((a << 2) | (b >> 4));

        if self.group[2] == b'=' {
            return;
        }

        let c = self.group[2];
        output.push((b << 4) | (c >> 2));

        if self.group[3] == b'=' {
            return;
        }

        let d = self.group[3];
        output.push((c << 6) | d);
    }

    fn finish(&self) -> Result<Vec<u8>, Error> {
        if let Some(offset) = self.pending_position_two_padding {
            return Err(Error::BadPadding { offset });
        }

        if self.len != 0 && !self.terminal_padding {
            return Err(Error::Truncated {
                offset: self.group_start,
            });
        }

        Ok(Vec::new())
    }
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

        let mut combined = Vec::with_capacity(self.tail.len() + input.len());
        combined.extend_from_slice(&self.tail);
        combined.extend_from_slice(input);

        let complete_len = combined.len() / 3 * 3;
        let mut output = String::with_capacity(complete_len / 3 * 4);
        for chunk in combined[..complete_len].chunks_exact(3) {
            output.extend(encode_group(chunk[0], chunk[1], chunk[2]).map(char::from));
        }

        self.tail.clear();
        self.tail.extend_from_slice(&combined[complete_len..]);
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        self.finished = true;
        let output = encode_tail(&self.tail);
        self.tail.clear();
        Ok(output)
    }
}

pub struct Decoder {
    state: DecoderState,
    finished: bool,
    sticky_error: Option<Error>,
}

impl Decoder {
    pub fn new() -> Self {
        Self {
            state: DecoderState::new(),
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
            let offset = self.state.next_offset;
            self.state.next_offset += 1;

            if let Err(error) = self.state.scan_byte(byte, offset, &mut output) {
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

        match self.state.finish() {
            Ok(output) => {
                self.finished = true;
                Ok(output)
            }
            Err(error) => {
                self.sticky_error = Some(error.clone());
                Err(error)
            }
        }
    }
}