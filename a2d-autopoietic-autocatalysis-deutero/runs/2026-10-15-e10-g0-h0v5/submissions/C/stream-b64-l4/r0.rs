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

fn encode_digit(digit: u8, alphabet: Alphabet) -> u8 {
    match digit {
        0..=25 => b'A' + digit,
        26..=51 => b'a' + (digit - 26),
        52..=61 => b'0' + (digit - 52),
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

fn encode_symbols(bytes: &[u8], alphabet: Alphabet, padding: bool) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 3 <= bytes.len() {
        let a = bytes[i];
        let b = bytes[i + 1];
        let c = bytes[i + 2];
        out.push(encode_digit(a >> 2, alphabet));
        out.push(encode_digit(((a & 3) << 4) | (b >> 4), alphabet));
        out.push(encode_digit(((b & 15) << 2) | (c >> 6), alphabet));
        out.push(encode_digit(c & 63, alphabet));
        i += 3;
    }
    match bytes.len() - i {
        1 => {
            let a = bytes[i];
            out.push(encode_digit(a >> 2, alphabet));
            out.push(encode_digit((a & 3) << 4, alphabet));
            if padding {
                out.extend_from_slice(b"==");
            }
        }
        2 => {
            let a = bytes[i];
            let b = bytes[i + 1];
            out.push(encode_digit(a >> 2, alphabet));
            out.push(encode_digit(((a & 3) << 4) | (b >> 4), alphabet));
            out.push(encode_digit((b & 15) << 2, alphabet));
            if padding {
                out.push(b'=');
            }
        }
        _ => {}
    }
    out
}

pub struct Encoder {
    options: Options,
    buffered: Vec<u8>,
    closed: bool,
    row_symbols: usize,
}

impl Encoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            buffered: Vec::new(),
            closed: false,
            row_symbols: 0,
        })
    }

    fn layout(&mut self, symbols: &[u8]) -> String {
        let mut out = Vec::new();
        for &symbol in symbols {
            if let Whitespace::Wrapped { width, ending } = self.options.whitespace {
                if self.row_symbols == width {
                    match ending {
                        LineEnding::Lf => out.push(b'\n'),
                        LineEnding::CrLf => out.extend_from_slice(b"\r\n"),
                    }
                    self.row_symbols = 0;
                }
                self.row_symbols += 1;
            }
            out.push(symbol);
        }
        String::from_utf8(out).unwrap()
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        self.buffered.extend_from_slice(input);
        let complete = self.buffered.len() / 3 * 3;
        let symbols = encode_symbols(
            &self.buffered[..complete],
            self.options.alphabet,
            true,
        );
        self.buffered.drain(..complete);
        Ok(self.layout(&symbols))
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        let symbols = encode_symbols(
            &self.buffered,
            self.options.alphabet,
            self.options.padding != Padding::Forbidden,
        );
        self.buffered.clear();
        self.closed = true;
        Ok(self.layout(&symbols))
    }
}

pub fn encode(input: &[u8], options: Options) -> Result<String, Error> {
    let mut encoder = Encoder::new(options)?;
    let mut out = encoder.push(input)?;
    out.push_str(&encoder.finish()?);
    Ok(out)
}

#[derive(Clone, Copy)]
struct Symbol {
    value: Option<u8>,
    offset: usize,
}

#[derive(Clone, Copy)]
enum WrapState {
    Normal,
    NeedLf { cr: usize },
    NeedSymbol { start: usize },
}

pub struct Decoder {
    options: Options,
    group: Vec<Symbol>,
    terminal: bool,
    offset: usize,
    symbol_count: usize,
    row_symbols: usize,
    wrap_state: WrapState,
    output: Vec<u8>,
    closed: bool,
    sticky: Option<Error>,
}

fn ascii_space(byte: u8) -> bool {
    matches!(byte, 9 | 10 | 11 | 12 | 13 | 32)
}

fn alphabet_value(byte: u8, alphabet: Alphabet) -> Option<u8> {
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

fn foreign_alphabet(byte: u8, alphabet: Alphabet) -> bool {
    match alphabet {
        Alphabet::Standard => matches!(byte, b'-' | b'_'),
        Alphabet::UrlSafe => matches!(byte, b'+' | b'/'),
    }
}

impl Decoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            group: Vec::with_capacity(4),
            terminal: false,
            offset: 0,
            symbol_count: 0,
            row_symbols: 0,
            wrap_state: WrapState::Normal,
            output: Vec::new(),
            closed: false,
            sticky: None,
        })
    }

    fn decode_group(&mut self) -> Result<(), Error> {
        let g = &self.group;
        let a = g[0].value.unwrap();
        let b = g[1].value.unwrap();
        if g[2].value.is_none() {
            if self.options.strict && b & 15 != 0 {
                return Err(Error::NonCanonical { offset: g[1].offset });
            }
            self.output.push((a << 2) | (b >> 4));
            self.terminal = true;
        } else {
            let c = g[2].value.unwrap();
            if g[3].value.is_none() {
                if self.options.strict && c & 3 != 0 {
                    return Err(Error::NonCanonical { offset: g[2].offset });
                }
                self.output.push((a << 2) | (b >> 4));
                self.output.push((b << 4) | (c >> 2));
                self.terminal = true;
            } else {
                let d = g[3].value.unwrap();
                self.output.push((a << 2) | (b >> 4));
                self.output.push((b << 4) | (c >> 2));
                self.output.push((c << 6) | d);
            }
        }
        self.group.clear();
        Ok(())
    }

    fn scan_symbol(&mut self, byte: u8, offset: usize) -> Result<(), Error> {
        if let Some(limit) = self.options.max_symbols {
            if self.symbol_count >= limit {
                return Err(Error::Overlong { offset, limit });
            }
        }
        self.symbol_count += 1;

        if self.terminal {
            return Err(Error::BadPadding { offset });
        }
        if self.group.len() == 3 && self.group[2].value.is_none() && byte != b'=' {
            return Err(Error::BadPadding { offset });
        }
        let pos = self.group.len();
        if byte == b'=' {
            if self.options.padding == Padding::Forbidden || pos < 2 {
                return Err(Error::BadPadding { offset });
            }
            if pos == 2 {
                self.group.push(Symbol { value: None, offset });
                return Ok(());
            }
            self.group.push(Symbol { value: None, offset });
            return self.decode_group();
        }

        let value = match alphabet_value(byte, self.options.alphabet) {
            Some(v) => Some(v),
            None if self.options.strict && foreign_alphabet(byte, self.options.alphabet) => {
                return Err(Error::MixedAlphabet { offset });
            }
            None => return Err(Error::InvalidByte { offset, byte }),
        };
        self.group.push(Symbol { value, offset });
        if self.group.len() == 4 {
            self.decode_group()?;
        }
        Ok(())
    }

    pub fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky {
            return Err(error.clone());
        }
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        let start_output = self.output.len();
        for &byte in input {
            let offset = self.offset;
            self.offset += 1;

            if let Whitespace::IgnoreAscii = self.options.whitespace {
                if ascii_space(byte) {
                    continue;
                }
            }

            if let Whitespace::Wrapped { width, ending } = self.options.whitespace {
                match self.wrap_state {
                    WrapState::NeedLf { cr } => {
                        if byte != b'\n' {
                            return self.fail(Error::BadWrap { offset });
                        }
                        self.wrap_state = WrapState::NeedSymbol { start: cr };
                        continue;
                    }
                    WrapState::NeedSymbol { .. } => {
                        if ascii_space(byte) {
                            return self.fail(Error::BadWrap { offset });
                        }
                        self.wrap_state = WrapState::Normal;
                        self.row_symbols = 0;
                    }
                    WrapState::Normal => {
                        if self.row_symbols == width {
                            match ending {
                                LineEnding::Lf if byte == b'\n' => {
                                    self.wrap_state = WrapState::NeedSymbol { start: offset };
                                    continue;
                                }
                                LineEnding::CrLf if byte == b'\r' => {
                                    self.wrap_state = WrapState::NeedLf { cr: offset };
                                    continue;
                                }
                                _ => return self.fail(Error::BadWrap { offset }),
                            }
                        }
                        if ascii_space(byte) {
                            return self.fail(Error::BadWrap { offset });
                        }
                        self.row_symbols += 1;
                    }
                }
            }

            if let Err(error) = self.scan_symbol(byte, offset) {
                return self.fail(error);
            }
        }
        let result = self.output[start_output..].to_vec();
        self.output.clear();
        Ok(result)
    }

    fn fail<T>(&mut self, error: Error) -> Result<T, Error> {
        self.sticky = Some(error.clone());
        Err(error)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky {
            return Err(error.clone());
        }
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        let start_output = self.output.len();

        match self.wrap_state {
            WrapState::NeedLf { cr } => return self.fail(Error::BadWrap { offset: cr }),
            WrapState::NeedSymbol { start } => return self.fail(Error::BadWrap { offset: start }),
            WrapState::Normal => {}
        }

        if self.group.len() == 3 && self.group[2].value.is_none() {
            return self.fail(Error::BadPadding {
                offset: self.group[2].offset,
            });
        }

        if !self.group.is_empty() {
            if self.options.padding == Padding::Required {
                return self.fail(Error::Truncated {
                    offset: self.group[0].offset,
                });
            }
            if self.group.len() == 1 {
                return self.fail(Error::Truncated {
                    offset: self.group[0].offset,
                });
            }
            if self.group.len() == 2 {
                if self.options.strict && self.group[1].value.unwrap() & 15 != 0 {
                    return self.fail(Error::NonCanonical {
                        offset: self.group[1].offset,
                    });
                }
                self.output.push(
                    (self.group[0].value.unwrap() << 2)
                        | (self.group[1].value.unwrap() >> 4),
                );
            } else if self.group.len() == 3 {
                if self.options.strict && self.group[2].value.unwrap() & 3 != 0 {
                    return self.fail(Error::NonCanonical {
                        offset: self.group[2].offset,
                    });
                }
                let a = self.group[0].value.unwrap();
                let b = self.group[1].value.unwrap();
                let c = self.group[2].value.unwrap();
                self.output.push((a << 2) | (b >> 4));
                self.output.push((b << 4) | (c >> 2));
            }
            self.group.clear();
        }
        self.closed = true;
        let result = self.output[start_output..].to_vec();
        self.output.clear();
        Ok(result)
    }
}

pub fn decode(input: &[u8], options: Options) -> Result<Vec<u8>, Error> {
    let mut decoder = Decoder::new(options)?;
    let mut out = decoder.push(input)?;
    out.extend(decoder.finish()?);
    Ok(out)
}

fn main() {}