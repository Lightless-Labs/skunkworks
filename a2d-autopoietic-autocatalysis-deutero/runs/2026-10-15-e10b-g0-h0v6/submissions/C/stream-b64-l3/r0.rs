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

fn validate(options: Options) -> Result<(), Error> {
    if let Whitespace::Wrapped { width, .. } = options.whitespace {
        if !(4..=120).contains(&width) || width % 4 != 0 {
            return Err(Error::InvalidWidth { width });
        }
    }
    Ok(())
}

fn encode_digit(value: u8, alphabet: Alphabet) -> u8 {
    match value {
        0..=25 => b'A' + value,
        26..=51 => b'a' + (value - 26),
        52..=61 => b'0' + (value - 52),
        62 => match alphabet {
            Alphabet::Standard => b'+',
            Alphabet::UrlSafe => b'-',
        },
        _ => match alphabet {
            Alphabet::Standard => b'/',
            Alphabet::UrlSafe => b'_',
        },
    }
}

fn decode_digit(byte: u8, alphabet: Alphabet) -> Option<u8> {
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
    let mut out = Vec::with_capacity((input.len() + 2) / 3 * 4);
    let mut chunks = input.chunks_exact(3);
    for chunk in &mut chunks {
        out.push(encode_digit(chunk[0] >> 2, alphabet));
        out.push(encode_digit(((chunk[0] & 3) << 4) | (chunk[1] >> 4), alphabet));
        out.push(encode_digit(((chunk[1] & 15) << 2) | (chunk[2] >> 6), alphabet));
        out.push(encode_digit(chunk[2] & 63, alphabet));
    }
    let rem = chunks.remainder();
    if !rem.is_empty() {
        out.push(encode_digit(rem[0] >> 2, alphabet));
        if rem.len() == 1 {
            out.push(encode_digit((rem[0] & 3) << 4, alphabet));
            if padding != Padding::Forbidden {
                out.extend_from_slice(b"==");
            }
        } else {
            out.push(encode_digit(((rem[0] & 3) << 4) | (rem[1] >> 4), alphabet));
            out.push(encode_digit((rem[1] & 15) << 2, alphabet));
            if padding != Padding::Forbidden {
                out.push(b'=');
            }
        }
    }
    out
}

fn wrap_symbols(raw: &[u8], whitespace: Whitespace) -> Vec<u8> {
    let Whitespace::Wrapped { width, ending } = whitespace else {
        return raw.to_vec();
    };
    let sep: &[u8] = match ending {
        LineEnding::Lf => b"\n",
        LineEnding::CrLf => b"\r\n",
    };
    let mut out = Vec::with_capacity(raw.len() + raw.len() / width * sep.len());
    for (i, chunk) in raw.chunks(width).enumerate() {
        if i != 0 {
            out.extend_from_slice(sep);
        }
        out.extend_from_slice(chunk);
    }
    out
}

pub fn encode(input: &[u8], options: Options) -> Result<String, Error> {
    validate(options)?;
    let raw = encode_raw(input, options.alphabet, options.padding);
    let wrapped = wrap_symbols(&raw, options.whitespace);
    Ok(String::from_utf8(wrapped).expect("base64 output is ASCII"))
}

pub struct Encoder {
    options: Options,
    pending: Vec<u8>,
    column: usize,
    closed: bool,
}

impl Encoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            pending: Vec::new(),
            column: 0,
            closed: false,
        })
    }

    fn emit_symbols(&mut self, symbols: &[u8], out: &mut Vec<u8>) {
        let Whitespace::Wrapped { width, ending } = self.options.whitespace else {
            out.extend_from_slice(symbols);
            return;
        };
        let sep: &[u8] = match ending {
            LineEnding::Lf => b"\n",
            LineEnding::CrLf => b"\r\n",
        };
        for &symbol in symbols {
            if self.column == width {
                out.extend_from_slice(sep);
                self.column = 0;
            }
            out.push(symbol);
            self.column += 1;
        }
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        self.pending.extend_from_slice(input);
        let complete_len = self.pending.len() / 3 * 3;
        let completed = self.pending[..complete_len].to_vec();
        self.pending.drain(..complete_len);

        let symbols = encode_raw(&completed, self.options.alphabet, Padding::Forbidden);
        let mut out = Vec::new();
        self.emit_symbols(&symbols, &mut out);
        Ok(String::from_utf8(out).expect("base64 output is ASCII"))
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        self.closed = true;
        let symbols = encode_raw(&self.pending, self.options.alphabet, self.options.padding);
        self.pending.clear();
        let mut out = Vec::new();
        self.emit_symbols(&symbols, &mut out);
        Ok(String::from_utf8(out).expect("base64 output is ASCII"))
    }
}

#[derive(Clone, Copy)]
enum WrapState {
    Normal,
    NeedSeparator { start: usize },
    NeedLf { start: usize, cr: usize },
    NeedSymbol { start: usize },
}

pub struct Decoder {
    options: Options,
    group: Vec<(u8, usize)>,
    offset: usize,
    terminal: bool,
    wrap_state: WrapState,
    column: usize,
    closed: bool,
    sticky: Option<Error>,
}

impl Decoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            group: Vec::new(),
            offset: 0,
            terminal: false,
            wrap_state: WrapState::Normal,
            column: 0,
            closed: false,
            sticky: None,
        })
    }

    fn scan_symbol(&mut self, byte: u8, offset: usize, out: &mut Vec<u8>) -> Result<(), Error> {
        if self.terminal {
            return Err(Error::BadPadding { offset });
        }
        if self.group.len() == 2 && self.group[1].0 == b'=' && byte != b'=' {
            return Err(Error::BadPadding { offset });
        }
        let pos = self.group.len();
        if byte == b'=' {
            if pos < 2 {
                return Err(Error::BadPadding { offset });
            }
            if pos == 2 {
                self.group.push((byte, offset));
                return Ok(());
            }
            if self.group[2].0 == b'=' {
                let a = decode_digit(self.group[0].0, self.options.alphabet).unwrap();
                let b = decode_digit(self.group[1].0, self.options.alphabet).unwrap();
                out.push((a << 2) | (b >> 4));
            } else {
                let a = decode_digit(self.group[0].0, self.options.alphabet).unwrap();
                let b = decode_digit(self.group[1].0, self.options.alphabet).unwrap();
                let c = decode_digit(self.group[2].0, self.options.alphabet).unwrap();
                out.push((a << 2) | (b >> 4));
                out.push((b << 4) | (c >> 2));
            }
            self.group.clear();
            self.terminal = true;
            return Ok(());
        }

        let Some(value) = decode_digit(byte, self.options.alphabet) else {
            return Err(Error::InvalidByte { offset, byte });
        };
        self.group.push((byte, offset));
        if self.group.len() == 4 {
            let a = decode_digit(self.group[0].0, self.options.alphabet).unwrap();
            let b = decode_digit(self.group[1].0, self.options.alphabet).unwrap();
            let c = decode_digit(self.group[2].0, self.options.alphabet).unwrap();
            let d = value;
            out.push((a << 2) | (b >> 4));
            out.push((b << 4) | (c >> 2));
            out.push((c << 6) | d);
            self.group.clear();
        }
        Ok(())
    }

    fn process_byte(&mut self, byte: u8, offset: usize, out: &mut Vec<u8>) -> Result<(), Error> {
        match self.options.whitespace {
            Whitespace::None => self.scan_symbol(byte, offset, out),
            Whitespace::IgnoreAscii => {
                if matches!(byte, 9 | 10 | 11 | 12 | 13 | 32) {
                    Ok(())
                } else {
                    self.scan_symbol(byte, offset, out)
                }
            }
            Whitespace::Wrapped { width, ending } => {
                let sep_first = match ending {
                    LineEnding::Lf => b'\n',
                    LineEnding::CrLf => b'\r',
                };
                match self.wrap_state {
                    WrapState::NeedSeparator { start } => {
                        if byte != sep_first {
                            return Err(Error::BadWrap { offset });
                        }
                        match ending {
                            LineEnding::Lf => {
                                self.wrap_state = WrapState::NeedSymbol { start };
                            }
                            LineEnding::CrLf => {
                                self.wrap_state = WrapState::NeedLf {
                                    start,
                                    cr: offset,
                                };
                            }
                        }
                        Ok(())
                    }
                    WrapState::NeedLf { start, cr } => {
                        if byte != b'\n' {
                            return Err(Error::BadWrap { offset });
                        }
                        self.wrap_state = WrapState::NeedSymbol { start };
                        let _ = cr;
                        Ok(())
                    }
                    WrapState::NeedSymbol { start } => {
                        if matches!(byte, b'\r' | b'\n' | 9 | 10 | 11 | 12 | 13 | 32) {
                            return Err(Error::BadWrap { offset });
                        }
                        self.wrap_state = WrapState::Normal;
                        self.column = 0;
                        let _ = start;
                        self.scan_symbol(byte, offset, out)?;
                        self.column += 1;
                        if self.column == width {
                            self.wrap_state = WrapState::NeedSeparator { start: offset + 1 };
                        }
                        Ok(())
                    }
                    WrapState::Normal => {
                        if matches!(byte, b'\r' | b'\n' | 9 | 10 | 11 | 12 | 13 | 32) {
                            return Err(Error::BadWrap { offset });
                        }
                        self.scan_symbol(byte, offset, out)?;
                        self.column += 1;
                        if self.column == width {
                            self.wrap_state = WrapState::NeedSeparator { start: offset + 1 };
                        }
                        Ok(())
                    }
                }
            }
        }
    }

    pub fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if let Some(err) = &self.sticky {
            return Err(err.clone());
        }
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        let mut out = Vec::new();
        for &byte in input {
            let offset = self.offset;
            self.offset += 1;
            if let Err(err) = self.process_byte(byte, offset, &mut out) {
                self.sticky = Some(err.clone());
                return Err(err);
            }
        }
        Ok(out)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, Error> {
        if let Some(err) = &self.sticky {
            return Err(err.clone());
        }
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        let eof_err = match self.wrap_state {
            WrapState::NeedSeparator { .. } => None,
            WrapState::NeedLf { cr, .. } => Some(Error::BadWrap { offset: cr }),
            WrapState::NeedSymbol { start } => Some(Error::BadWrap { offset: start }),
            WrapState::Normal => None,
        };
        if let Some(err) = eof_err {
            self.sticky = Some(err.clone());
            return Err(err);
        }
        if self.group.len() == 3 && self.group[2].0 == b'=' {
            let err = Error::BadPadding {
                offset: self.group[2].1,
            };
            self.sticky = Some(err.clone());
            return Err(err);
        }
        let mut out = Vec::new();
        match self.group.len() {
            0 => {}
            1 => {
                let err = Error::Truncated {
                    offset: self.group[0].1,
                };
                self.sticky = Some(err.clone());
                return Err(err);
            }
            2 | 3 => {
                if self.options.padding == Padding::Required {
                    let err = Error::Truncated {
                        offset: self.group[0].1,
                    };
                    self.sticky = Some(err.clone());
                    return Err(err);
                }
                let a = decode_digit(self.group[0].0, self.options.alphabet).unwrap();
                let b = decode_digit(self.group[1].0, self.options.alphabet).unwrap();
                out.push((a << 2) | (b >> 4));
                if self.group.len() == 3 {
                    let c = decode_digit(self.group[2].0, self.options.alphabet).unwrap();
                    out.push((b << 4) | (c >> 2));
                }
            }
            _ => {}
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