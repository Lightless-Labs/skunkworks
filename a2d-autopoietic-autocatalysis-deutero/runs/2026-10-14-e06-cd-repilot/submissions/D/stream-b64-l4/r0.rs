use std::fmt;

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

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Error {}

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

fn encode_symbols(input: &[u8], alphabet: Alphabet, padding: Padding) -> Vec<u8> {
    let table = alphabet_bytes(alphabet);
    let mut out = Vec::with_capacity(input.len().div_ceil(3) * 4);

    for chunk in input.chunks(3) {
        let a = chunk[0];
        let b = *chunk.get(1).unwrap_or(&0);
        let c = *chunk.get(2).unwrap_or(&0);

        out.push(table[(a >> 2) as usize]);
        out.push(table[(((a & 0x03) << 4) | (b >> 4)) as usize]);

        if chunk.len() >= 2 {
            out.push(table[(((b & 0x0f) << 2) | (c >> 6)) as usize]);
        } else if padding != Padding::Forbidden {
            out.push(b'=');
        }

        if chunk.len() == 3 {
            out.push(table[(c & 0x3f) as usize]);
        } else if padding != Padding::Forbidden {
            out.push(b'=');
        }
    }

    out
}

fn wrap_complete_symbols(raw: &[u8], whitespace: Whitespace) -> String {
    let mut out = Vec::with_capacity(raw.len() + raw.len() / 4);

    if let Whitespace::Wrapped { width, ending } = whitespace {
        for (i, &byte) in raw.iter().enumerate() {
            if i != 0 && i % width == 0 {
                match ending {
                    LineEnding::Lf => out.push(b'\n'),
                    LineEnding::CrLf => out.extend_from_slice(b"\r\n"),
                }
            }
            out.push(byte);
        }
    } else {
        out.extend_from_slice(raw);
    }

    String::from_utf8(out).expect("encoded symbols are ASCII")
}

pub fn encode(input: &[u8], options: Options) -> Result<String, Error> {
    validate_options(options)?;
    Ok(wrap_complete_symbols(
        &encode_symbols(input, options.alphabet, options.padding),
        options.whitespace,
    ))
}

pub struct Encoder {
    options: Options,
    pending: Vec<u8>,
    column: usize,
    separator_pending: bool,
    finished: bool,
}

impl Encoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate_options(options)?;
        Ok(Self {
            options,
            pending: Vec::with_capacity(2),
            column: 0,
            separator_pending: false,
            finished: false,
        })
    }

    fn append_symbols(&mut self, symbols: &[u8], out: &mut String) {
        let wrapped = match self.options.whitespace {
            Whitespace::Wrapped { width, ending } => Some((width, ending)),
            _ => None,
        };

        for &symbol in symbols {
            if let Some((width, ending)) = wrapped {
                if self.separator_pending {
                    match ending {
                        LineEnding::Lf => out.push('\n'),
                        LineEnding::CrLf => out.push_str("\r\n"),
                    }
                    self.separator_pending = false;
                }

                out.push(symbol as char);
                self.column += 1;
                if self.column == width {
                    self.column = 0;
                    self.separator_pending = true;
                }
            } else {
                out.push(symbol as char);
            }
        }
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }

        let mut combined = Vec::with_capacity(self.pending.len() + input.len());
        combined.extend_from_slice(&self.pending);
        combined.extend_from_slice(input);

        let complete_len = combined.len() / 3 * 3;
        self.pending.clear();
        self.pending.extend_from_slice(&combined[complete_len..]);

        let symbols = encode_symbols(
            &combined[..complete_len],
            self.options.alphabet,
            Padding::Forbidden,
        );

        let mut out = String::new();
        self.append_symbols(&symbols, &mut out);
        Ok(out)
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }

        let symbols = encode_symbols(
            &self.pending,
            self.options.alphabet,
            self.options.padding,
        );
        self.pending.clear();

        let mut out = String::new();
        self.append_symbols(&symbols, &mut out);
        self.separator_pending = false;
        self.finished = true;
        Ok(out)
    }
}

#[derive(Clone, Copy)]
enum FrameState {
    Row,
    NeedLf { cr_offset: usize },
    NeedSymbol { sep_offset: usize },
}

#[derive(Clone, Copy)]
enum DecodeState {
    Open,
    PendingSecondPad { offset: usize },
    PaddedComplete,
}

pub struct Decoder {
    options: Options,
    offset: usize,
    symbol_count: usize,
    group: Vec<(u8, usize)>,
    state: DecodeState,
    frame: FrameState,
    row_column: usize,
    finished: bool,
    sticky: Option<Error>,
}

impl Decoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate_options(options)?;
        Ok(Self {
            options,
            offset: 0,
            symbol_count: 0,
            group: Vec::with_capacity(4),
            state: DecodeState::Open,
            frame: FrameState::Row,
            row_column: 0,
            finished: false,
            sticky: None,
        })
    }

    fn is_ignored_ascii(byte: u8) -> bool {
        matches!(byte, 9 | 10 | 11 | 12 | 13 | 32)
    }

    fn is_other_alphabet_symbol(&self, byte: u8) -> bool {
        match self.options.alphabet {
            Alphabet::Standard => matches!(byte, b'-' | b'_'),
            Alphabet::UrlSafe => matches!(byte, b'+' | b'/'),
        }
    }

    fn lookup(&self, byte: u8, offset: usize) -> Result<u8, Error> {
        if let Some(value) = alphabet_bytes(self.options.alphabet)
            .iter()
            .position(|&candidate| candidate == byte)
        {
            return Ok(value as u8);
        }

        if self.options.strict && self.is_other_alphabet_symbol(byte) {
            return Err(Error::MixedAlphabet { offset });
        }

        Err(Error::InvalidByte { offset, byte })
    }

    fn process_symbol(
        &mut self,
        byte: u8,
        offset: usize,
        output: &mut Vec<u8>,
    ) -> Result<(), Error> {
        if let Some(limit) = self.options.max_symbols {
            if self.symbol_count >= limit {
                return Err(Error::Overlong { offset, limit });
            }
        }
        self.symbol_count += 1;

        if matches!(self.state, DecodeState::PaddedComplete) {
            return Err(Error::BadPadding { offset });
        }

        if matches!(self.state, DecodeState::PendingSecondPad { .. }) {
            if byte != b'=' {
                return Err(Error::BadPadding { offset });
            }

            let first = self.group[0].0;
            let second = self.group[1].0;
            let second_offset = self.group[1].1;
            let pad_offset = self.group[2].1;

            if self.options.padding == Padding::Forbidden {
                return Err(Error::BadPadding { offset: pad_offset });
            }
            if self.options.strict && second & 0x0f != 0 {
                return Err(Error::NonCanonical {
                    offset: second_offset,
                });
            }

            output.push((first << 2) | (second >> 4));
            self.group.clear();
            self.state = DecodeState::PaddedComplete;
            return Ok(());
        }

        let pos = self.group.len();
        if byte == b'=' {
            match pos {
                0 | 1 => return Err(Error::BadPadding { offset }),
                2 => {
                    self.group.push((b'=', offset));
                    self.state = DecodeState::PendingSecondPad { offset };
                    return Ok(());
                }
                3 => {
                    if self.options.padding == Padding::Forbidden {
                        return Err(Error::BadPadding { offset });
                    }

                    let a = self.group[0].0;
                    let b = self.group[1].0;
                    let c = self.group[2].0;
                    let c_offset = self.group[2].1;

                    if self.options.strict && c & 0x03 != 0 {
                        return Err(Error::NonCanonical { offset: c_offset });
                    }

                    output.push((a << 2) | (b >> 4));
                    output.push((b << 4) | (c >> 2));
                    self.group.clear();
                    self.state = DecodeState::PaddedComplete;
                    return Ok(());
                }
                _ => unreachable!(),
            }
        }

        let value = self.lookup(byte, offset)?;
        self.group.push((value, offset));

        if self.group.len() == 4 {
            let a = self.group[0].0;
            let b = self.group[1].0;
            let c = self.group[2].0;
            let d = self.group[3].0;
            output.push((a << 2) | (b >> 4));
            output.push((b << 4) | (c >> 2));
            output.push((c << 6) | d);
            self.group.clear();
        }

        Ok(())
    }

    fn scan_byte(&mut self, byte: u8, offset: usize, output: &mut Vec<u8>) -> Result<(), Error> {
        match self.options.whitespace {
            Whitespace::None => self.process_symbol(byte, offset, output),
            Whitespace::IgnoreAscii => {
                if Self::is_ignored_ascii(byte) {
                    Ok(())
                } else {
                    self.process_symbol(byte, offset, output)
                }
            }
            Whitespace::Wrapped { width, ending } => {
                match self.frame {
                    FrameState::NeedLf { cr_offset } => {
                        if byte != b'\n' {
                            return Err(Error::BadWrap { offset });
                        }
                        self.frame = FrameState::NeedSymbol {
                            sep_offset: cr_offset,
                        };
                        Ok(())
                    }
                    FrameState::NeedSymbol { sep_offset } => {
                        if matches!(byte, b'\r' | b'\n') {
                            return Err(Error::BadWrap { offset });
                        }
                        self.frame = FrameState::Row;
                        self.row_column = 0;

                        if matches!(self.state, DecodeState::PaddedComplete) {
                            return Err(Error::BadWrap { offset });
                        }

                        let _ = sep_offset;
                        self.process_symbol(byte, offset, output)
                    }
                    FrameState::Row => {
                        if self.row_column < width {
                            if matches!(byte, b'\r' | b'\n') {
                                return Err(Error::BadWrap { offset });
                            }
                            self.row_column += 1;
                            self.process_symbol(byte, offset, output)
                        } else {
                            match ending {
                                LineEnding::Lf if byte == b'\n' => {
                                    self.frame = FrameState::NeedSymbol {
                                        sep_offset: offset,
                                    };
                                    Ok(())
                                }
                                LineEnding::CrLf if byte == b'\r' => {
                                    self.frame = FrameState::NeedLf { cr_offset: offset };
                                    Ok(())
                                }
                                _ => Err(Error::BadWrap { offset }),
                            }
                        }
                    }
                }
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

            if let Err(error) = self.scan_byte(byte, offset, &mut output) {
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
        if self.finished {
            return Err(Error::AlreadyFinished);
        }

        if let FrameState::NeedLf { cr_offset } = self.frame {
            let error = Error::BadWrap { offset: cr_offset };
            self.sticky = Some(error.clone());
            return Err(error);
        }
        if let FrameState::NeedSymbol { sep_offset } = self.frame {
            let error = Error::BadWrap { offset: sep_offset };
            self.sticky = Some(error.clone());
            return Err(error);
        }

        if let DecodeState::PendingSecondPad { offset } = self.state {
            let error = Error::BadPadding { offset };
            self.sticky = Some(error.clone());
            return Err(error);
        }

        let mut output = Vec::new();

        if matches!(self.state, DecodeState::Open) {
            match self.group.len() {
                0 => {}
                1 => {
                    let error = Error::Truncated {
                        offset: self.group[0].1,
                    };
                    self.sticky = Some(error.clone());
                    return Err(error);
                }
                2 | 3 if self.options.padding != Padding::Required => {
                    if self.options.strict {
                        let (value, offset) = self.group[self.group.len() - 1];
                        let mask = if self.group.len() == 2 { 0x0f } else { 0x03 };
                        if value & mask != 0 {
                            let error = Error::NonCanonical { offset };
                            self.sticky = Some(error.clone());
                            return Err(error);
                        }
                    }

                    let a = self.group[0].0;
                    let b = self.group[1].0;
                    output.push((a << 2) | (b >> 4));

                    if self.group.len() == 3 {
                        let c = self.group[2].0;
                        output.push((b << 4) | (c >> 2));
                    }
                }
                _ => {
                    let error = Error::Truncated {
                        offset: self.group[0].1,
                    };
                    self.sticky = Some(error.clone());
                    return Err(error);
                }
            }
        }

        self.finished = true;
        Ok(output)
    }
}

pub fn decode(input: &[u8], options: Options) -> Result<Vec<u8>, Error> {
    let mut decoder = Decoder::new(options)?;
    let mut output = decoder.push(input)?;
    output.extend(decoder.finish()?);
    Ok(output)
}