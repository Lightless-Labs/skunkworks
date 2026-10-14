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

fn validate_options(options: Options) -> Result<(), Error> {
    if let Whitespace::Wrapped { width, .. } = options.whitespace {
        if !(4..=120).contains(&width) || width % 4 != 0 {
            return Err(Error::InvalidWidth { width });
        }
    }
    Ok(())
}

fn alphabet_byte(alphabet: Alphabet, value: u8) -> u8 {
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

fn lookup(byte: u8, alphabet: Alphabet) -> Option<u8> {
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

fn is_foreign(byte: u8, alphabet: Alphabet) -> bool {
    match alphabet {
        Alphabet::Standard => byte == b'-' || byte == b'_',
        Alphabet::UrlSafe => byte == b'+' || byte == b'/',
    }
}

fn ignored_ascii(byte: u8) -> bool {
    matches!(byte, 9 | 10 | 11 | 12 | 13 | 32)
}

pub struct Encoder {
    options: Options,
    tail: Vec<u8>,
    symbols_in_row: usize,
    closed: bool,
}

impl Encoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate_options(options)?;
        Ok(Self {
            options,
            tail: Vec::new(),
            symbols_in_row: 0,
            closed: false,
        })
    }

    fn emit_symbols(&mut self, symbols: &[u8], output: &mut String) {
        let (width, ending) = match self.options.whitespace {
            Whitespace::Wrapped { width, ending } => (Some(width), Some(ending)),
            _ => (None, None),
        };

        for &symbol in symbols {
            if let Some(width) = width {
                if self.symbols_in_row == width {
                    match ending.unwrap() {
                        LineEnding::Lf => output.push('\n'),
                        LineEnding::CrLf => output.push_str("\r\n"),
                    }
                    self.symbols_in_row = 0;
                }
            }
            output.push(symbol as char);
            self.symbols_in_row += 1;
        }
    }

    fn encode_block(&mut self, block: &[u8], output: &mut String) {
        let a = block[0];
        let b = block[1];
        let c = block[2];
        let symbols = [
            alphabet_byte(self.options.alphabet, a >> 2),
            alphabet_byte(self.options.alphabet, ((a & 0x03) << 4) | (b >> 4)),
            alphabet_byte(self.options.alphabet, ((b & 0x0f) << 2) | (c >> 6)),
            alphabet_byte(self.options.alphabet, c & 0x3f),
        ];
        self.emit_symbols(&symbols, output);
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }

        let mut output = String::new();
        let mut index = 0;

        if !self.tail.is_empty() {
            while self.tail.len() < 3 && index < input.len() {
                self.tail.push(input[index]);
                index += 1;
            }
            if self.tail.len() == 3 {
                let block = [self.tail[0], self.tail[1], self.tail[2]];
                self.encode_block(&block, &mut output);
                self.tail.clear();
            }
        }

        while index + 3 <= input.len() {
            self.encode_block(&input[index..index + 3], &mut output);
            index += 3;
        }

        self.tail.extend_from_slice(&input[index..]);
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }

        let mut output = String::new();
        match self.tail.len() {
            0 => {}
            1 => {
                let a = self.tail[0];
                let mut symbols = vec![
                    alphabet_byte(self.options.alphabet, a >> 2),
                    alphabet_byte(self.options.alphabet, (a & 0x03) << 4),
                ];
                if self.options.padding != Padding::Forbidden {
                    symbols.extend_from_slice(b"==");
                }
                self.emit_symbols(&symbols, &mut output);
            }
            2 => {
                let a = self.tail[0];
                let b = self.tail[1];
                let mut symbols = vec![
                    alphabet_byte(self.options.alphabet, a >> 2),
                    alphabet_byte(self.options.alphabet, ((a & 0x03) << 4) | (b >> 4)),
                    alphabet_byte(self.options.alphabet, (b & 0x0f) << 2),
                ];
                if self.options.padding != Padding::Forbidden {
                    symbols.push(b'=');
                }
                self.emit_symbols(&symbols, &mut output);
            }
            _ => unreachable!(),
        }

        self.tail.clear();
        self.closed = true;
        Ok(output)
    }
}

pub fn encode(input: &[u8], options: Options) -> Result<String, Error> {
    let mut encoder = Encoder::new(options)?;
    let mut output = encoder.push(input)?;
    output.push_str(&encoder.finish()?);
    Ok(output)
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
    symbols_seen: usize,
    group: Vec<Symbol>,
    pending_second_pad: Option<usize>,
    terminal_padding: bool,
    emitted: Vec<u8>,
    row_symbols: usize,
    frame_state: FrameState,
    closed: bool,
    sticky: Option<Error>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FrameState {
    Normal,
    NeedLf { cr_offset: usize },
    NeedSymbol { separator_offset: usize },
}

impl Decoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate_options(options)?;
        Ok(Self {
            options,
            offset: 0,
            symbols_seen: 0,
            group: Vec::with_capacity(4),
            pending_second_pad: None,
            terminal_padding: false,
            emitted: Vec::new(),
            row_symbols: 0,
            frame_state: FrameState::Normal,
            closed: false,
            sticky: None,
        })
    }

    fn fail(&mut self, error: Error) -> Error {
        self.sticky = Some(error.clone());
        error
    }

    fn decode_group(&mut self, output: &mut Vec<u8>) -> Result<(), Error> {
        let g = &self.group;
        if g.len() != 4 {
            return Ok(());
        }

        let a = g[0].value.unwrap();
        let b = g[1].value.unwrap();

        if g[2].byte == b'=' {
            if g[3].byte != b'=' {
                return Err(Error::BadPadding { offset: g[3].offset });
            }
            if self.options.strict && b & 0x0f != 0 {
                return Err(Error::NonCanonical { offset: g[1].offset });
            }
            output.push((a << 2) | (b >> 4));
            self.terminal_padding = true;
        } else {
            let c = g[2].value.unwrap();
            if g[3].byte == b'=' {
                if self.options.strict && c & 0x03 != 0 {
                    return Err(Error::NonCanonical { offset: g[2].offset });
                }
                output.push((a << 2) | (b >> 4));
                output.push((b << 4) | (c >> 2));
                self.terminal_padding = true;
            } else {
                let d = g[3].value.unwrap();
                output.push((a << 2) | (b >> 4));
                output.push((b << 4) | (c >> 2));
                output.push((c << 6) | d);
            }
        }

        self.group.clear();
        self.pending_second_pad = None;
        Ok(())
    }

    fn process_symbol(&mut self, byte: u8, offset: usize, output: &mut Vec<u8>) -> Result<(), Error> {
        if let Some(limit) = self.options.max_symbols {
            if self.symbols_seen >= limit {
                return Err(Error::Overlong { offset, limit });
            }
        }
        self.symbols_seen += 1;

        if self.terminal_padding {
            return Err(Error::BadPadding { offset });
        }
        if self.pending_second_pad.is_some() && byte != b'=' {
            return Err(Error::BadPadding { offset });
        }

        let position = self.group.len();

        if byte == b'=' {
            if self.options.padding == Padding::Forbidden {
                return Err(Error::BadPadding { offset });
            }
            match position {
                0 | 1 => return Err(Error::BadPadding { offset }),
                2 => {
                    self.group.push(Symbol {
                        byte,
                        value: None,
                        offset,
                    });
                    self.pending_second_pad = Some(offset);
                    return Ok(());
                }
                3 => {
                    self.group.push(Symbol {
                        byte,
                        value: None,
                        offset,
                    });
                    self.decode_group(output)?;
                    return Ok(());
                }
                _ => unreachable!(),
            }
        }

        let value = lookup(byte, self.options.alphabet);
        let Some(value) = value else {
            if self.options.strict && is_foreign(byte, self.options.alphabet) {
                return Err(Error::MixedAlphabet { offset });
            }
            return Err(Error::InvalidByte { offset, byte });
        };

        self.group.push(Symbol {
            byte,
            value: Some(value),
            offset,
        });

        if self.group.len() == 4 {
            self.decode_group(output)?;
        }
        Ok(())
    }

    fn framing_byte(&mut self, byte: u8, offset: usize) -> Result<Option<bool>, Error> {
        // Returns Some(true) for a skipped byte, Some(false) for a symbol,
        // and None when the byte is consumed as framing.
        match self.options.whitespace {
            Whitespace::None => Ok(Some(false)),
            Whitespace::IgnoreAscii => {
                if ignored_ascii(byte) {
                    Ok(Some(true))
                } else {
                    Ok(Some(false))
                }
            }
            Whitespace::Wrapped { width, ending } => match self.frame_state {
                FrameState::NeedLf { cr_offset } => {
                    if byte == b'\n' {
                        self.frame_state = FrameState::NeedSymbol {
                            separator_offset: cr_offset,
                        };
                        Ok(None)
                    } else {
                        Err(Error::BadWrap { offset })
                    }
                }
                FrameState::NeedSymbol { separator_offset } => {
                    // A symbol is required after the separator. Do not consume
                    // it here; normal width checks apply below.
                    self.frame_state = FrameState::Normal;
                    let _ = separator_offset;
                    self.wrapped_symbol_start(byte, offset, width, ending)?;
                    Ok(Some(false))
                }
                FrameState::Normal => {
                    if self.row_symbols == width {
                        match ending {
                            LineEnding::Lf if byte == b'\n' => {
                                self.row_symbols = 0;
                                self.frame_state = FrameState::NeedSymbol {
                                    separator_offset: offset,
                                };
                                Ok(None)
                            }
                            LineEnding::CrLf if byte == b'\r' => {
                                self.row_symbols = 0;
                                self.frame_state = FrameState::NeedLf {
                                    cr_offset: offset,
                                };
                                Ok(None)
                            }
                            _ => Err(Error::BadWrap { offset }),
                        }
                    } else if matches!(byte, b'\r' | b'\n') {
                        Err(Error::BadWrap { offset })
                    } else {
                        self.row_symbols += 1;
                        Ok(Some(false))
                    }
                }
            },
        }
    }

    fn wrapped_symbol_start(
        &mut self,
        byte: u8,
        offset: usize,
        width: usize,
        _ending: LineEnding,
    ) -> Result<(), Error> {
        if matches!(byte, b'\r' | b'\n') {
            return Err(Error::BadWrap { offset });
        }
        self.row_symbols = 1;
        Ok(())
    }

    fn scan_byte(&mut self, byte: u8, offset: usize, output: &mut Vec<u8>) -> Result<(), Error> {
        let framed = match self.framing_byte(byte, offset) {
            Ok(value) => value,
            Err(error) => return Err(error),
        };
        match framed {
            None => Ok(()),
            Some(true) => Ok(()),
            Some(false) => self.process_symbol(byte, offset, output),
        }
    }

    fn finish_inner(&mut self) -> Result<Vec<u8>, Error> {
        match self.frame_state {
            FrameState::NeedLf { cr_offset } => {
                return Err(Error::BadWrap { offset: cr_offset });
            }
            FrameState::NeedSymbol { separator_offset } => {
                return Err(Error::BadWrap {
                    offset: separator_offset,
                });
            }
            FrameState::Normal => {}
        }

        if let Whitespace::Wrapped { width, .. } = self.options.whitespace {
            if self.row_symbols > width {
                unreachable!();
            }
        }

        if let Some(offset) = self.pending_second_pad {
            return Err(Error::BadPadding { offset });
        }

        if self.options.padding == Padding::Required && !self.group.is_empty() {
            return Err(Error::Truncated {
                offset: self.group[0].offset,
            });
        }

        if self.group.len() == 1 {
            return Err(Error::Truncated {
                offset: self.group[0].offset,
            });
        }

        let mut output = Vec::new();
        if self.group.len() == 2 {
            if self.options.strict && self.group[1].value.unwrap() & 0x0f != 0 {
                return Err(Error::NonCanonical {
                    offset: self.group[1].offset,
                });
            }
            output.push(
                (self.group[0].value.unwrap() << 2)
                    | (self.group[1].value.unwrap() >> 4),
            );
        } else if self.group.len() == 3 {
            if self.options.strict && self.group[2].value.unwrap() & 0x03 != 0 {
                return Err(Error::NonCanonical {
                    offset: self.group[2].offset,
                });
            }
            output.push(
                (self.group[0].value.unwrap() << 2)
                    | (self.group[1].value.unwrap() >> 4),
            );
            output.push(
                (self.group[1].value.unwrap() << 4)
                    | (self.group[2].value.unwrap() >> 2),
            );
        }

        Ok(output)
    }

    pub fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky {
            return Err(error.clone());
        }
        if self.closed {
            return Err(Error::AlreadyFinished);
        }

        let mut output = Vec::new();
        for &byte in input {
            let offset = self.offset;
            self.offset += 1;
            if let Err(error) = self.scan_byte(byte, offset, &mut output) {
                return Err(self.fail(error));
            }
        }

        self.emitted.extend_from_slice(&output);
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky {
            return Err(error.clone());
        }
        if self.closed {
            return Err(Error::AlreadyFinished);
        }

        match self.finish_inner() {
            Ok(output) => {
                self.closed = true;
                Ok(output)
            }
            Err(error) => Err(self.fail(error)),
        }
    }
}

pub fn decode(input: &[u8], options: Options) -> Result<Vec<u8>, Error> {
    let mut decoder = Decoder::new(options)?;
    let mut output = decoder.push(input)?;
    output.extend_from_slice(&decoder.finish()?);
    Ok(output)
}