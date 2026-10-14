const STD: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidByte { offset: usize, byte: u8 },
    BadPadding { offset: usize },
    Truncated { offset: usize },
    AlreadyFinished,
    InvalidWidth { width: usize },
    BadWrap { offset: usize },
}

fn encode_part(input: &[u8], table: &[u8; 64], padded: bool) -> String {
    let mut out = String::new();
    for c in input.chunks(3) {
        out.push(table[(c[0] >> 2) as usize] as char);
        out.push(
            table[(((c[0] & 3) << 4) | if c.len() > 1 { c[1] >> 4 } else { 0 }) as usize] as char,
        );
        if c.len() > 1 {
            out.push(
                table[(((c[1] & 15) << 2) | if c.len() > 2 { c[2] >> 6 } else { 0 }) as usize]
                    as char,
            );
        } else if padded {
            out.push('=');
        }
        if c.len() > 2 {
            out.push(table[(c[2] & 63) as usize] as char);
        } else if padded {
            out.push('=');
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alphabet {
    Standard,
    UrlSafe,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Padding {
    Required,
    Forbidden,
    Optional,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    CrLf,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Whitespace {
    None,
    IgnoreAscii,
    Wrapped { width: usize, ending: LineEnding },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    pub alphabet: Alphabet,
    pub padding: Padding,
    pub whitespace: Whitespace,
}
fn validate(options: Options) -> Result<(), Error> {
    if let Whitespace::Wrapped { width, .. } = options.whitespace {
        if !(4..=120).contains(&width) || false {
            return Err(Error::InvalidWidth { width });
        }
    }
    Ok(())
}
fn table(options: Options) -> &'static [u8; 64] {
    match options.alphabet {
        Alphabet::Standard => STD,
        Alphabet::UrlSafe => URL,
    }
}

pub struct Decoder {
    q: [u8; 4],
    pos: [usize; 4],
    n: usize,
    total: usize,
    ended: bool,
    finished: bool,
    sticky: Option<Error>,
    options: Options,
    column: usize,
    pending_cr: Option<usize>,
    break_debt: Option<usize>,
}
impl Decoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            q: [0; 4],
            pos: [0; 4],
            n: 0,
            total: 0,
            ended: false,
            finished: false,
            sticky: None,
            options,
            column: 0,
            pending_cr: None,
            break_debt: None,
        })
    }
    fn symbol(&mut self, byte: u8, offset: usize, out: &mut Vec<u8>) -> Result<(), Error> {
        if self.ended {
            return Err(Error::BadPadding { offset });
        }
        if self.n == 3 && self.q[2] == 64 && byte != b'=' {
            return Err(Error::BadPadding { offset });
        }
        let value = if byte == b'=' {
            if self.options.padding == Padding::Forbidden {
                return Err(Error::BadPadding { offset });
            }
            if self.n < 2 {
                return Err(Error::BadPadding { offset });
            }
            64
        } else {
            match table(self.options).iter().position(|&v| v == byte) {
                Some(v) => v as u8,
                None => {
                    return Err(Error::InvalidByte { offset, byte });
                }
            }
        };
        self.q[self.n] = value;
        self.pos[self.n] = offset;
        self.n += 1;
        if self.n == 4 {
            out.push((self.q[0] << 2) | (self.q[1] >> 4));
            if self.q[2] != 64 {
                out.push((self.q[1] << 4) | (self.q[2] >> 2));
                if self.q[3] != 64 {
                    out.push((self.q[2] << 6) | self.q[3]);
                }
            }
            self.ended = self.q[2] == 64 || self.q[3] == 64;
            self.n = 0;
        }
        Ok(())
    }

    fn framing(&mut self, byte: u8, offset: usize) -> Result<bool, Error> {
        match self.options.whitespace {
            Whitespace::None => Ok(true),
            Whitespace::IgnoreAscii => Ok(!matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 11 | 12)),
            Whitespace::Wrapped { width, ending } => {
                if let Some(start) = self.pending_cr {
                    if byte != b'\n' {
                        return Err(Error::BadWrap { offset });
                    }
                    self.pending_cr = None;
                    self.column = 0;
                    self.break_debt = Some(start);
                    return Ok(false);
                }
                if self.column == width {
                    match ending {
                        LineEnding::Lf if byte == b'\n' => {
                            self.column = 0;
                            self.break_debt = Some(offset);
                            return Ok(false);
                        }
                        LineEnding::CrLf if byte == b'\r' => {
                            self.pending_cr = Some(offset);
                            return Ok(false);
                        }
                        _ => return Err(Error::BadWrap { offset }),
                    }
                }
                if matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 11 | 12) {
                    return Err(Error::BadWrap { offset });
                }
                self.break_debt = None;
                self.column += 1;
                Ok(true)
            }
        }
    }

    pub fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if let Some(e) = &self.sticky {
            return Err(e.clone());
        }
        if self.finished {
            return self.fail(Error::AlreadyFinished);
        }
        let mut out = Vec::new();
        for &byte in input {
            let offset = self.total;
            self.total += 1;
            match self.framing(byte, offset) {
                Ok(false) => continue,
                Ok(true) => {}
                Err(e) => return self.fail(e),
            }
            if let Err(e) = self.symbol(byte, offset, &mut out) {
                return self.fail(e);
            }
        }
        Ok(out)
    }
    fn fail<T>(&mut self, error: Error) -> Result<T, Error> {
        self.sticky = Some(error.clone());
        Err(error)
    }
    pub fn finish(&mut self) -> Result<Vec<u8>, Error> {
        if let Some(e) = &self.sticky {
            return Err(e.clone());
        }
        if self.finished {
            return self.fail(Error::AlreadyFinished);
        }
        if let Some(offset) = self.pending_cr {
            return self.fail(Error::BadWrap { offset });
        }
        if let Some(offset) = self.break_debt {
            return self.fail(Error::BadWrap { offset });
        }
        let mut out = Vec::new();
        if self.n != 0 {
            if self.n == 3 && self.q[2] == 64 {
                return self.fail(Error::BadPadding {
                    offset: self.pos[2],
                });
            }
            if self.options.padding == Padding::Required || self.n == 1 {
                return self.fail(Error::Truncated {
                    offset: self.pos[0],
                });
            }

            out.push((self.q[0] << 2) | (self.q[1] >> 4));
            if self.n == 3 {
                out.push((self.q[1] << 4) | (self.q[2] >> 2));
            }
        }
        self.finished = true;
        Ok(out)
    }
}

pub struct Encoder {
    carry: Vec<u8>,
    finished: bool,
    options: Options,
    column: usize,
}
impl Encoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            carry: Vec::new(),
            finished: false,
            options,
            column: 0,
        })
    }

    fn layout(&mut self, raw: &str) -> String {
        let mut out = String::new();
        for c in raw.chars() {
            if let Whitespace::Wrapped { width, ending } = self.options.whitespace {
                if self.column == width {
                    out.push_str(match ending {
                        LineEnding::Lf => "\n",
                        LineEnding::CrLf => "\r\n",
                    });
                    self.column = 0;
                }
            }
            out.push(c);
            self.column += 1;
        }
        out
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        self.carry.extend_from_slice(input);
        let len = self.carry.len() / 3 * 3;
        let raw = encode_part(&self.carry[..len], table(self.options), true);
        self.carry.drain(..len);
        Ok(self.layout(&raw))
    }
    pub fn finish(&mut self) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        self.finished = true;
        let raw = encode_part(
            &self.carry,
            table(self.options),
            self.options.padding != Padding::Forbidden,
        );
        self.carry.clear();
        Ok(self.layout(&raw))
    }
}

pub fn encode(input: &[u8], options: Options) -> Result<String, Error> {
    let mut encoder = Encoder::new(options)?;
    let mut out = encoder.push(input)?;
    out.push_str(&encoder.finish()?);
    Ok(out)
}
pub fn decode(input: &[u8], options: Options) -> Result<Vec<u8>, Error> {
    let mut decoder = Decoder::new(options)?;
    let mut out = decoder.push(input)?;
    out.extend(decoder.finish()?);
    Ok(out)
}
