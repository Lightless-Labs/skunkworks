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

fn alphabet_bytes(alphabet: Alphabet) -> &'static [u8; 64] {
    match alphabet {
        Alphabet::Standard => b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/",
        Alphabet::UrlSafe => b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_",
    }
}

fn digit(byte: u8, alphabet: Alphabet) -> Option<u8> {
    let table = alphabet_bytes(alphabet);
    table.iter().position(|&b| b == byte).map(|n| n as u8)
}

fn encode_group(input: &[u8], alphabet: Alphabet, padding: Padding) -> Vec<u8> {
    let table = alphabet_bytes(alphabet);
    let a = input[0];
    let b = *input.get(1).unwrap_or(&0);
    let c = *input.get(2).unwrap_or(&0);
    let mut out = Vec::with_capacity(4);
    out.push(table[(a >> 2) as usize]);
    out.push(table[(((a & 0x03) << 4) | (b >> 4)) as usize]);
    if input.len() >= 2 {
        out.push(table[(((b & 0x0f) << 2) | (c >> 6)) as usize]);
    } else if padding != Padding::Forbidden {
        out.push(b'=');
    }
    if input.len() >= 3 {
        out.push(table[(c & 0x3f) as usize]);
    } else if padding != Padding::Forbidden {
        out.push(b'=');
    }
    out
}

fn wrap_symbols(symbols: Vec<u8>, whitespace: Whitespace) -> String {
    let Whitespace::Wrapped { width, ending } = whitespace else {
        return String::from_utf8(symbols).expect("base64 alphabet is ASCII");
    };
    if symbols.is_empty() {
        return String::new();
    }
    let sep: &[u8] = match ending {
        LineEnding::Lf => b"\n",
        LineEnding::CrLf => b"\r\n",
    };
    let mut out = Vec::with_capacity(symbols.len() + symbols.len() / width * sep.len());
    for (i, chunk) in symbols.chunks(width).enumerate() {
        if i != 0 {
            out.extend_from_slice(sep);
        }
        out.extend_from_slice(chunk);
    }
    String::from_utf8(out).expect("base64 output is ASCII")
}

pub fn encode(input: &[u8], options: Options) -> Result<String, Error> {
    validate(options)?;
    let mut encoder = Encoder::new(options)?;
    let mut out = encoder.push(input)?;
    out.push_str(&encoder.finish()?);
    Ok(out)
}

pub fn decode(input: &[u8], options: Options) -> Result<Vec<u8>, Error> {
    validate(options)?;
    let mut decoder = Decoder::new(options)?;
    let mut out = decoder.push(input)?;
    out.extend(decoder.finish()?);
    Ok(out)
}

pub struct Encoder {
    options: Options,
    pending: Vec<u8>,
    closed: bool,
}

impl Encoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            pending: Vec::new(),
            closed: false,
        })
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        let mut symbols = Vec::new();
        let mut all = Vec::with_capacity(self.pending.len() + input.len());
        all.extend_from_slice(&self.pending);
        all.extend_from_slice(input);
        let complete = all.len() / 3 * 3;
        for chunk in all[..complete].chunks_exact(3) {
            symbols.extend(encode_group(chunk, self.options.alphabet, self.options.padding));
        }
        self.pending.clear();
        self.pending.extend_from_slice(&all[complete..]);
        Ok(wrap_symbols(symbols, self.options.whitespace))
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        self.closed = true;
        let symbols = if self.pending.is_empty() {
            Vec::new()
        } else {
            encode_group(&self.pending, self.options.alphabet, self.options.padding)
        };
        self.pending.clear();
        Ok(wrap_symbols(symbols, self.options.whitespace))
    }
}

#[derive(Clone, Copy)]
enum FrameState {
    Normal,
    NeedLf { cr_offset: usize },
    NeedSymbol { separator_offset: usize },
}

pub struct Decoder {
    options: Options,
    group: Vec<(u8, usize)>,
    offset: usize,
    symbols_in_row: usize,
    frame: FrameState,
    terminal_padding: bool,
    pending_second_pad: Option<usize>,
    closed: bool,
    sticky: Option<Error>,
}

impl Decoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            group: Vec::with_capacity(4),
            offset: 0,
            symbols_in_row: 0,
            frame: FrameState::Normal,
            terminal_padding: false,
            pending_second_pad: None,
            closed: false,
            sticky: None,
        })
    }

    fn sticky_or_closed(&self) -> Option<Error> {
        self.sticky
            .clone()
            .or_else(|| if self.closed { Some(Error::AlreadyFinished) } else { None })
    }

    fn fail(&mut self, error: Error) -> Error {
        self.sticky = Some(error.clone());
        error
    }

    fn skipped(&self, byte: u8) -> bool {
        matches!(
            (self.options.whitespace, byte),
            (Whitespace::IgnoreAscii, 9 | 10 | 11 | 12 | 13 | 32)
        )
    }

    fn framing_byte(&mut self, byte: u8, at: usize) -> Result<bool, Error> {
        match self.options.whitespace {
            Whitespace::Wrapped { width, ending } => {
                match self.frame {
                    FrameState::NeedLf { cr_offset } => {
                        if byte != b'\n' {
                            return Err(Error::BadWrap { offset: at });
                        }
                        self.frame = FrameState::NeedSymbol {
                            separator_offset: cr_offset,
                        };
                        Ok(false)
                    }
                    FrameState::NeedSymbol { .. } => {
                        self.frame = FrameState::Normal;
                        self.symbols_in_row = 0;
                        if byte == b'\r' || byte == b'\n' {
                            return Err(Error::BadWrap { offset: at });
                        }
                        Ok(true)
                    }
                    FrameState::Normal => {
                        if self.symbols_in_row == width {
                            match ending {
                                LineEnding::Lf if byte == b'\n' => {
                                    self.frame = FrameState::NeedSymbol {
                                        separator_offset: at,
                                    };
                                    Ok(false)
                                }
                                LineEnding::CrLf if byte == b'\r' => {
                                    self.frame = FrameState::NeedLf { cr_offset: at };
                                    Ok(false)
                                }
                                _ => Err(Error::BadWrap { offset: at }),
                            }
                        } else if byte == b'\r' || byte == b'\n' {
                            Err(Error::BadWrap { offset: at })
                        } else {
                            Ok(true)
                        }
                    }
                }
            }
            Whitespace::IgnoreAscii | Whitespace::None => Ok(true),
        }
    }

    fn complete_group(&mut self, out: &mut Vec<u8>) {
        let g = &self.group;
        let n = g.len();
        let a = digit(g[0].0, self.options.alphabet).unwrap();
        let b = digit(g[1].0, self.options.alphabet).unwrap();
        out.push((a << 2) | (b >> 4));
        if n == 4 && g[2].0 != b'=' {
            let c = digit(g[2].0, self.options.alphabet).unwrap();
            out.push((b << 4) | (c >> 2));
            if g[3].0 != b'=' {
                let d = digit(g[3].0, self.options.alphabet).unwrap();
                out.push((c << 6) | d);
            }
        }
        if n == 4 && (g[2].0 == b'=' || g[3].0 == b'=') {
            self.terminal_padding = true;
        }
        self.group.clear();
        self.pending_second_pad = None;
    }

    pub fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if let Some(error) = self.sticky_or_closed() {
            return Err(error);
        }
        let mut out = Vec::new();
        for &byte in input {
            let at = self.offset;
            self.offset += 1;

            if self.skipped(byte) {
                continue;
            }

            let symbol = match self.framing_byte(byte, at) {
                Ok(symbol) => symbol,
                Err(error) => return Err(self.fail(error)),
            };
            if !symbol {
                continue;
            }

            if matches!(self.options.whitespace, Whitespace::Wrapped { .. }) {
                self.symbols_in_row += 1;
            }

            if self.terminal_padding {
                return Err(self.fail(Error::BadPadding { offset: at }));
            }

            if self.pending_second_pad.is_some() && byte != b'=' {
                return Err(self.fail(Error::BadPadding { offset: at }));
            }

            let pos = self.group.len();
            if byte == b'=' {
                if pos <= 1 || (pos == 2 && self.options.padding == Padding::Forbidden) {
                    return Err(self.fail(Error::BadPadding { offset: at }));
                }
                if pos == 2 {
                    self.pending_second_pad = Some(at);
                    self.group.push((byte, at));
                    continue;
                }
                if pos == 3 {
                    if self.options.padding == Padding::Forbidden {
                        return Err(self.fail(Error::BadPadding { offset: at }));
                    }
                    self.group.push((byte, at));
                    self.complete_group(&mut out);
                    continue;
                }
                return Err(self.fail(Error::BadPadding { offset: at }));
            }

            let Some(_) = digit(byte, self.options.alphabet) else {
                return Err(self.fail(Error::InvalidByte { offset: at, byte }));
            };
            if pos == 3 && self.group[2].0 == b'=' {
                return Err(self.fail(Error::BadPadding { offset: at }));
            }

            self.group.push((byte, at));
            if self.group.len() == 4 {
                self.complete_group(&mut out);
            }
        }
        Ok(out)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, Error> {
        if let Some(error) = self.sticky_or_closed() {
            return Err(error);
        }
        if let FrameState::NeedLf { cr_offset } = self.frame {
            return Err(self.fail(Error::BadWrap { offset: cr_offset }));
        }
        if let FrameState::NeedSymbol { separator_offset } = self.frame {
            return Err(self.fail(Error::BadWrap {
                offset: separator_offset,
            }));
        }

        if let Some(at) = self.pending_second_pad {
            return Err(self.fail(Error::BadPadding { offset: at }));
        }

        let n = self.group.len();
        if n == 0 {
            self.closed = true;
            return Ok(Vec::new());
        }

        match self.options.padding {
            Padding::Required => {
                return Err(self.fail(Error::Truncated {
                    offset: self.group[0].1,
                }));
            }
            Padding::Forbidden | Padding::Optional => match n {
                1 => {
                    return Err(self.fail(Error::Truncated {
                        offset: self.group[0].1,
                    }));
                }
                2 | 3 => {
                    let mut out = Vec::with_capacity(if n == 2 { 1 } else { 2 });
                    self.complete_group(&mut out);
                    self.closed = true;
                    Ok(out)
                }
                _ => unreachable!(),
            },
        }
    }
}

pub fn main() {}