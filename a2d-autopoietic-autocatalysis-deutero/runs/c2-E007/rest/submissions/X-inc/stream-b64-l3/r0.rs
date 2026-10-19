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

fn validate_options(options: Options) -> Result<(), Error> {
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

fn encoded_symbol(value: u8, alphabet: Alphabet) -> u8 {
    alphabet_bytes(alphabet)[value as usize]
}

fn decode_symbol(byte: u8, alphabet: Alphabet) -> Option<u8> {
    alphabet_bytes(alphabet)
        .iter()
        .position(|&candidate| candidate == byte)
        .map(|value| value as u8)
}

fn encode_group(bytes: &[u8], options: Options, out: &mut String, column: &mut usize) {
    let a = bytes[0];
    let b = if bytes.len() > 1 { bytes[1] } else { 0 };
    let c = if bytes.len() > 2 { bytes[2] } else { 0 };

    let mut symbols = [0u8; 4];
    symbols[0] = encoded_symbol(a >> 2, options.alphabet);
    symbols[1] = encoded_symbol(((a & 0x03) << 4) | (b >> 4), options.alphabet);

    let count = match bytes.len() {
        1 => {
            if options.padding != Padding::Forbidden {
                symbols[2] = b'=';
                symbols[3] = b'=';
                4
            } else {
                2
            }
        }
        2 => {
            symbols[2] = encoded_symbol((b & 0x0f) << 2, options.alphabet);
            if options.padding != Padding::Forbidden {
                symbols[3] = b'=';
                4
            } else {
                3
            }
        }
        _ => {
            symbols[2] = encoded_symbol(((b & 0x0f) << 2) | (c >> 6), options.alphabet);
            symbols[3] = encoded_symbol(c & 0x3f, options.alphabet);
            4
        }
    };

    for &symbol in &symbols[..count] {
        if let Whitespace::Wrapped { width, ending } = options.whitespace {
            if *column == width {
                match ending {
                    LineEnding::Lf => out.push('\n'),
                    LineEnding::CrLf => out.push_str("\r\n"),
                }
                *column = 0;
            }
        }
        out.push(symbol as char);
        *column += 1;
    }
}

pub fn encode(input: &[u8], options: Options) -> Result<String, Error> {
    validate_options(options)?;
    let mut encoder = Encoder::new(options)?;
    let mut result = encoder.push(input)?;
    result.push_str(&encoder.finish()?);
    Ok(result)
}

pub fn decode(input: &[u8], options: Options) -> Result<Vec<u8>, Error> {
    validate_options(options)?;
    let mut decoder = Decoder::new(options)?;
    let mut result = decoder.push(input)?;
    result.extend(decoder.finish()?);
    Ok(result)
}

pub struct Encoder {
    options: Options,
    buffered: Vec<u8>,
    column: usize,
    finished: bool,
}

impl Encoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate_options(options)?;
        Ok(Self {
            options,
            buffered: Vec::with_capacity(2),
            column: 0,
            finished: false,
        })
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }

        let mut out = String::new();
        let mut index = 0;

        if !self.buffered.is_empty() {
            while self.buffered.len() < 3 && index < input.len() {
                self.buffered.push(input[index]);
                index += 1;
            }
            if self.buffered.len() == 3 {
                encode_group(&self.buffered, self.options, &mut out, &mut self.column);
                self.buffered.clear();
            }
        }

        while index + 3 <= input.len() {
            encode_group(&input[index..index + 3], self.options, &mut out, &mut self.column);
            index += 3;
        }

        self.buffered.extend_from_slice(&input[index..]);
        Ok(out)
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        self.finished = true;

        let mut out = String::new();
        if !self.buffered.is_empty() {
            encode_group(&self.buffered, self.options, &mut out, &mut self.column);
            self.buffered.clear();
        }
        Ok(out)
    }
}

#[derive(Clone, Copy)]
enum WrapState {
    Normal,
    NeedSeparator,
    NeedLf { cr_offset: usize },
    NeedSymbol { separator_offset: usize },
}

pub struct Decoder {
    options: Options,
    symbols: Vec<(u8, usize)>,
    offset: usize,
    terminal_padding: bool,
    wrap_state: WrapState,
    row_symbols: usize,
    finished: bool,
    sticky_error: Option<Error>,
}

impl Decoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate_options(options)?;
        Ok(Self {
            options,
            symbols: Vec::with_capacity(4),
            offset: 0,
            terminal_padding: false,
            wrap_state: WrapState::Normal,
            row_symbols: 0,
            finished: false,
            sticky_error: None,
        })
    }

    fn emit_group(&mut self, out: &mut Vec<u8>) {
        let vals: Vec<u8> = self.symbols.iter().map(|&(v, _)| v).collect();
        match vals.as_slice() {
            [a, b, c, d] => {
                if *c == 64 {
                    out.push((a << 2) | (b >> 4));
                    self.terminal_padding = true;
                } else if *d == 64 {
                    out.push((a << 2) | (b >> 4));
                    out.push((b << 4) | (c >> 2));
                    self.terminal_padding = true;
                } else {
                    out.push((a << 2) | (b >> 4));
                    out.push((b << 4) | (c >> 2));
                    out.push((c << 6) | d);
                }
            }
            _ => {}
        }
        self.symbols.clear();
    }

    fn process_symbol(&mut self, byte: u8, offset: usize, out: &mut Vec<u8>) -> Result<(), Error> {
        if self.terminal_padding {
            return Err(Error::BadPadding { offset });
        }

        if self.symbols.len() == 3 && self.symbols[2].0 == 64 && byte != b'=' {
            return Err(Error::BadPadding { offset });
        }

        let position = self.symbols.len();
        if byte == b'=' {
            if position <= 1 {
                return Err(Error::BadPadding { offset });
            }
            if position == 2 {
                if self.options.padding == Padding::Forbidden {
                    self.symbols.push((64, offset));
                    return Ok(());
                }
                self.symbols.push((64, offset));
                return Ok(());
            }
            if position == 3 {
                if self.symbols[2].0 != 64 && self.options.padding == Padding::Forbidden {
                    return Err(Error::BadPadding { offset });
                }
                if self.options.padding == Padding::Forbidden {
                    return Err(Error::BadPadding { offset });
                }
                self.symbols.push((64, offset));
                self.emit_group(out);
                return Ok(());
            }
        } else {
            if self.symbols.len() == 3 && self.symbols[2].0 == 64 {
                return Err(Error::BadPadding { offset });
            }
            if position >= 4 {
                return Err(Error::BadPadding { offset });
            }
            match decode_symbol(byte, self.options.alphabet) {
                Some(value) => self.symbols.push((value, offset)),
                None => return Err(Error::InvalidByte { offset, byte }),
            }
            if self.symbols.len() == 4 {
                self.emit_group(out);
            }
        }
        Ok(())
    }

    fn process_byte(&mut self, byte: u8, offset: usize, out: &mut Vec<u8>) -> Result<(), Error> {
        match self.options.whitespace {
            Whitespace::None => self.process_symbol(byte, offset, out),
            Whitespace::IgnoreAscii => {
                if matches!(byte, 9 | 10 | 11 | 12 | 13 | 32) {
                    Ok(())
                } else {
                    self.process_symbol(byte, offset, out)
                }
            }
            Whitespace::Wrapped { ending, width } => {
                match self.wrap_state {
                    WrapState::NeedSeparator => {
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
                    WrapState::NeedLf { .. } => {
                        if byte == b'\n' {
                            let separator_offset = match self.wrap_state {
                                WrapState::NeedLf { cr_offset } => cr_offset,
                                _ => unreachable!(),
                            };
                            self.wrap_state = WrapState::NeedSymbol { separator_offset };
                            return Ok(());
                        }
                        return Err(Error::BadWrap { offset });
                    }
                    WrapState::NeedSymbol { .. } => {
                        let separator_offset = match self.wrap_state {
                            WrapState::NeedSymbol { separator_offset } => separator_offset,
                            _ => unreachable!(),
                        };
                        if matches!(byte, 9 | 10 | 11 | 12 | 13 | 32) {
                            return Err(Error::BadWrap { offset });
                        }
                        self.wrap_state = WrapState::Normal;
                        self.row_symbols = 0;
                        if let Err(error) = self.process_symbol(byte, offset, out) {
                            return Err(error);
                        }
                        self.row_symbols += 1;
                        if self.row_symbols == width {
                            self.wrap_state = WrapState::NeedSeparator;
                        }
                        let _ = separator_offset;
                        return Ok(());
                    }
                    WrapState::Normal => {}
                }

                if self.row_symbols == width {
                    self.wrap_state = WrapState::NeedSeparator;
                    return self.process_byte(byte, offset, out);
                }

                if matches!(byte, 9 | 10 | 11 | 12 | 13 | 32) {
                    return Err(Error::BadWrap { offset });
                }

                self.process_symbol(byte, offset, out)?;
                self.row_symbols += 1;
                if self.row_symbols == width {
                    self.wrap_state = WrapState::NeedSeparator;
                }
                Ok(())
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
            if let Err(error) = self.process_byte(byte, offset, &mut out) {
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

        if let WrapState::NeedLf { cr_offset } = self.wrap_state {
            return self.set_sticky(Error::BadWrap { offset: cr_offset });
        }
        if let WrapState::NeedSymbol { separator_offset } = self.wrap_state {
            return self.set_sticky(Error::BadWrap {
                offset: separator_offset,
            });
        }

        if self.symbols.len() == 3 && self.symbols[2].0 == 64 {
            let offset = self.symbols[2].1;
            return self.set_sticky(Error::BadPadding { offset });
        }

        let mut out = Vec::new();
        if self.symbols.is_empty() {
            self.finished = true;
            return Ok(out);
        }

        match self.options.padding {
            Padding::Required => {
                let offset = self.symbols[0].1;
                self.set_sticky(Error::Truncated { offset })
            }
            Padding::Forbidden | Padding::Optional => match self.symbols.len() {
                1 => {
                    let offset = self.symbols[0].1;
                    self.set_sticky(Error::Truncated { offset })
                }
                2 => {
                    let a = self.symbols[0].0;
                    let b = self.symbols[1].0;
                    out.push((a << 2) | (b >> 4));
                    self.symbols.clear();
                    self.finished = true;
                    Ok(out)
                }
                3 => {
                    let a = self.symbols[0].0;
                    let b = self.symbols[1].0;
                    let c = self.symbols[2].0;
                    out.push((a << 2) | (b >> 4));
                    out.push((b << 4) | (c >> 2));
                    self.symbols.clear();
                    self.finished = true;
                    Ok(out)
                }
                _ => {
                    self.finished = true;
                    Ok(out)
                }
            },
        }
    }

    fn set_sticky(&mut self, error: Error) -> Result<Vec<u8>, Error> {
        self.sticky_error = Some(error.clone());
        Err(error)
    }
}