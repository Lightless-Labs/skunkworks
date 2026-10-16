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

fn encode_symbol(value: u8, alphabet: Alphabet) -> u8 {
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

fn decode_symbol(byte: u8, alphabet: Alphabet) -> Option<u8> {
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

fn encode_group(input: &[u8], alphabet: Alphabet, padded: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(4);
    let a = input[0];
    let b = if input.len() > 1 { input[1] } else { 0 };
    let c = if input.len() > 2 { input[2] } else { 0 };

    out.push(encode_symbol(a >> 2, alphabet));
    out.push(encode_symbol(((a & 0x03) << 4) | (b >> 4), alphabet));

    if input.len() > 1 {
        out.push(encode_symbol(((b & 0x0f) << 2) | (c >> 6), alphabet));
    } else if padded {
        out.push(b'=');
    }

    if input.len() > 2 {
        out.push(encode_symbol(c & 0x3f, alphabet));
    } else if padded {
        out.push(b'=');
    }

    out
}

fn format_wrapped(symbols: &[u8], width: usize, ending: LineEnding) -> String {
    let mut out = String::new();
    for (i, chunk) in symbols.chunks(width).enumerate() {
        if i != 0 {
            match ending {
                LineEnding::Lf => out.push('\n'),
                LineEnding::CrLf => out.push_str("\r\n"),
            }
        }
        for &b in chunk {
            out.push(b as char);
        }
    }
    out
}

pub fn encode(input: &[u8], options: Options) -> Result<String, Error> {
    validate_options(options)?;
    let padded = options.padding != Padding::Forbidden;
    let mut symbols = Vec::new();

    for chunk in input.chunks(3) {
        if chunk.len() == 3 {
            symbols.extend_from_slice(&encode_group(chunk, options.alphabet, padded));
        } else {
            symbols.extend_from_slice(&encode_group(chunk, options.alphabet, padded));
        }
    }

    Ok(match options.whitespace {
        Whitespace::Wrapped { width, ending } => format_wrapped(&symbols, width, ending),
        _ => symbols.into_iter().map(char::from).collect(),
    })
}

pub struct Encoder {
    options: Options,
    buffer: Vec<u8>,
    finished: bool,
}

impl Encoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate_options(options)?;
        Ok(Self {
            options,
            buffer: Vec::new(),
            finished: false,
        })
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }

        self.buffer.extend_from_slice(input);
        let complete_len = (self.buffer.len() / 3) * 3;
        let mut symbols = Vec::with_capacity((complete_len / 3) * 4);

        for chunk in self.buffer[..complete_len].chunks_exact(3) {
            symbols.extend_from_slice(&encode_group(chunk, self.options.alphabet, true));
        }
        self.buffer.drain(..complete_len);

        Ok(match self.options.whitespace {
            Whitespace::Wrapped { width, ending } => format_wrapped(&symbols, width, ending),
            _ => symbols.into_iter().map(char::from).collect(),
        })
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }

        self.finished = true;
        let symbols = if self.buffer.is_empty() {
            Vec::new()
        } else {
            encode_group(
                &self.buffer,
                self.options.alphabet,
                self.options.padding != Padding::Forbidden,
            )
        };
        self.buffer.clear();

        Ok(match self.options.whitespace {
            Whitespace::Wrapped { width, ending } => format_wrapped(&symbols, width, ending),
            _ => symbols.into_iter().map(char::from).collect(),
        })
    }
}

#[derive(Clone, Copy)]
struct Symbol {
    byte: u8,
    offset: usize,
}

pub struct Decoder {
    options: Options,
    offset: usize,
    symbols: Vec<Symbol>,
    terminal_padding: bool,
    pending_second_padding: Option<usize>,
    wrapped_symbols: usize,
    wrap_state: WrapState,
    saw_any_symbol: bool,
    finished: bool,
    sticky_error: Option<Error>,
}

#[derive(Clone, Copy)]
enum WrapState {
    Row,
    NeedLf { cr_offset: usize },
    NeedSymbol { separator_offset: usize },
}

impl Decoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate_options(options)?;
        Ok(Self {
            options,
            offset: 0,
            symbols: Vec::with_capacity(4),
            terminal_padding: false,
            pending_second_padding: None,
            wrapped_symbols: 0,
            wrap_state: WrapState::Row,
            saw_any_symbol: false,
            finished: false,
            sticky_error: None,
        })
    }

    fn fail<T>(&mut self, error: Error) -> Result<T, Error> {
        self.sticky_error = Some(error.clone());
        Err(error)
    }

    fn process_symbol(&mut self, byte: u8, offset: usize, output: &mut Vec<u8>) -> Result<(), Error> {
        if self.terminal_padding {
            return Err(Error::BadPadding { offset });
        }

        if self.pending_second_padding.is_some() && byte != b'=' {
            return Err(Error::BadPadding { offset });
        }

        let position = self.symbols.len();
        if byte == b'=' {
            if self.options.padding == Padding::Forbidden {
                return Err(Error::BadPadding { offset });
            }
            match position {
                0 | 1 => return Err(Error::BadPadding { offset }),
                2 => {
                    self.pending_second_padding = Some(offset);
                    self.symbols.push(Symbol { byte, offset });
                    self.saw_any_symbol = true;
                    return Ok(());
                }
                3 => {
                    if self.pending_second_padding.is_none() {
                        if self.symbols[2].byte == b'=' {
                            // xx== is completed here.
                        } else {
                            // xxx= is completed here.
                        }
                    }
                    self.symbols.push(Symbol { byte, offset });
                    self.emit_group(output);
                    self.terminal_padding = true;
                    self.pending_second_padding = None;
                    self.saw_any_symbol = true;
                    return Ok(());
                }
                _ => unreachable!(),
            }
        }

        let value = decode_symbol(byte, self.options.alphabet)
            .ok_or(Error::InvalidByte { offset, byte })?;
        self.symbols.push(Symbol { byte, offset });
        self.saw_any_symbol = true;

        if self.symbols.len() == 4 {
            let _ = value;
            self.emit_group(output);
        }
        Ok(())
    }

    fn emit_group(&mut self, output: &mut Vec<u8>) {
        let a = decode_symbol(self.symbols[0].byte, self.options.alphabet).unwrap();
        let b = decode_symbol(self.symbols[1].byte, self.options.alphabet).unwrap();

        if self.symbols[2].byte == b'=' {
            output.push((a << 2) | (b >> 4));
        } else {
            let c = decode_symbol(self.symbols[2].byte, self.options.alphabet).unwrap();
            if self.symbols[3].byte == b'=' {
                output.push((a << 2) | (b >> 4));
                output.push((b << 4) | (c >> 2));
            } else {
                let d = decode_symbol(self.symbols[3].byte, self.options.alphabet).unwrap();
                output.push((a << 2) | (b >> 4));
                output.push((b << 4) | (c >> 2));
                output.push((c << 6) | d);
            }
        }

        self.symbols.clear();
    }

    fn skip_ascii(byte: u8) -> bool {
        matches!(byte, 9 | 10 | 11 | 12 | 13 | 32)
    }

    pub fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky_error {
            return Err(error.clone());
        }
        if self.finished {
            let error = Error::AlreadyFinished;
            self.sticky_error = Some(error.clone());
            return Err(error);
        }

        let mut output = Vec::new();
        for &byte in input {
            let offset = self.offset;
            self.offset += 1;

            if matches!(self.options.whitespace, Whitespace::IgnoreAscii)
                && Self::skip_ascii(byte)
            {
                continue;
            }

            if let Whitespace::Wrapped { width, ending } = self.options.whitespace {
                match self.wrap_state {
                    WrapState::NeedLf { cr_offset } => {
                        if byte != b'\n' {
                            return self.fail(Error::BadWrap { offset });
                        }
                        self.wrap_state = WrapState::NeedSymbol {
                            separator_offset: cr_offset,
                        };
                        continue;
                    }
                    WrapState::NeedSymbol { separator_offset } => {
                        self.wrap_state = WrapState::Row;
                        if matches!(byte, b'\r' | b'\n' | 9 | 11 | 12 | 32) {
                            return self.fail(Error::BadWrap { offset });
                        }
                        let _ = separator_offset;
                    }
                    WrapState::Row => {
                        if self.wrapped_symbols == width {
                            match ending {
                                LineEnding::Lf if byte == b'\n' => {
                                    self.wrap_state = WrapState::NeedSymbol {
                                        separator_offset: offset,
                                    };
                                    self.wrapped_symbols = 0;
                                    continue;
                                }
                                LineEnding::CrLf if byte == b'\r' => {
                                    self.wrap_state = WrapState::NeedLf { cr_offset: offset };
                                    self.wrapped_symbols = 0;
                                    continue;
                                }
                                _ => return self.fail(Error::BadWrap { offset }),
                            }
                        } else if matches!(byte, b'\r' | b'\n' | 9 | 11 | 12 | 32) {
                            return self.fail(Error::BadWrap { offset });
                        }
                    }
                }
                self.wrapped_symbols += 1;
            }

            if let Err(error) = self.process_symbol(byte, offset, &mut output) {
                return self.fail(error);
            }
        }

        Ok(output)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky_error {
            return Err(error.clone());
        }
        if self.finished {
            let error = Error::AlreadyFinished;
            self.sticky_error = Some(error.clone());
            return Err(error);
        }

        if let WrapState::NeedLf { cr_offset } = self.wrap_state {
            return self.fail(Error::BadWrap { offset: cr_offset });
        }
        if let WrapState::NeedSymbol { separator_offset } = self.wrap_state {
            return self.fail(Error::BadWrap {
                offset: separator_offset,
            });
        }

        if let Some(offset) = self.pending_second_padding {
            return self.fail(Error::BadPadding { offset });
        }

        let mut output = Vec::new();
        if !self.symbols.is_empty() {
            match self.symbols.len() {
                1 => return self.fail(Error::Truncated {
                    offset: self.symbols[0].offset,
                }),
                2 | 3 if self.options.padding != Padding::Required => {
                    let a = decode_symbol(self.symbols[0].byte, self.options.alphabet).unwrap();
                    let b = decode_symbol(self.symbols[1].byte, self.options.alphabet).unwrap();
                    output.push((a << 2) | (b >> 4));
                    if self.symbols.len() == 3 {
                        let c = decode_symbol(self.symbols[2].byte, self.options.alphabet).unwrap();
                        output.push((b << 4) | (c >> 2));
                    }
                    self.symbols.clear();
                }
                _ => return self.fail(Error::Truncated {
                    offset: self.symbols[0].offset,
                }),
            }
        }

        self.finished = true;
        Ok(output)
    }
}

pub fn decode(input: &[u8], options: Options) -> Result<Vec<u8>, Error> {
    validate_options(options)?;
    let mut decoder = Decoder::new(options)?;
    let mut output = decoder.push(input)?;
    output.extend(decoder.finish()?);
    Ok(output)
}