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
    pub strict: bool,
    pub max_symbols: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidByte { offset: usize, byte: u8 },
    BadPadding { offset: usize },
    Truncated { offset: usize },
    AlreadyFinished,
    InvalidWidth { width: usize },
    BadWrap { offset: usize },
    NonCanonical { offset: usize },
    MixedAlphabet { offset: usize },
    Overlong { offset: usize, limit: usize },
}

fn validate(options: Options) -> Result<(), Error> {
    if let Whitespace::Wrapped { width, .. } = options.whitespace {
        if !(4..=120).contains(&width) || width % 4 != 0 {
            return Err(Error::InvalidWidth { width });
        }
    }
    Ok(())
}

fn symbol(alphabet: Alphabet, value: u8) -> u8 {
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

fn lookup(options: Options, byte: u8, offset: usize) -> Result<u8, Error> {
    let value = match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' if options.alphabet == Alphabet::Standard => Some(62),
        b'/' if options.alphabet == Alphabet::Standard => Some(63),
        b'-' if options.alphabet == Alphabet::UrlSafe => Some(62),
        b'_' if options.alphabet == Alphabet::UrlSafe => Some(63),
        _ => None,
    };
    if let Some(v) = value {
        return Ok(v);
    }
    if options.strict {
        let foreign = match options.alphabet {
            Alphabet::Standard => matches!(byte, b'-' | b'_'),
            Alphabet::UrlSafe => matches!(byte, b'+' | b'/'),
        };
        if foreign {
            return Err(Error::MixedAlphabet { offset });
        }
    }
    Err(Error::InvalidByte { offset, byte })
}

fn wrapped_separator(ending: LineEnding) -> &'static [u8] {
    match ending {
        LineEnding::Lf => b"\n",
        LineEnding::CrLf => b"\r\n",
    }
}

fn emit_symbols(
    out: &mut String,
    raw: &[u8],
    options: Options,
    column: &mut usize,
    pending_sep: &mut bool,
) {
    for &b in raw {
        if let Whitespace::Wrapped { width, ending } = options.whitespace {
            if *pending_sep {
                out.push_str(std::str::from_utf8(wrapped_separator(ending)).unwrap());
                *pending_sep = false;
                *column = 0;
            }
            if *column == width {
                *pending_sep = true;
                out.push_str(std::str::from_utf8(wrapped_separator(ending)).unwrap());
                *pending_sep = false;
                *column = 0;
            }
            *column += 1;
            if *column == width {
                *pending_sep = true;
            }
        }
        out.push(b as char);
    }
}

fn encode_group(bytes: &[u8], options: Options) -> Vec<u8> {
    let a = bytes[0];
    let b = if bytes.len() > 1 { bytes[1] } else { 0 };
    let c = if bytes.len() > 2 { bytes[2] } else { 0 };
    let mut result = vec![
        symbol(options.alphabet, a >> 2),
        symbol(options.alphabet, ((a & 3) << 4) | (b >> 4)),
    ];
    if bytes.len() > 1 {
        result.push(symbol(options.alphabet, ((b & 15) << 2) | (c >> 6)));
    }
    if bytes.len() > 2 {
        result.push(symbol(options.alphabet, c & 63));
    } else if options.padding != Padding::Forbidden {
        result.push(b'=');
        if bytes.len() == 1 {
            result.push(b'=');
        }
    }
    result
}

pub struct Encoder {
    options: Options,
    buffer: Vec<u8>,
    closed: bool,
    column: usize,
    pending_sep: bool,
}

impl Encoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            buffer: Vec::new(),
            closed: false,
            column: 0,
            pending_sep: false,
        })
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        self.buffer.extend_from_slice(input);
        let mut out = String::new();
        let complete = self.buffer.len() / 3 * 3;
        for chunk in self.buffer[..complete].chunks_exact(3) {
            let raw = encode_group(chunk, self.options);
            emit_symbols(&mut out, &raw, self.options, &mut self.column, &mut self.pending_sep);
        }
        self.buffer.drain(..complete);
        Ok(out)
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        self.closed = true;
        let mut out = String::new();
        if !self.buffer.is_empty() {
            let raw = encode_group(&self.buffer, self.options);
            emit_symbols(&mut out, &raw, self.options, &mut self.column, &mut self.pending_sep);
            self.buffer.clear();
        }
        Ok(out)
    }
}

pub fn encode(input: &[u8], options: Options) -> Result<String, Error> {
    validate(options)?;
    let mut encoder = Encoder::new(options)?;
    let mut result = encoder.push(input)?;
    result.push_str(&encoder.finish()?);
    Ok(result)
}

#[derive(Clone, Copy)]
struct Symbol {
    byte: u8,
    value: Option<u8>,
    offset: usize,
}

pub struct Decoder {
    options: Options,
    offset: usize,
    symbols: usize,
    group: Vec<Symbol>,
    padded: bool,
    closed: bool,
    sticky: Option<Error>,
    wrap_column: usize,
    wrap_state: WrapState,
    last_wrap_start: usize,
}

#[derive(Clone, Copy)]
enum WrapState {
    Row,
    NeedSeparator,
    NeedLf { cr_offset: usize },
    NeedSymbol { separator_offset: usize },
}

impl Decoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            offset: 0,
            symbols: 0,
            group: Vec::with_capacity(4),
            padded: false,
            closed: false,
            sticky: None,
            wrap_column: 0,
            wrap_state: WrapState::Row,
            last_wrap_start: 0,
        })
    }

    fn remember_error(&mut self, error: Error) -> Error {
        self.sticky = Some(error.clone());
        error
    }

    fn decode_group(&mut self, out: &mut Vec<u8>, end: usize) -> Result<(), Error> {
        let g = &self.group;
        let p0 = g[0].value.unwrap();
        let p1 = g[1].value.unwrap();
        let second_offset = g[1].offset;
        if g[2].byte == b'=' {
            if self.options.strict && (p1 & 15) != 0 {
                return Err(Error::NonCanonical { offset: second_offset });
            }
            out.push((p0 << 2) | (p1 >> 4));
            self.padded = true;
        } else if g[3].byte == b'=' {
            let p2 = g[2].value.unwrap();
            if self.options.strict && (p2 & 3) != 0 {
                return Err(Error::NonCanonical { offset: g[2].offset });
            }
            out.push((p0 << 2) | (p1 >> 4));
            out.push((p1 << 4) | (p2 >> 2));
            self.padded = true;
        } else {
            let p2 = g[2].value.unwrap();
            let p3 = g[3].value.unwrap();
            out.push((p0 << 2) | (p1 >> 4));
            out.push((p1 << 4) | (p2 >> 2));
            out.push((p2 << 6) | p3);
        }
        self.group.clear();
        let _ = end;
        Ok(())
    }

    fn scan_symbol(&mut self, byte: u8, offset: usize, out: &mut Vec<u8>) -> Result<(), Error> {
        if let Some(limit) = self.options.max_symbols {
            if self.symbols >= limit {
                return Err(Error::Overlong { offset, limit });
            }
        }
        self.symbols += 1;

        if self.padded {
            return Err(Error::BadPadding { offset });
        }
        if self.group.len() == 3 && self.group[2].byte == b'=' && byte != b'=' {
            return Err(Error::BadPadding { offset });
        }
        let pos = self.group.len();
        if pos <= 1 && byte == b'=' {
            return Err(Error::BadPadding { offset });
        }
        if byte == b'=' {
            if self.options.padding == Padding::Forbidden {
                return Err(Error::BadPadding { offset });
            }
            if pos != 2 && pos != 3 {
                return Err(Error::BadPadding { offset });
            }
            self.group.push(Symbol { byte, value: None, offset });
        } else {
            let value = lookup(self.options, byte, offset)?;
            self.group.push(Symbol { byte, value: Some(value), offset });
        }
        if self.group.len() == 4 {
            self.decode_group(out, offset)?;
        }
        Ok(())
    }

    fn framing_byte(&mut self, byte: u8, offset: usize) -> Result<bool, Error> {
        match self.options.whitespace {
            Whitespace::None => Ok(false),
            Whitespace::IgnoreAscii => Ok(matches!(byte, 9 | 10 | 11 | 12 | 13 | 32)),
            Whitespace::Wrapped { width, ending } => {
                match self.wrap_state {
                    WrapState::NeedSeparator => {
                        let first = match ending {
                            LineEnding::Lf => b'\n',
                            LineEnding::CrLf => b'\r',
                        };
                        if byte != first {
                            return Err(Error::BadWrap { offset });
                        }
                        if ending == LineEnding::Lf {
                            self.wrap_state = WrapState::NeedSymbol { separator_offset: offset };
                        } else {
                            self.wrap_state = WrapState::NeedLf { cr_offset: offset };
                        }
                        return Ok(true);
                    }
                    WrapState::NeedLf { cr_offset } => {
                        if byte != b'\n' {
                            return Err(Error::BadWrap { offset });
                        }
                        self.wrap_state = WrapState::NeedSymbol { separator_offset: cr_offset };
                        return Ok(true);
                    }
                    WrapState::NeedSymbol { .. } => {
                        self.wrap_state = WrapState::Row;
                        self.wrap_column = 0;
                    }
                    WrapState::Row => {}
                }
                if matches!(byte, 9 | 10 | 11 | 12 | 13 | 32) {
                    return Err(Error::BadWrap { offset });
                }
                if self.wrap_column == 0 {
                    self.last_wrap_start = offset;
                }
                self.wrap_column += 1;
                if self.wrap_column == width {
                    self.wrap_state = WrapState::NeedSeparator;
                }
                Ok(false)
            }
        }
    }

    pub fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky {
            return Err(error.clone());
        }
        if self.closed {
            let error = Error::AlreadyFinished;
            self.sticky = Some(error.clone());
            return Err(error);
        }
        let mut out = Vec::new();
        for &byte in input {
            let offset = self.offset;
            self.offset += 1;
            match self.framing_byte(byte, offset) {
                Err(error) => return Err(self.remember_error(error)),
                Ok(true) => continue,
                Ok(false) => {}
            }
            if let Err(error) = self.scan_symbol(byte, offset, &mut out) {
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
            let error = Error::AlreadyFinished;
            self.sticky = Some(error.clone());
            return Err(error);
        }

        let eof = self.offset;
        if let Whitespace::Wrapped { .. } = self.options.whitespace {
            match self.wrap_state {
                WrapState::NeedSeparator => {}
                WrapState::NeedLf { cr_offset } => {
                    let error = Error::BadWrap { offset: cr_offset };
                    return Err(self.remember_error(error));
                }
                WrapState::NeedSymbol { separator_offset } => {
                    let error = Error::BadWrap { offset: separator_offset };
                    return Err(self.remember_error(error));
                }
                WrapState::Row => {}
            }
        }

        if self.group.len() == 3 && self.group[2].byte == b'=' {
            let error = Error::BadPadding { offset: self.group[2].offset };
            return Err(self.remember_error(error));
        }
        let mut out = Vec::new();
        if self.group.is_empty() {
            self.closed = true;
            return Ok(out);
        }
        let len = self.group.len();
        if len == 1 || (self.options.padding == Padding::Required && len != 0) {
            let error = Error::Truncated { offset: self.group[0].offset };
            return Err(self.remember_error(error));
        }
        if len == 2 {
            let p0 = self.group[0].value.unwrap();
            let p1 = self.group[1].value.unwrap();
            if self.options.strict && p1 & 15 != 0 {
                let error = Error::NonCanonical { offset: self.group[1].offset };
                return Err(self.remember_error(error));
            }
            out.push((p0 << 2) | (p1 >> 4));
        } else if len == 3 {
            let p0 = self.group[0].value.unwrap();
            let p1 = self.group[1].value.unwrap();
            let p2 = self.group[2].value.unwrap();
            if self.options.strict && p2 & 3 != 0 {
                let error = Error::NonCanonical { offset: self.group[2].offset };
                return Err(self.remember_error(error));
            }
            out.push((p0 << 2) | (p1 >> 4));
            out.push((p1 << 4) | (p2 >> 2));
        }
        let _ = eof;
        self.closed = true;
        Ok(out)
    }
}

pub fn decode(input: &[u8], options: Options) -> Result<Vec<u8>, Error> {
    validate(options)?;
    let mut decoder = Decoder::new(options)?;
    let mut result = decoder.push(input)?;
    result.extend(decoder.finish()?);
    Ok(result)
}