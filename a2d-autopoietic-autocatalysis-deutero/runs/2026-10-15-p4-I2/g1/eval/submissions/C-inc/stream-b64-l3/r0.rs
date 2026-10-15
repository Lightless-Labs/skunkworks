use std::fmt;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidByte { offset: usize, byte: u8 },
    BadPadding { offset: usize },
    Truncated { offset: usize },
    AlreadyFinished,
    InvalidWidth { width: usize },
    BadWrap { offset: usize },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Error {}

fn validate(options: Options) -> Result<(), Error> {
    if let Whitespace::Wrapped { width, .. } = options.whitespace {
        if !(4..=120).contains(&width) || width % 4 != 0 {
            return Err(Error::InvalidWidth { width });
        }
    }
    Ok(())
}

fn alphabet_byte(alphabet: Alphabet, value: u8) -> u8 {
    const TABLE: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut byte = TABLE[value as usize];
    if alphabet == Alphabet::UrlSafe {
        if byte == b'+' {
            byte = b'-';
        } else if byte == b'/' {
            byte = b'_';
        }
    }
    byte
}

fn digit(alphabet: Alphabet, byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' if alphabet == Alphabet::Standard => Some(62),
        b'/' if alphabet == Alphabet::Standard => Some(63),
        b'-' if alphabet == Alphabet::UrlSafe => Some(62),
        b'_' if alphabet == Alphabet::UrlSafe => Some(63),
        _ => None,
    }
}

fn encode_raw(input: &[u8], alphabet: Alphabet, padding: Padding) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len().div_ceil(3) * 4);
    let mut chunks = input.chunks_exact(3);
    for chunk in &mut chunks {
        let n = ((chunk[0] as u32) << 16)
            | ((chunk[1] as u32) << 8)
            | chunk[2] as u32;
        for shift in [18, 12, 6, 0] {
            out.push(alphabet_byte(alphabet, ((n >> shift) & 63) as u8));
        }
    }

    let rem = chunks.remainder();
    if !rem.is_empty() {
        let n = ((rem[0] as u32) << 16)
            | if rem.len() == 2 {
                (rem[1] as u32) << 8
            } else {
                0
            };
        out.push(alphabet_byte(alphabet, ((n >> 18) & 63) as u8));
        out.push(alphabet_byte(alphabet, ((n >> 12) & 63) as u8));
        if rem.len() == 2 {
            out.push(alphabet_byte(alphabet, ((n >> 6) & 63) as u8));
            if padding != Padding::Forbidden {
                out.push(b'=');
            }
        } else if padding != Padding::Forbidden {
            out.extend_from_slice(b"==");
        }
    }
    out
}

fn wrap_encoded(raw: &[u8], whitespace: Whitespace) -> String {
    match whitespace {
        Whitespace::Wrapped { width, ending } if raw.len() > width => {
            let separator: &[u8] = match ending {
                LineEnding::Lf => b"\n",
                LineEnding::CrLf => b"\r\n",
            };
            let mut out = Vec::with_capacity(raw.len() + raw.len() / width * separator.len());
            for (i, chunk) in raw.chunks(width).enumerate() {
                if i != 0 {
                    out.extend_from_slice(separator);
                }
                out.extend_from_slice(chunk);
            }
            String::from_utf8(out).unwrap()
        }
        _ => String::from_utf8(raw.to_vec()).unwrap(),
    }
}

pub fn encode(input: &[u8], options: Options) -> Result<String, Error> {
    validate(options)?;
    let raw = encode_raw(input, options.alphabet, options.padding);
    Ok(wrap_encoded(&raw, options.whitespace))
}

pub struct Encoder {
    options: Options,
    tail: Vec<u8>,
    raw_symbols: usize,
    closed: bool,
}

impl Encoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            tail: Vec::new(),
            raw_symbols: 0,
            closed: false,
        })
    }

    fn format_symbols(&mut self, raw: &[u8]) -> String {
        let mut out = Vec::new();
        match self.options.whitespace {
            Whitespace::Wrapped { width, ending } => {
                for &byte in raw {
                    if self.raw_symbols > 0 && self.raw_symbols % width == 0 {
                        match ending {
                            LineEnding::Lf => out.push(b'\n'),
                            LineEnding::CrLf => out.extend_from_slice(b"\r\n"),
                        }
                    }
                    out.push(byte);
                    self.raw_symbols += 1;
                }
            }
            _ => {
                out.extend_from_slice(raw);
                self.raw_symbols += raw.len();
            }
        }
        String::from_utf8(out).unwrap()
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        if input.is_empty() {
            return Ok(String::new());
        }

        let mut all = std::mem::take(&mut self.tail);
        all.extend_from_slice(input);
        let complete_len = all.len() / 3 * 3;
        self.tail.extend_from_slice(&all[complete_len..]);
        let raw = encode_raw(
            &all[..complete_len],
            self.options.alphabet,
            Padding::Forbidden,
        );
        Ok(self.format_symbols(&raw))
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        self.closed = true;
        let raw = encode_raw(&self.tail, self.options.alphabet, self.options.padding);
        self.tail.clear();
        Ok(self.format_symbols(&raw))
    }
}

#[derive(Clone, Copy)]
enum WrapState {
    Row,
    NeedLf { first: usize },
    NeedCrLf { cr: usize },
    NeedSymbol { first: usize },
}

pub struct Decoder {
    options: Options,
    offset: usize,
    group: Vec<(u8, usize)>,
    terminal: bool,
    pending_second_pad: Option<usize>,
    wrap_state: WrapState,
    row_symbols: usize,
    closed: bool,
    sticky: Option<Error>,
}

impl Decoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            offset: 0,
            group: Vec::with_capacity(4),
            terminal: false,
            pending_second_pad: None,
            wrap_state: WrapState::Row,
            row_symbols: 0,
            closed: false,
            sticky: None,
        })
    }

    fn remember_error(&mut self, error: Error) -> Error {
        self.sticky = Some(error.clone());
        error
    }

    fn push_symbol(&mut self, byte: u8, at: usize, out: &mut Vec<u8>) -> Result<(), Error> {
        if self.terminal {
            return Err(Error::BadPadding { offset: at });
        }

        if let Some(pos2) = self.pending_second_pad {
            if byte != b'=' {
                return Err(Error::BadPadding { offset: at });
            }
            self.pending_second_pad = None;
            self.group.push((byte, at));

            let a = digit(self.options.alphabet, self.group[0].0).unwrap();
            let b = digit(self.options.alphabet, self.group[1].0).unwrap();
            out.push((a << 2) | (b >> 4));
            self.group.clear();
            self.terminal = true;
            return Ok(());
        }

        let pos = self.group.len();
        if byte == b'=' {
            match pos {
                0 | 1 => return Err(Error::BadPadding { offset: at }),
                2 => {
                    if self.options.padding == Padding::Forbidden {
                        return Err(Error::BadPadding { offset: at });
                    }
                    self.pending_second_pad = Some(at);
                    self.group.push((byte, at));
                    return Ok(());
                }
                3 => {
                    if self.options.padding == Padding::Forbidden {
                        return Err(Error::BadPadding { offset: at });
                    }
                    self.group.push((byte, at));
                    let a = digit(self.options.alphabet, self.group[0].0).unwrap();
                    let b = digit(self.options.alphabet, self.group[1].0).unwrap();
                    let c = digit(self.options.alphabet, self.group[2].0).unwrap();
                    out.push((a << 2) | (b >> 4));
                    out.push((b << 4) | (c >> 2));
                    self.group.clear();
                    self.terminal = true;
                    return Ok(());
                }
                _ => unreachable!(),
            }
        }

        let value = digit(self.options.alphabet, byte)
            .ok_or(Error::InvalidByte { offset: at, byte })?;
        self.group.push((byte, at));
        if self.group.len() == 4 {
            let a = digit(self.options.alphabet, self.group[0].0).unwrap();
            let b = digit(self.options.alphabet, self.group[1].0).unwrap();
            let c = digit(self.options.alphabet, self.group[2].0).unwrap();
            out.push((a << 2) | (b >> 4));
            out.push((b << 4) | (c >> 2));
            out.push((c << 6) | value);
            self.group.clear();
        }
        Ok(())
    }

    fn process_byte(
        &mut self,
        byte: u8,
        at: usize,
        out: &mut Vec<u8>,
    ) -> Result<(), Error> {
        match self.options.whitespace {
            Whitespace::None => self.push_symbol(byte, at, out),
            Whitespace::IgnoreAscii => {
                if matches!(byte, 9 | 10 | 11 | 12 | 13 | 32) {
                    Ok(())
                } else {
                    self.push_symbol(byte, at, out)
                }
            }
            Whitespace::Wrapped { width, ending } => {
                match self.wrap_state {
                    WrapState::NeedLf { .. } => {
                        if byte != b'\n' {
                            return Err(Error::BadWrap { offset: at });
                        }
                        self.wrap_state = WrapState::NeedSymbol {
                            first: at.saturating_sub(1),
                        };
                        Ok(())
                    }
                    WrapState::NeedCrLf { cr } => {
                        if byte != b'\n' {
                            return Err(Error::BadWrap { offset: at });
                        }
                        self.wrap_state = WrapState::NeedSymbol { first: cr };
                        Ok(())
                    }
                    WrapState::NeedSymbol { .. } => {
                        self.wrap_state = WrapState::Row;
                        self.row_symbols = 0;
                        self.process_wrapped_symbol(byte, at, out)
                    }
                    WrapState::Row => {
                        if self.row_symbols == width {
                            match ending {
                                LineEnding::Lf if byte == b'\n' => {
                                    self.wrap_state = WrapState::NeedSymbol { first: at };
                                    Ok(())
                                }
                                LineEnding::CrLf if byte == b'\r' => {
                                    self.wrap_state = WrapState::NeedCrLf { cr: at };
                                    Ok(())
                                }
                                _ => Err(Error::BadWrap { offset: at }),
                            }
                        } else if matches!(byte, 9 | 10 | 11 | 12 | 13 | 32) {
                            Err(Error::BadWrap { offset: at })
                        } else {
                            self.process_wrapped_symbol(byte, at, out)
                        }
                    }
                }
            }
        }
    }

    fn process_wrapped_symbol(
        &mut self,
        byte: u8,
        at: usize,
        out: &mut Vec<u8>,
    ) -> Result<(), Error> {
        self.row_symbols += 1;
        self.push_symbol(byte, at, out)
    }

    pub fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky {
            return Err(error.clone());
        }
        if self.closed {
            return Err(Error::AlreadyFinished);
        }

        let mut out = Vec::new();
        for &byte in input {
            let at = self.offset;
            self.offset += 1;
            if let Err(error) = self.process_byte(byte, at, &mut out) {
                return Err(self.remember_error(error));
            }
        }
        Ok(out)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky {
            return Err(error.clone());
        }
        if self.closed {
            return Err(Error::AlreadyFinished);
        }

        if let Whitespace::Wrapped { .. } = self.options.whitespace {
            match self.wrap_state {
                WrapState::NeedLf { first }
                | WrapState::NeedSymbol { first } => {
                    return Err(self.remember_error(Error::BadWrap { offset: first }));
                }
                WrapState::NeedCrLf { cr } => {
                    return Err(self.remember_error(Error::BadWrap { offset: cr }));
                }
                WrapState::Row => {}
            }
        }

        if let Some(pos) = self.pending_second_pad {
            return Err(self.remember_error(Error::BadPadding { offset: pos }));
        }

        let mut out = Vec::new();
        match self.group.len() {
            0 => {}
            1 => {
                return Err(self.remember_error(Error::Truncated {
                    offset: self.group[0].1,
                }));
            }
            2 | 3 => match self.options.padding {
                Padding::Required => {
                    return Err(self.remember_error(Error::Truncated {
                        offset: self.group[0].1,
                    }));
                }
                Padding::Forbidden | Padding::Optional => {
                    let a = digit(self.options.alphabet, self.group[0].0).unwrap();
                    let b = digit(self.options.alphabet, self.group[1].0).unwrap();
                    out.push((a << 2) | (b >> 4));
                    if self.group.len() == 3 {
                        let c = digit(self.options.alphabet, self.group[2].0).unwrap();
                        out.push((b << 4) | (c >> 2));
                    }
                }
            },
            _ => unreachable!(),
        }

        self.closed = true;
        Ok(out)
    }
}

pub fn decode(input: &[u8], options: Options) -> Result<Vec<u8>, Error> {
    validate(options)?;
    let mut decoder = Decoder::new(options)?;
    let mut out = decoder.push(input)?;
    out.extend(decoder.finish()?);
    Ok(out)
}

fn main() {}