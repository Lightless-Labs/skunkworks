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

fn encode_digit(value: u8, alphabet: Alphabet) -> u8 {
    match value {
        0..=25 => b'A' + value,
        26..=51 => b'a' + value - 26,
        52..=61 => b'0' + value - 52,
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

fn is_foreign_alphabet_byte(byte: u8, alphabet: Alphabet) -> bool {
    match alphabet {
        Alphabet::Standard => byte == b'-' || byte == b'_',
        Alphabet::UrlSafe => byte == b'+' || byte == b'/',
    }
}

fn encode_group(input: &[u8], options: Options) -> Vec<u8> {
    let a = input[0];
    let b = if input.len() > 1 { input[1] } else { 0 };
    let c = if input.len() > 2 { input[2] } else { 0 };

    let mut out = vec![
        encode_digit(a >> 2, options.alphabet),
        encode_digit(((a & 0x03) << 4) | (b >> 4), options.alphabet),
    ];

    if input.len() > 1 {
        out.push(encode_digit(((b & 0x0f) << 2) | (c >> 6), options.alphabet));
    } else if options.padding != Padding::Forbidden {
        out.extend_from_slice(b"==");
        return out;
    }

    if input.len() > 2 {
        out.push(encode_digit(c & 0x3f, options.alphabet));
    } else if options.padding != Padding::Forbidden {
        out.push(b'=');
    }

    out
}

pub fn encode(input: &[u8], options: Options) -> Result<String, Error> {
    validate(options)?;
    let mut encoder = Encoder::new(options)?;
    let mut result = encoder.push(input)?;
    result.push_str(&encoder.finish()?);
    Ok(result)
}

pub fn decode(input: &[u8], options: Options) -> Result<Vec<u8>, Error> {
    validate(options)?;
    let mut decoder = Decoder::new(options)?;
    let mut result = decoder.push(input)?;
    result.extend(decoder.finish()?);
    Ok(result)
}

pub struct Encoder {
    options: Options,
    tail: Vec<u8>,
    column: usize,
    closed: bool,
}

impl Encoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            tail: Vec::new(),
            column: 0,
            closed: false,
        })
    }

    fn append_symbols(&mut self, out: &mut String, symbols: &[u8]) {
        if let Whitespace::Wrapped { width, ending } = self.options.whitespace {
            for &symbol in symbols {
                if self.column == width {
                    match ending {
                        LineEnding::Lf => out.push('\n'),
                        LineEnding::CrLf => out.push_str("\r\n"),
                    }
                    self.column = 0;
                }
                out.push(symbol as char);
                self.column += 1;
            }
        } else {
            for &symbol in symbols {
                out.push(symbol as char);
            }
        }
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }

        let mut out = String::new();
        for &byte in input {
            self.tail.push(byte);
            if self.tail.len() == 3 {
                let symbols = encode_group(&self.tail, self.options);
                self.append_symbols(&mut out, &symbols);
                self.tail.clear();
            }
        }
        Ok(out)
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }

        let mut out = String::new();
        if !self.tail.is_empty() {
            let symbols = encode_group(&self.tail, self.options);
            self.append_symbols(&mut out, &symbols);
            self.tail.clear();
        }
        self.closed = true;
        Ok(out)
    }
}

#[derive(Clone, Copy)]
struct Symbol {
    value: u8,
    offset: usize,
}

#[derive(Clone, Copy)]
enum FrameState {
    Row,
    NeedLf { cr_offset: usize },
    NeedSymbol { sep_offset: usize },
}

pub struct Decoder {
    options: Options,
    offset: usize,
    symbols_seen: usize,
    group: Vec<Symbol>,
    pending_second_pad: Option<usize>,
    terminal_pad: bool,
    frame_column: usize,
    frame_state: FrameState,
    closed: bool,
    sticky: Option<Error>,
}

impl Decoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            offset: 0,
            symbols_seen: 0,
            group: Vec::with_capacity(4),
            pending_second_pad: None,
            terminal_pad: false,
            frame_column: 0,
            frame_state: FrameState::Row,
            closed: false,
            sticky: None,
        })
    }

    fn emit_padded_group(&mut self, out: &mut Vec<u8>, one_byte: bool) -> Result<(), Error> {
        let a = self.group[0].value;
        let b = self.group[1].value;

        if one_byte {
            if self.options.strict && b & 0x0f != 0 {
                return Err(Error::NonCanonical {
                    offset: self.group[1].offset,
                });
            }
            out.push((a << 2) | (b >> 4));
        } else {
            let c = self.group[2].value;
            if self.options.strict && c & 0x03 != 0 {
                return Err(Error::NonCanonical {
                    offset: self.group[2].offset,
                });
            }
            out.push((a << 2) | (b >> 4));
            out.push((b << 4) | (c >> 2));
        }

        self.group.clear();
        Ok(())
    }

    fn scan_symbol(&mut self, byte: u8, at: usize, out: &mut Vec<u8>) -> Result<(), Error> {
        if let Some(limit) = self.options.max_symbols {
            if self.symbols_seen >= limit {
                return Err(Error::Overlong { offset: at, limit });
            }
        }
        self.symbols_seen += 1;

        if self.terminal_pad {
            return Err(Error::BadPadding { offset: at });
        }

        if self.pending_second_pad.is_some() {
            if byte != b'=' {
                return Err(Error::BadPadding { offset: at });
            }
            self.group.push(Symbol {
                value: 0,
                offset: at,
            });
            self.pending_second_pad = None;
            self.emit_padded_group(out, true)?;
            self.terminal_pad = true;
            return Ok(());
        }

        let position = self.group.len();
        if byte == b'=' {
            if self.options.padding == Padding::Forbidden {
                return Err(Error::BadPadding { offset: at });
            }
            match position {
                0 | 1 => Err(Error::BadPadding { offset: at }),
                2 => {
                    self.pending_second_pad = Some(at);
                    self.group.push(Symbol {
                        value: 0,
                        offset: at,
                    });
                    Ok(())
                }
                3 => {
                    self.group.push(Symbol {
                        value: 0,
                        offset: at,
                    });
                    self.emit_padded_group(out, false)?;
                    self.terminal_pad = true;
                    Ok(())
                }
                _ => Err(Error::BadPadding { offset: at }),
            }?;
            return Ok(());
        }

        let value = match decode_digit(byte, self.options.alphabet) {
            Some(value) => value,
            None
                if self.options.strict
                    && is_foreign_alphabet_byte(byte, self.options.alphabet) =>
            {
                return Err(Error::MixedAlphabet { offset: at });
            }
            None => return Err(Error::InvalidByte { offset: at, byte }),
        };

        self.group.push(Symbol { value, offset: at });
        if self.group.len() == 4 {
            let a = self.group[0].value;
            let b = self.group[1].value;
            let c = self.group[2].value;
            let d = self.group[3].value;
            out.push((a << 2) | (b >> 4));
            out.push((b << 4) | (c >> 2));
            out.push((c << 6) | d);
            self.group.clear();
        }
        Ok(())
    }

    fn framing_byte(&mut self, byte: u8, at: usize) -> Result<bool, Error> {
        match self.options.whitespace {
            Whitespace::None => Ok(false),
            Whitespace::IgnoreAscii => Ok(matches!(byte, 9 | 10 | 11 | 12 | 13 | 32)),
            Whitespace::Wrapped { width, ending } => match self.frame_state {
                FrameState::NeedLf { cr_offset } => {
                    if byte == b'\n' {
                        self.frame_state = FrameState::NeedSymbol {
                            sep_offset: cr_offset,
                        };
                        Ok(true)
                    } else {
                        Err(Error::BadWrap { offset: at })
                    }
                }
                FrameState::NeedSymbol { .. } => {
                    self.frame_column = 1;
                    self.frame_state = FrameState::Row;
                    Ok(false)
                }
                FrameState::Row => {
                    if self.frame_column == width {
                        match ending {
                            LineEnding::Lf if byte == b'\n' => {
                                self.frame_column = 0;
                                self.frame_state = FrameState::NeedSymbol { sep_offset: at };
                                Ok(true)
                            }
                            LineEnding::CrLf if byte == b'\r' => {
                                self.frame_state = FrameState::NeedLf { cr_offset: at };
                                Ok(true)
                            }
                            _ => Err(Error::BadWrap { offset: at }),
                        }
                    } else if matches!(byte, 9 | 10 | 11 | 12 | 13 | 32) {
                        Err(Error::BadWrap { offset: at })
                    } else {
                        self.frame_column += 1;
                        Ok(false)
                    }
                }
            },
        }
    }

    fn eof(&self) -> Result<(), Error> {
        match self.frame_state {
            FrameState::NeedLf { cr_offset } => return Err(Error::BadWrap { offset: cr_offset }),
            FrameState::NeedSymbol { sep_offset } => {
                return Err(Error::BadWrap { offset: sep_offset });
            }
            FrameState::Row => {}
        }

        if let Some(offset) = self.pending_second_pad {
            return Err(Error::BadPadding { offset });
        }

        if self.group.is_empty() {
            return Ok(());
        }

        if self.options.padding == Padding::Required || self.group.len() == 1 {
            return Err(Error::Truncated {
                offset: self.group[0].offset,
            });
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

        let mut out = Vec::new();
        for &byte in input {
            let at = self.offset;
            self.offset += 1;

            let skipped = match self.framing_byte(byte, at) {
                Ok(skipped) => skipped,
                Err(error) => {
                    self.sticky = Some(error.clone());
                    return Err(error);
                }
            };
            if skipped {
                continue;
            }

            if let Err(error) = self.scan_symbol(byte, at, &mut out) {
                self.sticky = Some(error.clone());
                return Err(error);
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

        if let Err(error) = self.eof() {
            self.sticky = Some(error.clone());
            return Err(error);
        }

        let mut out = Vec::new();
        match self.group.len() {
            0 => {}
            2 => {
                let a = self.group[0].value;
                let b = self.group[1].value;
                if self.options.strict && b & 0x0f != 0 {
                    let error = Error::NonCanonical {
                        offset: self.group[1].offset,
                    };
                    self.sticky = Some(error.clone());
                    return Err(error);
                }
                out.push((a << 2) | (b >> 4));
            }
            3 => {
                let a = self.group[0].value;
                let b = self.group[1].value;
                let c = self.group[2].value;
                if self.options.strict && c & 0x03 != 0 {
                    let error = Error::NonCanonical {
                        offset: self.group[2].offset,
                    };
                    self.sticky = Some(error.clone());
                    return Err(error);
                }
                out.push((a << 2) | (b >> 4));
                out.push((b << 4) | (c >> 2));
            }
            _ => {}
        }

        self.group.clear();
        self.closed = true;
        Ok(out)
    }
}