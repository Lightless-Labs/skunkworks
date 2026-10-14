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
    output.push_str(&encoder.finish().expect("encoder is still open"));
    output
}

pub fn decode(input: &[u8]) -> Result<Vec<u8>, Error> {
    let mut decoder = Decoder::new();
    let mut output = decoder.push(input)?;
    output.extend(decoder.finish()?);
    Ok(output)
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
        let mut combined = Vec::with_capacity(self.tail.len() + input.len());
        combined.extend_from_slice(&self.tail);
        combined.extend_from_slice(input);

        let complete = combined.len() / 3 * 3;
        for group in combined[..complete].chunks_exact(3) {
            encode_group(group, &mut output);
        }

        self.tail.clear();
        self.tail.extend_from_slice(&combined[complete..]);
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }

        let mut output = String::new();
        if !self.tail.is_empty() {
            encode_group(&self.tail, &mut output);
        }
        self.tail.clear();
        self.finished = true;
        Ok(output)
    }
}

#[derive(Clone, Copy)]
enum DecoderState {
    Collecting,
    PendingThirdPadding { offset: usize },
    TerminalPadding,
}

pub struct Decoder {
    group: [u8; 4],
    offsets: [usize; 4],
    len: usize,
    state: DecoderState,
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
            state: DecoderState::Collecting,
            next_offset: 0,
            finished: false,
            sticky_error: None,
        }
    }

    fn fail(&mut self, error: Error) -> Error {
        self.sticky_error = Some(error.clone());
        error
    }

    fn complete_group(&mut self, output: &mut Vec<u8>) {
        let a = digit(self.group[0]).expect("validated digit");
        let b = digit(self.group[1]).expect("validated digit");

        output.push((a << 2) | (b >> 4));

        if self.group[2] != b'=' {
            let c = digit(self.group[2]).expect("validated digit");
            output.push((b << 4) | (c >> 2));

            if self.group[3] != b'=' {
                let d = digit(self.group[3]).expect("validated digit");
                output.push((c << 6) | d);
            }
        }

        if self.group[2] == b'=' || self.group[3] == b'=' {
            self.state = DecoderState::TerminalPadding;
        }
        self.len = 0;
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

            match self.state {
                DecoderState::TerminalPadding => {
                    return Err(self.fail(Error::BadPadding { offset }));
                }
                DecoderState::PendingThirdPadding {
                    offset: padding_offset,
                } => {
                    if byte != b'=' {
                        return Err(self.fail(Error::BadPadding { offset }));
                    }

                    self.group[3] = b'=';
                    self.offsets[3] = offset;
                    self.complete_group(&mut output);
                    debug_assert!(matches!(self.state, DecoderState::TerminalPadding));
                    let _ = padding_offset;
                }
                DecoderState::Collecting => {
                    if self.len < 2 && byte == b'=' {
                        return Err(self.fail(Error::BadPadding { offset }));
                    }

                    if byte == b'=' {
                        if self.len == 2 {
                            self.group[2] = b'=';
                            self.offsets[2] = offset;
                            self.len = 3;
                            self.state =
                                DecoderState::PendingThirdPadding { offset };
                        } else if self.len == 3 {
                            self.group[3] = b'=';
                            self.offsets[3] = offset;
                            self.complete_group(&mut output);
                        }
                        continue;
                    }

                    if digit(byte).is_none() {
                        return Err(self.fail(Error::InvalidByte { offset, byte }));
                    }

                    self.group[self.len] = byte;
                    self.offsets[self.len] = offset;
                    self.len += 1;

                    if self.len == 4 {
                        self.complete_group(&mut output);
                    }
                }
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

        match self.state {
            DecoderState::PendingThirdPadding { offset } => {
                return Err(self.fail(Error::BadPadding { offset }));
            }
            DecoderState::Collecting if self.len != 0 => {
                return Err(self.fail(Error::Truncated {
                    offset: self.offsets[0],
                }));
            }
            DecoderState::Collecting | DecoderState::TerminalPadding => {}
        }

        self.finished = true;
        Ok(Vec::new())
    }
}