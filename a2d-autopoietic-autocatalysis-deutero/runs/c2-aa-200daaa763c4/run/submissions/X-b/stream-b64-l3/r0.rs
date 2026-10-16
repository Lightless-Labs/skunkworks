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

fn alphabet_bytes(alphabet: Alphabet) -> &'static [u8; 64] {
    match alphabet {
        Alphabet::Standard => b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/",
        Alphabet::UrlSafe => b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_",
    }
}

fn encode_symbols(input: &[u8], alphabet: Alphabet, padding: Padding) -> Vec<u8> {
    let table = alphabet_bytes(alphabet);
    let mut out = Vec::with_capacity(input.len().div_ceil(3) * 4);
    let mut chunks = input.chunks_exact(3);
    for chunk in &mut chunks {
        let n = ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8) | chunk[2] as u32;
        out.push(table[((n >> 18) & 63) as usize]);
        out.push(table[((n >> 12) & 63) as usize]);
        out.push(table[((n >> 6) & 63) as usize]);
        out.push(table[(n & 63) as usize]);
    }
    let tail = chunks.remainder();
    if !tail.is_empty() {
        let a = tail[0];
        let b = if tail.len() == 2 { tail[1] } else { 0 };
        out.push(table[(a >> 2) as usize]);
        out.push(table[(((a & 3) << 4) | (b >> 4)) as usize]);
        if tail.len() == 2 {
            out.push(table[((b & 15) << 2) as usize]);
            if padding != Padding::Forbidden {
                out.push(b'=');
            }
        } else if padding != Padding::Forbidden {
            out.extend_from_slice(b"==");
        }
    }
    out
}

fn wrap_symbols(symbols: &[u8], whitespace: Whitespace) -> String {
    let mut out = String::new();
    match whitespace {
        Whitespace::Wrapped { width, ending } => {
            for (i, chunk) in symbols.chunks(width).enumerate() {
                if i != 0 {
                    match ending {
                        LineEnding::Lf => out.push('\n'),
                        LineEnding::CrLf => out.push_str("\r\n"),
                    }
                }
                out.push_str(std::str::from_utf8(chunk).unwrap());
            }
        }
        _ => out.push_str(std::str::from_utf8(symbols).unwrap()),
    }
    out
}

pub fn encode(input: &[u8], options: Options) -> Result<String, Error> {
    validate(options)?;
    Ok(wrap_symbols(
        &encode_symbols(input, options.alphabet, options.padding),
        options.whitespace,
    ))
}

pub struct Encoder {
    options: Options,
    pending: Vec<u8>,
    symbols_in_row: usize,
    finished: bool,
}

impl Encoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            pending: Vec::new(),
            symbols_in_row: 0,
            finished: false,
        })
    }

    fn emit(&mut self, symbols: &[u8]) -> String {
        let mut out = String::new();
        let mut pos = 0;
        while pos < symbols.len() {
            if let Whitespace::Wrapped { width, ending } = self.options.whitespace {
                if self.symbols_in_row == width {
                    match ending {
                        LineEnding::Lf => out.push('\n'),
                        LineEnding::CrLf => out.push_str("\r\n"),
                    }
                    self.symbols_in_row = 0;
                }
                let take = (width - self.symbols_in_row).min(symbols.len() - pos);
                out.push_str(std::str::from_utf8(&symbols[pos..pos + take]).unwrap());
                self.symbols_in_row += take;
                pos += take;
            } else {
                out.push_str(std::str::from_utf8(&symbols[pos..]).unwrap());
                break;
            }
        }
        out
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        self.pending.extend_from_slice(input);
        let complete_len = self.pending.len() / 3 * 3;
        let bytes: Vec<u8> = self.pending.drain(..complete_len).collect();
        let symbols = encode_symbols(&bytes, self.options.alphabet, Padding::Forbidden);
        Ok(self.emit(&symbols))
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        self.finished = true;
        let tail = std::mem::take(&mut self.pending);
        let symbols = encode_symbols(&tail, self.options.alphabet, self.options.padding);
        Ok(self.emit(&symbols))
    }
}

fn digit(byte: u8, alphabet: Alphabet) -> Option<u8> {
    alphabet_bytes(alphabet).iter().position(|&b| b == byte).map(|v| v as u8)
}

fn is_ignored(byte: u8) -> bool {
    matches!(byte, 9 | 10 | 11 | 12 | 13 | 32)
}

#[derive(Clone, Copy)]
struct Symbol {
    byte: u8,
    offset: usize,
}

pub struct Decoder {
    options: Options,
    offset: usize,
    group: Vec<Symbol>,
    pending_second_pad: Option<usize>,
    terminal_padding: bool,
    wrapped_symbols: usize,
    wrap_state: WrapState,
    finished: bool,
    sticky_error: Option<Error>,
}

#[derive(Clone, Copy)]
enum WrapState {
    Normal,
    NeedLf { cr_offset: usize },
    NeedSymbol { separator_offset: usize },
}

impl Decoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            offset: 0,
            group: Vec::with_capacity(4),
            pending_second_pad: None,
            terminal_padding: false,
            wrapped_symbols: 0,
            wrap_state: WrapState::Normal,
            finished: false,
            sticky_error: None,
        })
    }

    fn scan_symbol(&mut self, symbol: Symbol, out: &mut Vec<u8>) -> Result<(), Error> {
        let b = symbol.byte;
        let offset = symbol.offset;

        if self.terminal_padding {
            return Err(Error::BadPadding { offset });
        }
        if let Some(first_pad) = self.pending_second_pad {
            if b != b'=' {
                return Err(Error::BadPadding { offset });
            }
            self.pending_second_pad = None;
            self.group.push(symbol);
            self.terminal_padding = true;
            let a = digit(self.group[0].byte, self.options.alphabet).unwrap();
            let c = digit(self.group[1].byte, self.options.alphabet).unwrap();
            out.push((a << 2) | (c >> 4));
            self.group.clear();
            let _ = first_pad;
            return Ok(());
        }

        if b == b'=' {
            if self.options.padding == Padding::Forbidden {
                return Err(Error::BadPadding { offset });
            }
            match self.group.len() {
                0 | 1 => return Err(Error::BadPadding { offset }),
                2 => {
                    self.pending_second_pad = Some(offset);
                    self.group.push(symbol);
                    return Ok(());
                }
                3 => {
                    let a = digit(self.group[0].byte, self.options.alphabet).unwrap();
                    let c = digit(self.group[1].byte, self.options.alphabet).unwrap();
                    let d = digit(self.group[2].byte, self.options.alphabet).unwrap();
                    out.push((a << 2) | (c >> 4));
                    out.push((c << 4) | (d >> 2));
                    self.group.clear();
                    self.terminal_padding = true;
                    return Ok(());
                }
                _ => unreachable!(),
            }
        }

        let value = digit(b, self.options.alphabet)
            .ok_or(Error::InvalidByte { offset, byte: b })?;
        self.group.push(symbol);
        if self.group.len() == 4 {
            let a = digit(self.group[0].byte, self.options.alphabet).unwrap();
            let c = digit(self.group[1].byte, self.options.alphabet).unwrap();
            let d = digit(self.group[2].byte, self.options.alphabet).unwrap();
            let e = value;
            out.push((a << 2) | (c >> 4));
            out.push((c << 4) | (d >> 2));
            out.push((d << 6) | e);
            self.group.clear();
        }
        Ok(())
    }

    fn scan_physical(&mut self, byte: u8, offset: usize, out: &mut Vec<u8>) -> Result<(), Error> {
        match self.options.whitespace {
            Whitespace::None => self.scan_symbol(Symbol { byte, offset }, out),
            Whitespace::IgnoreAscii => {
                if is_ignored(byte) {
                    Ok(())
                } else {
                    self.scan_symbol(Symbol { byte, offset }, out)
                }
            }
            Whitespace::Wrapped { width, ending } => {
                match self.wrap_state {
                    WrapState::NeedLf { cr_offset } => {
                        if byte != b'\n' {
                            return Err(Error::BadWrap { offset });
                        }
                        self.wrap_state = WrapState::NeedSymbol {
                            separator_offset: cr_offset,
                        };
                        return Ok(());
                    }
                    WrapState::NeedSymbol { .. } => {
                        if matches!(byte, b'\r' | b'\n' | 9 | 11 | 12 | 32) {
                            return Err(Error::BadWrap { offset });
                        }
                        self.wrap_state = WrapState::Normal;
                        self.wrapped_symbols = 0;
                    }
                    WrapState::Normal => {
                        if self.wrapped_symbols == width {
                            match ending {
                                LineEnding::Lf if byte == b'\n' => {
                                    self.wrap_state = WrapState::NeedSymbol {
                                        separator_offset: offset,
                                    };
                                    return Ok(());
                                }
                                LineEnding::CrLf if byte == b'\r' => {
                                    self.wrap_state = WrapState::NeedLf { cr_offset: offset };
                                    return Ok(());
                                }
                                _ => return Err(Error::BadWrap { offset }),
                            }
                        }
                        if matches!(byte, b'\r' | b'\n' | 9 | 11 | 12 | 32) {
                            return Err(Error::BadWrap { offset });
                        }
                    }
                }
                self.wrapped_symbols += 1;
                self.scan_symbol(Symbol { byte, offset }, out)
            }
        }
    }

    pub fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky_error {
            return Err(error.clone());
        }
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        let mut out = Vec::new();
        for &byte in input {
            let offset = self.offset;
            self.offset += 1;
            if let Err(error) = self.scan_physical(byte, offset, &mut out) {
                self.sticky_error = Some(error.clone());
                return Err(error);
            }
        }
        Ok(out)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky_error {
            return Err(error.clone());
        }
        if self.finished {
            return Err(Error::AlreadyFinished);
        }

        let error = match self.wrap_state {
            WrapState::NeedLf { cr_offset } => Some(Error::BadWrap { offset: cr_offset }),
            WrapState::NeedSymbol { separator_offset } => {
                Some(Error::BadWrap { offset: separator_offset })
            }
            WrapState::Normal => None,
        };
        if let Some(error) = error {
            self.sticky_error = Some(error.clone());
            return Err(error);
        }

        if let Some(offset) = self.pending_second_pad {
            let error = Error::BadPadding { offset };
            self.sticky_error = Some(error.clone());
            return Err(error);
        }

        let mut out = Vec::new();
        if !self.group.is_empty() {
            let len = self.group.len();
            if self.options.padding == Padding::Required {
                let error = Error::Truncated {
                    offset: self.group[0].offset,
                };
                self.sticky_error = Some(error.clone());
                return Err(error);
            }
            if len == 1 {
                let error = Error::Truncated {
                    offset: self.group[0].offset,
                };
                self.sticky_error = Some(error.clone());
                return Err(error);
            }
            let a = digit(self.group[0].byte, self.options.alphabet).unwrap();
            let b = digit(self.group[1].byte, self.options.alphabet).unwrap();
            out.push((a << 2) | (b >> 4));
            if len == 3 {
                let c = digit(self.group[2].byte, self.options.alphabet).unwrap();
                out.push((b << 4) | (c >> 2));
            }
            self.group.clear();
        }

        self.finished = true;
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