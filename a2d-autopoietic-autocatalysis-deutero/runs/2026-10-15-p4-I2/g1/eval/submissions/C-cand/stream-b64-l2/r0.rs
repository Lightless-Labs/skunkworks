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
    ALPHABET.iter().position(|&b| b == byte).map(|n| n as u8)
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
        output.push('=');
        return;
    }

    if bytes.len() > 2 {
        output.push(ALPHABET[(c & 0x3f) as usize] as char);
    } else {
        output.push('=');
    }
}

pub fn encode(input: &[u8]) -> String {
    let mut output = String::new();
    let mut chunks = input.chunks_exact(3);
    for chunk in &mut chunks {
        encode_group(chunk, &mut output);
    }
    if !chunks.remainder().is_empty() {
        encode_group(chunks.remainder(), &mut output);
    }
    output
}

#[derive(Clone)]
enum StreamState {
    Open,
    Finished,
    Failed(Error),
}

pub struct Encoder {
    pending: Vec<u8>,
    state: StreamState,
}

impl Encoder {
    pub fn new() -> Self {
        Self {
            pending: Vec::new(),
            state: StreamState::Open,
        }
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        match &self.state {
            StreamState::Finished => return Err(Error::AlreadyFinished),
            StreamState::Failed(error) => return Err(error.clone()),
            StreamState::Open => {}
        }

        let mut output = String::new();
        let mut index = 0;

        if !self.pending.is_empty() {
            while self.pending.len() < 3 && index < input.len() {
                self.pending.push(input[index]);
                index += 1;
            }
            if self.pending.len() == 3 {
                encode_group(&self.pending, &mut output);
                self.pending.clear();
            }
        }

        while index + 3 <= input.len() {
            encode_group(&input[index..index + 3], &mut output);
            index += 3;
        }

        self.pending.extend_from_slice(&input[index..]);
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        match &self.state {
            StreamState::Finished => return Err(Error::AlreadyFinished),
            StreamState::Failed(error) => return Err(error.clone()),
            StreamState::Open => {}
        }

        let mut output = String::new();
        if !self.pending.is_empty() {
            encode_group(&self.pending, &mut output);
            self.pending.clear();
        }
        self.state = StreamState::Finished;
        Ok(output)
    }
}

impl Default for Encoder {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Decoder {
    group: [u8; 4],
    group_offsets: [usize; 4],
    group_len: usize,
    offset: usize,
    terminal_padding: bool,
    state: StreamState,
}

impl Decoder {
    pub fn new() -> Self {
        Self {
            group: [0; 4],
            group_offsets: [0; 4],
            group_len: 0,
            offset: 0,
            terminal_padding: false,
            state: StreamState::Open,
        }
    }

    fn fail(&mut self, error: Error) -> Error {
        self.state = StreamState::Failed(error.clone());
        error
    }

    fn process_byte(&mut self, byte: u8, offset: usize, output: &mut Vec<u8>) -> Result<(), Error> {
        if self.terminal_padding {
            return Err(Error::BadPadding { offset });
        }

        if self.group_len == 3 && self.group[2] == b'=' && byte != b'=' {
            return Err(Error::BadPadding { offset });
        }

        let position = self.group_len;
        if byte == b'=' && position <= 1 {
            return Err(Error::BadPadding { offset });
        }

        if byte != b'=' && digit(byte).is_none() {
            return Err(Error::InvalidByte { offset, byte });
        }

        self.group[position] = byte;
        self.group_offsets[position] = offset;
        self.group_len += 1;

        if self.group_len == 4 {
            let a = digit(self.group[0]).expect("validated alphabet digit");
            let b = digit(self.group[1]).expect("validated alphabet digit");

            if self.group[2] == b'=' {
                if self.group[3] != b'=' {
                    return Err(Error::BadPadding {
                        offset: self.group_offsets[3],
                    });
                }
                output.push((a << 2) | (b >> 4));
                self.terminal_padding = true;
            } else {
                let c = digit(self.group[2]).expect("validated alphabet digit");
                if self.group[3] == b'=' {
                    output.push((a << 2) | (b >> 4));
                    output.push((b << 4) | (c >> 2));
                    self.terminal_padding = true;
                } else {
                    let d = digit(self.group[3]).expect("validated alphabet digit");
                    output.push((a << 2) | (b >> 4));
                    output.push((b << 4) | (c >> 2));
                    output.push((c << 6) | d);
                }
            }

            self.group_len = 0;
        }
        Ok(())
    }

    pub fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        match &self.state {
            StreamState::Finished => return Err(Error::AlreadyFinished),
            StreamState::Failed(error) => return Err(error.clone()),
            StreamState::Open => {}
        }

        let mut output = Vec::new();
        for &byte in input {
            let offset = self.offset;
            self.offset = self.offset.saturating_add(1);

            if let Err(error) = self.process_byte(byte, offset, &mut output) {
                return Err(self.fail(error));
            }
        }
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, Error> {
        match &self.state {
            StreamState::Finished => return Err(Error::AlreadyFinished),
            StreamState::Failed(error) => return Err(error.clone()),
            StreamState::Open => {}
        }

        if self.group_len != 0 {
            let error = if self.group_len == 3 && self.group[2] == b'=' {
                Error::BadPadding {
                    offset: self.group_offsets[2],
                }
            } else {
                Error::Truncated {
                    offset: self.group_offsets[0],
                }
            };
            return Err(self.fail(error));
        }

        self.state = StreamState::Finished;
        Ok(Vec::new())
    }
}

impl Default for Decoder {
    fn default() -> Self {
        Self::new()
    }
}

pub fn decode(input: &[u8]) -> Result<Vec<u8>, Error> {
    let mut decoder = Decoder::new();
    let mut output = decoder.push(input)?;
    output.extend(decoder.finish()?);
    Ok(output)
}

fn main() {}