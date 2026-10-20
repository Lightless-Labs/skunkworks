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

fn digit(byte: u8, alphabet: Alphabet) -> Option<u8> {
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

fn encode_group(input: &[u8], alphabet: Alphabet, padded: bool) -> String {
    let table = match alphabet {
        Alphabet::Standard => b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/",
        Alphabet::UrlSafe => b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_",
    };
    let a = input[0];
    let b = if input.len() > 1 { input[1] } else { 0 };
    let c = if input.len() > 2 { input[2] } else { 0 };

    let mut out = String::with_capacity(4);
    out.push(table[(a >> 2) as usize] as char);
    out.push(table[(((a & 3) << 4) | (b >> 4)) as usize] as char);
    if input.len() > 1 {
        out.push(table[(((b & 15) << 2) | (c >> 6)) as usize] as char);
    } else if padded {
        out.push('=');
    }
    if input.len() > 2 {
        out.push(table[(c & 63) as usize] as char);
    } else if padded {
        out.push('=');
    }
    out
}

pub fn encode(input: &[u8], options: Options) -> Result<String, Error> {
    validate(options)?;
    let mut encoder = Encoder::new(options)?;
    let mut output = encoder.push(input)?;
    output.push_str(&encoder.finish()?);
    Ok(output)
}

pub fn decode(input: &[u8], options: Options) -> Result<Vec<u8>, Error> {
    validate(options)?;
    let mut decoder = Decoder::new(options)?;
    let mut output = decoder.push(input)?;
    output.extend(decoder.finish()?);
    Ok(output)
}

pub struct Encoder {
    options: Options,
    pending: Vec<u8>,
    finished: bool,
    column: usize,
}

impl Encoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            pending: Vec::with_capacity(2),
            finished: false,
            column: 0,
        })
    }

    fn append_symbols(&mut self, symbols: &str, output: &mut String) {
        if let Whitespace::Wrapped { width, ending } = self.options.whitespace {
            for ch in symbols.chars() {
                if self.column == width {
                    match ending {
                        LineEnding::Lf => output.push('\n'),
                        LineEnding::CrLf => output.push_str("\r\n"),
                    }
                    self.column = 0;
                }
                output.push(ch);
                self.column += 1;
            }
        } else {
            output.push_str(symbols);
        }
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        let mut output = String::new();
        for &byte in input {
            self.pending.push(byte);
            if self.pending.len() == 3 {
                let group = encode_group(&self.pending, self.options.alphabet, true);
                self.append_symbols(&group, &mut output);
                self.pending.clear();
            }
        }
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        self.finished = true;
        let mut output = String::new();
        if !self.pending.is_empty() {
            let padded = self.options.padding != Padding::Forbidden;
            let group = encode_group(&self.pending, self.options.alphabet, padded);
            self.append_symbols(&group, &mut output);
            self.pending.clear();
        }
        Ok(output)
    }
}

#[derive(Clone, Copy)]
struct Symbol {
    byte: u8,
    offset: usize,
}

pub struct Decoder {
    options: Options,
    group: Vec<Symbol>,
    terminal_padding: bool,
    pending_second_pad: Option<usize>,
    offset: usize,
    finished: bool,
    sticky: Option<Error>,
    row_count: usize,
    wrap_state: WrapState,
}

#[derive(Clone, Copy)]
enum WrapState {
    Normal,
    NeedSeparator { first: usize, cr_seen: bool },
    NeedSymbol { first: usize },
}

impl Decoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            group: Vec::with_capacity(4),
            terminal_padding: false,
            pending_second_pad: None,
            offset: 0,
            finished: false,
            sticky: None,
            row_count: 0,
            wrap_state: WrapState::Normal,
        })
    }

    fn bad_padding(&self, offset: usize) -> Error {
        Error::BadPadding { offset }
    }

    fn emit_group(&mut self, output: &mut Vec<u8>) {
        let vals: Vec<u8> = self
            .group
            .iter()
            .map(|s| digit(s.byte, self.options.alphabet).unwrap_or(0))
            .collect();
        let a = vals[0];
        let b = vals[1];
        output.push((a << 2) | (b >> 4));
        if self.group[2].byte != b'=' {
            let c = vals[2];
            output.push((b << 4) | (c >> 2));
            if self.group[3].byte != b'=' {
                output.push((c << 6) | vals[3]);
            }
        }
        if self.group[2].byte == b'=' || self.group[3].byte == b'=' {
            self.terminal_padding = true;
        }
        self.group.clear();
    }

    fn process_symbol(&mut self, symbol: Symbol, output: &mut Vec<u8>) -> Result<(), Error> {
        if self.terminal_padding {
            return Err(self.bad_padding(symbol.offset));
        }
        if let Some(pad_offset) = self.pending_second_pad.take() {
            if symbol.byte != b'=' {
                return Err(self.bad_padding(symbol.offset));
            }
            if self.options.padding == Padding::Forbidden {
                return Err(self.bad_padding(pad_offset));
            }
            self.group.push(symbol);
            self.emit_group(output);
            return Ok(());
        }

        let pos = self.group.len();
        if symbol.byte == b'=' {
            if pos <= 1 {
                return Err(self.bad_padding(symbol.offset));
            }
            if pos == 2 {
                if self.options.padding == Padding::Forbidden {
                    return Err(self.bad_padding(symbol.offset));
                }
                self.group.push(symbol);
                self.pending_second_pad = Some(symbol.offset);
                return Ok(());
            }
            if pos == 3 {
                if self.options.padding == Padding::Forbidden {
                    return Err(self.bad_padding(symbol.offset));
                }
                self.group.push(symbol);
                self.emit_group(output);
                return Ok(());
            }
        } else if digit(symbol.byte, self.options.alphabet).is_none() {
            return Err(Error::InvalidByte {
                offset: symbol.offset,
                byte: symbol.byte,
            });
        }

        self.group.push(symbol);
        if self.group.len() == 4 {
            self.emit_group(output);
        }
        Ok(())
    }

    fn is_ignored(byte: u8) -> bool {
        matches!(byte, 9 | 10 | 11 | 12 | 13 | 32)
    }

    fn wrapped_byte(&mut self, byte: u8, offset: usize) -> Result<Option<Symbol>, Error> {
        let Whitespace::Wrapped { width, ending } = self.options.whitespace else {
            if self.options.whitespace == Whitespace::IgnoreAscii && Self::is_ignored(byte) {
                return Ok(None);
            }
            return Ok(Some(Symbol { byte, offset }));
        };

        match self.wrap_state {
            WrapState::Normal => {
                if self.row_count == width {
                    let expected = match ending {
                        LineEnding::Lf => byte == b'\n',
                        LineEnding::CrLf => byte == b'\r',
                    };
                    if !expected {
                        return Err(Error::BadWrap { offset });
                    }
                    self.wrap_state = match ending {
                        LineEnding::Lf => WrapState::NeedSymbol { first: offset },
                        LineEnding::CrLf => {
                            WrapState::NeedSeparator { first: offset, cr_seen: true }
                        }
                    };
                    return Ok(None);
                }
                if Self::is_ignored(byte) {
                    return Err(Error::BadWrap { offset });
                }
                self.row_count += 1;
                Ok(Some(Symbol { byte, offset }))
            }
            WrapState::NeedSeparator { first, cr_seen: true } => {
                if byte != b'\n' {
                    return Err(Error::BadWrap { offset });
                }
                self.wrap_state = WrapState::NeedSymbol { first };
                Ok(None)
            }
            WrapState::NeedSeparator { .. } => unreachable!(),
            WrapState::NeedSymbol { first } => {
                if Self::is_ignored(byte) {
                    return Err(Error::BadWrap { offset });
                }
                self.row_count = 1;
                self.wrap_state = WrapState::Normal;
                Ok(Some(Symbol { byte, offset }))
            }
        }
    }

    pub fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky {
            return Err(error.clone());
        }
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        let mut output = Vec::new();
        for &byte in input {
            let offset = self.offset;
            self.offset += 1;
            match self.wrapped_byte(byte, offset) {
                Ok(Some(symbol)) => {
                    if let Err(error) = self.process_symbol(symbol, &mut output) {
                        self.sticky = Some(error.clone());
                        return Err(error);
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    self.sticky = Some(error.clone());
                    return Err(error);
                }
            }
        }
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky {
            return Err(error.clone());
        }
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        if let WrapState::NeedSymbol { first } = self.wrap_state {
            let error = Error::BadWrap { offset: first };
            self.sticky = Some(error.clone());
            return Err(error);
        }
        if let WrapState::NeedSeparator { first, .. } = self.wrap_state {
            let error = Error::BadWrap { offset: first };
            self.sticky = Some(error.clone());
            return Err(error);
        }
        if let Some(offset) = self.pending_second_pad {
            let error = self.bad_padding(offset);
            self.sticky = Some(error.clone());
            return Err(error);
        }

        let mut output = Vec::new();
        if !self.group.is_empty() {
            if self.options.padding == Padding::Required || self.group.len() == 1 {
                let error = Error::Truncated {
                    offset: self.group[0].offset,
                };
                self.sticky = Some(error.clone());
                return Err(error);
            }
            let vals: Vec<u8> = self
                .group
                .iter()
                .map(|s| digit(s.byte, self.options.alphabet).unwrap_or(0))
                .collect();
            output.push((vals[0] << 2) | (vals[1] >> 4));
            if self.group.len() == 3 {
                output.push((vals[1] << 4) | (vals[2] >> 2));
            }
        }
        self.finished = true;
        Ok(output)
    }
}