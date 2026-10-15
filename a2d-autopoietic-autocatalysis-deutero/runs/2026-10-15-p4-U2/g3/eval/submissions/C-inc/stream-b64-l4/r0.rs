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

fn lookup_digit(byte: u8, alphabet: Alphabet, strict: bool, offset: usize) -> Result<u8, Error> {
    let value = match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' if alphabet == Alphabet::Standard => Some(62),
        b'/' if alphabet == Alphabet::Standard => Some(63),
        b'-' if alphabet == Alphabet::UrlSafe => Some(62),
        b'_' if alphabet == Alphabet::UrlSafe => Some(63),
        _ => None,
    };
    if let Some(value) = value {
        return Ok(value);
    }

    if strict {
        let foreign = match alphabet {
            Alphabet::Standard => byte == b'-' || byte == b'_',
            Alphabet::UrlSafe => byte == b'+' || byte == b'/',
        };
        if foreign {
            return Err(Error::MixedAlphabet { offset });
        }
    }
    Err(Error::InvalidByte { offset, byte })
}

fn is_ignored(byte: u8) -> bool {
    matches!(byte, 9 | 10 | 11 | 12 | 13 | 32)
}

struct SymbolWriter {
    whitespace: Whitespace,
    symbols_in_row: usize,
    output: String,
}

impl SymbolWriter {
    fn new(whitespace: Whitespace) -> Self {
        Self {
            whitespace,
            symbols_in_row: 0,
            output: String::new(),
        }
    }

    fn symbol(&mut self, byte: u8) {
        if let Whitespace::Wrapped { width, ending } = self.whitespace {
            if self.symbols_in_row == width {
                match ending {
                    LineEnding::Lf => self.output.push('\n'),
                    LineEnding::CrLf => self.output.push_str("\r\n"),
                }
                self.symbols_in_row = 0;
            }
            self.symbols_in_row += 1;
        }
        self.output.push(byte as char);
    }

    fn take(&mut self) -> String {
        std::mem::take(&mut self.output)
    }
}

pub struct Encoder {
    options: Options,
    tail: Vec<u8>,
    writer: SymbolWriter,
    closed: bool,
}

impl Encoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate_options(options)?;
        Ok(Self {
            options,
            tail: Vec::with_capacity(2),
            writer: SymbolWriter::new(options.whitespace),
            closed: false,
        })
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }

        let mut data = Vec::with_capacity(self.tail.len() + input.len());
        data.extend_from_slice(&self.tail);
        data.extend_from_slice(input);
        self.tail.clear();

        let mut chunks = data.chunks_exact(3);
        for triple in &mut chunks {
            self.emit_triple(triple[0], triple[1], triple[2]);
        }
        self.tail.extend_from_slice(chunks.remainder());
        Ok(self.writer.take())
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        self.closed = true;

        match self.tail.len() {
            0 => {}
            1 => {
                let a = self.tail[0];
                self.writer.symbol(encode_digit(a >> 2, self.options.alphabet));
                self.writer.symbol(encode_digit((a & 0x03) << 4, self.options.alphabet));
                if self.options.padding != Padding::Forbidden {
                    self.writer.symbol(b'=');
                    self.writer.symbol(b'=');
                }
            }
            2 => {
                let a = self.tail[0];
                let b = self.tail[1];
                self.writer.symbol(encode_digit(a >> 2, self.options.alphabet));
                self.writer.symbol(
                    encode_digit(((a & 0x03) << 4) | (b >> 4), self.options.alphabet),
                );
                self.writer.symbol(encode_digit((b & 0x0f) << 2, self.options.alphabet));
                if self.options.padding != Padding::Forbidden {
                    self.writer.symbol(b'=');
                }
            }
            _ => unreachable!(),
        }
        self.tail.clear();
        Ok(self.writer.take())
    }

    fn emit_triple(&mut self, a: u8, b: u8, c: u8) {
        self.writer.symbol(encode_digit(a >> 2, self.options.alphabet));
        self.writer.symbol(
            encode_digit(((a & 0x03) << 4) | (b >> 4), self.options.alphabet),
        );
        self.writer.symbol(
            encode_digit(((b & 0x0f) << 2) | (c >> 6), self.options.alphabet),
        );
        self.writer.symbol(encode_digit(c & 0x3f, self.options.alphabet));
    }
}

pub struct Decoder {
    options: Options,
    offset: usize,
    symbols_seen: usize,
    group: Vec<(u8, usize, u8)>,
    pending_second_pad: Option<usize>,
    padded_done: bool,
    rows_symbols: usize,
    wrap_state: WrapState,
    sticky: Option<Error>,
    closed: bool,
}

#[derive(Clone, Copy)]
enum WrapState {
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
            padded_done: false,
            rows_symbols: 0,
            wrap_state: WrapState::Normal,
            sticky: None,
            closed: false,
        })
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
            let current = self.offset;
            self.offset += 1;
            if let Err(error) = self.process_byte(byte, current, &mut output) {
                self.sticky = Some(error.clone());
                return Err(error);
            }
        }
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky {
            return Err(error.clone());
        }
        if self.closed {
            return Err(Error::AlreadyFinished);
        }

        let result = self.finish_inner();
        match result {
            Ok(output) => {
                self.closed = true;
                Ok(output)
            }
            Err(error) => {
                self.sticky = Some(error.clone());
                Err(error)
            }
        }
    }

    fn process_byte(
        &mut self,
        byte: u8,
        offset: usize,
        output: &mut Vec<u8>,
    ) -> Result<(), Error> {
        match self.options.whitespace {
            Whitespace::IgnoreAscii if is_ignored(byte) => return Ok(()),
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
                        if is_ignored(byte) {
                            return Err(Error::BadWrap { offset });
                        }
                        self.wrap_state = WrapState::Normal;
                        self.rows_symbols = 0;
                    }
                    WrapState::Normal => {
                        if self.rows_symbols == width {
                            match ending {
                                LineEnding::Lf => {
                                    if byte != b'\n' {
                                        return Err(Error::BadWrap { offset });
                                    }
                                    self.wrap_state = WrapState::NeedSymbol {
                                        separator_offset: offset,
                                    };
                                    return Ok(());
                                }
                                LineEnding::CrLf => {
                                    if byte != b'\r' {
                                        return Err(Error::BadWrap { offset });
                                    }
                                    self.wrap_state = WrapState::NeedLf {
                                        cr_offset: offset,
                                    };
                                    return Ok(());
                                }
                            }
                        } else if is_ignored(byte) {
                            return Err(Error::BadWrap { offset });
                        }
                    }
                }
            }
            _ => {}
        }

        if self.options.max_symbols.is_some_and(|limit| self.symbols_seen >= limit) {
            return Err(Error::Overlong {
                offset,
                limit: self.options.max_symbols.unwrap(),
            });
        }
        self.symbols_seen += 1;

        if matches!(self.options.whitespace, Whitespace::Wrapped { .. }) {
            self.rows_symbols += 1;
        }

        if self.padded_done {
            return Err(Error::BadPadding { offset });
        }
        if let Some(pending_offset) = self.pending_second_pad {
            if byte != b'=' {
                return Err(Error::BadPadding { offset });
            }
            self.pending_second_pad = None;
            let first = self.group[0];
            let second = self.group[1];
            let one = (first.2 << 2) | (second.2 >> 4);
            if self.options.strict && (second.2 & 0x0f) != 0 {
                return Err(Error::NonCanonical { offset: second.1 });
            }
            output.push(one);
            self.group.clear();
            self.padded_done = true;
            let _ = pending_offset;
            return Ok(());
        }

        let position = self.group.len();
        if byte == b'=' {
            if self.options.padding == Padding::Forbidden {
                return Err(Error::BadPadding { offset });
            }
            match position {
                0 | 1 => return Err(Error::BadPadding { offset }),
                2 => {
                    self.pending_second_pad = Some(offset);
                    self.group.push((byte, offset, 0));
                    return Ok(());
                }
                3 => {
                    let first = self.group[0];
                    let second = self.group[1];
                    let third = self.group[2];
                    let two = (first.2 << 2) | (second.2 >> 4);
                    let three = (second.2 << 4) | (third.2 >> 2);
                    if self.options.strict && (third.2 & 0x03) != 0 {
                        return Err(Error::NonCanonical { offset: third.1 });
                    }
                    output.push(two);
                    output.push(three);
                    self.group.clear();
                    self.padded_done = true;
                    return Ok(());
                }
                _ => unreachable!(),
            }
        }

        let digit = lookup_digit(byte, self.options.alphabet, self.options.strict, offset)?;
        self.group.push((byte, offset, digit));
        if self.group.len() == 4 {
            let a = self.group[0].2;
            let b = self.group[1].2;
            let c = self.group[2].2;
            let d = self.group[3].2;
            output.push((a << 2) | (b >> 4));
            output.push((b << 4) | (c >> 2));
            output.push((c << 6) | d);
            self.group.clear();
        }
        Ok(())
    }

    fn finish_inner(&mut self) -> Result<Vec<u8>, Error> {
        let mut output = Vec::new();
        match self.wrap_state {
            WrapState::NeedLf { cr_offset } => {
                return Err(Error::BadWrap { offset: cr_offset });
            }
            WrapState::NeedSymbol { separator_offset } => {
                return Err(Error::BadWrap {
                    offset: separator_offset,
                });
            }
            WrapState::Normal => {}
        }

        if self.pending_second_pad.is_some() {
            return Err(Error::BadPadding {
                offset: self.pending_second_pad.unwrap(),
            });
        }
        if self.padded_done || self.group.is_empty() {
            return Ok(output);
        }

        let count = self.group.len();
        if count == 1 {
            return Err(Error::Truncated {
                offset: self.group[0].1,
            });
        }

        if self.options.padding == Padding::Required {
            return Err(Error::Truncated {
                offset: self.group[0].1,
            });
        }

        if count == 2 {
            if self.options.strict && (self.group[1].2 & 0x0f) != 0 {
                return Err(Error::NonCanonical {
                    offset: self.group[1].1,
                });
            }
            output.push((self.group[0].2 << 2) | (self.group[1].2 >> 4));
        } else if count == 3 {
            if self.options.strict && (self.group[2].2 & 0x03) != 0 {
                return Err(Error::NonCanonical {
                    offset: self.group[2].1,
                });
            }
            output.push((self.group[0].2 << 2) | (self.group[1].2 >> 4));
            output.push((self.group[1].2 << 4) | (self.group[2].2 >> 2));
        }
        Ok(output)
    }
}

pub fn encode(input: &[u8], options: Options) -> Result<String, Error> {
    validate_options(options)?;
    let mut encoder = Encoder::new(options)?;
    let mut output = encoder.push(input)?;
    output.push_str(&encoder.finish()?);
    Ok(output)
}

pub fn decode(input: &[u8], options: Options) -> Result<Vec<u8>, Error> {
    validate_options(options)?;
    let mut decoder = Decoder::new(options)?;
    let mut output = decoder.push(input)?;
    output.extend(decoder.finish()?);
    Ok(output)
}

fn main() {}