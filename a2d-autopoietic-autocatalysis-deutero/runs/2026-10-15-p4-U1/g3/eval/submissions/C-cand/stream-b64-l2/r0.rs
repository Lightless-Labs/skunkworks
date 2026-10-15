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
    ALPHABET.iter().position(|&b| b == byte).map(|i| i as u8)
}

fn encode_group(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(4);
    let a = bytes[0];
    let b = bytes.get(1).copied().unwrap_or(0);
    let c = bytes.get(2).copied().unwrap_or(0);

    out.push(ALPHABET[(a >> 2) as usize] as char);
    out.push(ALPHABET[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);

    if bytes.len() >= 2 {
        out.push(ALPHABET[(((b & 0x0f) << 2) | (c >> 6)) as usize] as char);
    } else {
        out.push('=');
    }

    if bytes.len() >= 3 {
        out.push(ALPHABET[(c & 0x3f) as usize] as char);
    } else {
        out.push('=');
    }
    out
}

pub fn encode(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        out.push_str(&encode_group(chunk));
    }
    out
}

#[derive(Clone, Copy)]
struct Symbol {
    value: u8,
    offset: usize,
}

fn decode_group(group: &[Symbol; 4]) -> Vec<u8> {
    let a = group[0].value;
    let b = group[1].value;
    let c = group[2].value;
    let d = group[3].value;

    let mut out = vec![(a << 2) | (b >> 4)];
    if c != 64 {
        out.push((b << 4) | (c >> 2));
    }
    if d != 64 {
        out.push((c << 6) | d);
    }
    out
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

        let mut data = Vec::with_capacity(self.tail.len() + input.len());
        data.extend_from_slice(&self.tail);
        data.extend_from_slice(input);

        let complete_len = data.len() / 3 * 3;
        let mut out = String::with_capacity(complete_len / 3 * 4);
        for chunk in data[..complete_len].chunks_exact(3) {
            out.push_str(&encode_group(chunk));
        }

        self.tail.clear();
        self.tail.extend_from_slice(&data[complete_len..]);
        Ok(out)
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        self.finished = true;
        let out = if self.tail.is_empty() {
            String::new()
        } else {
            encode_group(&self.tail)
        };
        self.tail.clear();
        Ok(out)
    }
}

enum DecodePhase {
    Open,
    PendingThird(Symbol),
    Terminal,
}

pub struct Decoder {
    group: Vec<Symbol>,
    phase: DecodePhase,
    offset: usize,
    finished: bool,
    sticky_error: Option<Error>,
}

impl Decoder {
    pub fn new() -> Self {
        Self {
            group: Vec::with_capacity(4),
            phase: DecodePhase::Open,
            offset: 0,
            finished: false,
            sticky_error: None,
        }
    }

    fn fail(&mut self, error: Error) -> Error {
        self.sticky_error = Some(error.clone());
        error
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

            if matches!(self.phase, DecodePhase::Terminal) {
                return Err(self.fail(Error::BadPadding { offset: at }));
            }

            if let DecodePhase::PendingThird(pending) = self.phase {
                if byte != b'=' {
                    return Err(self.fail(Error::BadPadding { offset: at }));
                }
                self.group.push(Symbol {
                    value: 64,
                    offset: at,
                });
                let group: [Symbol; 4] = self.group.clone().try_into().ok().unwrap();
                output.extend(decode_group(&group));
                self.group.clear();
                self.phase = DecodePhase::Terminal;
                let _ = pending;
                continue;
            }

            let position = self.group.len();
            if byte == b'=' {
                match position {
                    0 | 1 => {
                        return Err(self.fail(Error::BadPadding { offset: at }));
                    }
                    2 => {
                        let pending = Symbol {
                            value: 64,
                            offset: at,
                        };
                        self.group.push(pending);
                        self.phase = DecodePhase::PendingThird(pending);
                    }
                    3 => {
                        self.group.push(Symbol {
                            value: 64,
                            offset: at,
                        });
                        let group: [Symbol; 4] = self.group.clone().try_into().ok().unwrap();
                        output.extend(decode_group(&group));
                        self.group.clear();
                        self.phase = DecodePhase::Terminal;
                    }
                    _ => unreachable!(),
                }
                continue;
            }

            let Some(value) = digit(byte) else {
                return Err(self.fail(Error::InvalidByte { offset: at, byte }));
            };
            self.group.push(Symbol { value, offset: at });

            if self.group.len() == 4 {
                let group: [Symbol; 4] = self.group.clone().try_into().ok().unwrap();
                output.extend(decode_group(&group));
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

        if let DecodePhase::PendingThird(symbol) = self.phase {
            return Err(self.fail(Error::BadPadding {
                offset: symbol.offset,
            }));
        }

        if !self.group.is_empty() {
            return Err(self.fail(Error::Truncated {
                offset: self.group[0].offset,
            }));
        }

        self.finished = true;
        Ok(Vec::new())
    }
}

pub fn decode(input: &[u8]) -> Result<Vec<u8>, Error> {
    let mut decoder = Decoder::new();
    let mut output = decoder.push(input)?;
    output.extend(decoder.finish()?);
    Ok(output)
}

fn main() {}