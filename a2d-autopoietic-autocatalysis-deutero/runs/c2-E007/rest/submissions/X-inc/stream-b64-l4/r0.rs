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

fn opposite_symbol(byte: u8, alphabet: Alphabet) -> bool {
    match alphabet {
        Alphabet::Standard => byte == b'-' || byte == b'_',
        Alphabet::UrlSafe => byte == b'+' || byte == b'/',
    }
}

fn symbol(value: u8, alphabet: Alphabet) -> u8 {
    match value {
        0..=25 => b'A' + value,
        26..=51 => b'a' + value - 26,
        52..=61 => b'0' + value - 52,
        62 if alphabet == Alphabet::Standard => b'+',
        63 if alphabet == Alphabet::Standard => b'/',
        62 => b'-',
        _ => b'_',
    }
}

fn wrap_valid(options: Options) -> Option<(usize, LineEnding)> {
    match options.whitespace {
        Whitespace::Wrapped { width, ending } => Some((width, ending)),
        _ => None,
    }
}

fn append_encoded_symbols(
    out: &mut String,
    symbols: &[u8],
    options: Options,
    row_symbols: &mut usize,
    emitted_any: &mut bool,
) {
    let wrapping = wrap_valid(options);
    for &s in symbols {
        if let Some((width, ending)) = wrapping {
            if *row_symbols == width {
                match ending {
                    LineEnding::Lf => out.push('\n'),
                    LineEnding::CrLf => out.push_str("\r\n"),
                }
                *row_symbols = 0;
            }
            *row_symbols += 1;
        }
        *emitted_any = true;
        out.push(s as char);
    }
}

fn encode_triple(bytes: &[u8], options: Options) -> Vec<u8> {
    let b0 = bytes[0];
    let b1 = if bytes.len() > 1 { bytes[1] } else { 0 };
    let b2 = if bytes.len() > 2 { bytes[2] } else { 0 };
    let mut result = vec![
        symbol(b0 >> 2, options.alphabet),
        symbol(((b0 & 0x03) << 4) | (b1 >> 4), options.alphabet),
    ];
    if bytes.len() > 1 {
        result.push(symbol(((b1 & 0x0f) << 2) | (b2 >> 6), options.alphabet));
    } else if options.padding != Padding::Forbidden {
        result.extend_from_slice(b"==");
        return result;
    }
    if bytes.len() > 2 {
        result.push(symbol(b2 & 0x3f, options.alphabet));
    } else if options.padding != Padding::Forbidden {
        result.push(b'=');
    }
    result
}

pub fn encode(input: &[u8], options: Options) -> Result<String, Error> {
    validate(options)?;
    let mut encoder = Encoder::new(options)?;
    let mut result = encoder.push(input)?;
    result.push_str(&encoder.finish()?);
    Ok(result)
}

pub struct Encoder {
    options: Options,
    pending: Vec<u8>,
    closed: bool,
    row_symbols: usize,
    emitted_any: bool,
}

impl Encoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            pending: Vec::new(),
            closed: false,
            row_symbols: 0,
            emitted_any: false,
        })
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        let mut combined = std::mem::take(&mut self.pending);
        combined.extend_from_slice(input);
        let complete = combined.len() / 3 * 3;
        self.pending.extend_from_slice(&combined[complete..]);

        let mut out = String::new();
        for chunk in combined[..complete].chunks_exact(3) {
            let symbols = encode_triple(chunk, self.options);
            append_encoded_symbols(
                &mut out,
                &symbols,
                self.options,
                &mut self.row_symbols,
                &mut self.emitted_any,
            );
        }
        Ok(out)
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        self.closed = true;
        let mut out = String::new();
        if !self.pending.is_empty() {
            let symbols = encode_triple(&self.pending, self.options);
            append_encoded_symbols(
                &mut out,
                &symbols,
                self.options,
                &mut self.row_symbols,
                &mut self.emitted_any,
            );
            self.pending.clear();
        }
        Ok(out)
    }
}

#[derive(Clone, Copy)]
enum FrameState {
    Normal,
    NeedLf { cr_offset: usize },
    NeedSymbol { separator_offset: usize },
}

pub struct Decoder {
    options: Options,
    offset: usize,
    symbol_count: usize,
    group: Vec<(u8, usize)>,
    terminal_padding: bool,
    pending_second_padding: Option<usize>,
    frame_state: FrameState,
    row_symbols: usize,
    closed: bool,
    sticky: Option<Error>,
}

impl Decoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            offset: 0,
            symbol_count: 0,
            group: Vec::with_capacity(4),
            terminal_padding: false,
            pending_second_padding: None,
            frame_state: FrameState::Normal,
            row_symbols: 0,
            closed: false,
            sticky: None,
        })
    }

    fn fail(&mut self, error: Error) -> Error {
        self.sticky = Some(error.clone());
        error
    }

    fn scan_byte(&mut self, byte: u8, offset: usize, output: &mut Vec<u8>) -> Result<(), Error> {
        match self.options.whitespace {
            Whitespace::IgnoreAscii if matches!(byte, 9 | 10 | 11 | 12 | 13 | 32) => {
                return Ok(());
            }
            Whitespace::Wrapped { width, ending } => {
                match self.frame_state {
                    FrameState::NeedLf { cr_offset } => {
                        if byte != b'\n' {
                            return Err(Error::BadWrap { offset });
                        }
                        self.frame_state = FrameState::NeedSymbol {
                            separator_offset: cr_offset,
                        };
                        return Ok(());
                    }
                    FrameState::NeedSymbol { .. } => {
                        if matches!(byte, b'\r' | b'\n' | 9 | 11 | 12 | 32) {
                            return Err(Error::BadWrap { offset });
                        }
                        self.frame_state = FrameState::Normal;
                        self.row_symbols = 0;
                    }
                    FrameState::Normal => {
                        if self.row_symbols == width {
                            match ending {
                                LineEnding::Lf if byte == b'\n' => {
                                    self.frame_state = FrameState::NeedSymbol {
                                        separator_offset: offset,
                                    };
                                    return Ok(());
                                }
                                LineEnding::CrLf if byte == b'\r' => {
                                    self.frame_state = FrameState::NeedLf { cr_offset: offset };
                                    return Ok(());
                                }
                                _ => return Err(Error::BadWrap { offset }),
                            }
                        } else if matches!(byte, b'\r' | b'\n' | 9 | 11 | 12 | 32) {
                            return Err(Error::BadWrap { offset });
                        }
                    }
                }
            }
            _ => {}
        }

        if let Some(limit) = self.options.max_symbols {
            if self.symbol_count >= limit {
                return Err(Error::Overlong { offset, limit });
            }
        }
        self.symbol_count += 1;
        if matches!(self.options.whitespace, Whitespace::Wrapped { .. }) {
            self.row_symbols += 1;
        }

        if self.terminal_padding {
            return Err(Error::BadPadding { offset });
        }
        if let Some(padding_offset) = self.pending_second_padding {
            if byte != b'=' {
                return Err(Error::BadPadding { offset });
            }
            self.pending_second_padding = None;
            self.group.push((byte, offset));
            self.complete_group(output)?;
            self.terminal_padding = true;
            let _ = padding_offset;
            return Ok(());
        }

        let pos = self.group.len();
        if byte == b'=' {
            if self.options.padding == Padding::Forbidden {
                return Err(Error::BadPadding { offset });
            }
            match pos {
                0 | 1 => return Err(Error::BadPadding { offset }),
                2 => {
                    self.pending_second_padding = Some(offset);
                    self.group.push((byte, offset));
                    return Ok(());
                }
                3 => {
                    self.group.push((byte, offset));
                    self.complete_group(output)?;
                    self.terminal_padding = true;
                    return Ok(());
                }
                _ => unreachable!(),
            }
        }

        if let Some(value) = digit(byte, self.options.alphabet) {
            self.group.push((value, offset));
            if self.group.len() == 4 {
                self.complete_group(output)?;
            }
            return Ok(());
        }
        if self.options.strict && opposite_symbol(byte, self.options.alphabet) {
            return Err(Error::MixedAlphabet { offset });
        }
        Err(Error::InvalidByte { offset, byte })
    }

    fn complete_group(&mut self, output: &mut Vec<u8>) -> Result<(), Error> {
        let len = self.group.len();
        if len != 4 {
            return Ok(());
        }

        let a = self.group[0].0;
        let b = self.group[1].0;
        let c = self.group[2].0;
        let d = self.group[3].0;

        if self.group[2].0 == b'=' {
            if self.options.strict && b & 0x0f != 0 {
                return Err(Error::NonCanonical {
                    offset: self.group[1].1,
                });
            }
            output.push((a << 2) | (b >> 4));
        } else if self.group[3].0 == b'=' {
            if self.options.strict && c & 0x03 != 0 {
                return Err(Error::NonCanonical {
                    offset: self.group[2].1,
                });
            }
            output.push((a << 2) | (b >> 4));
            output.push((b << 4) | (c >> 2));
        } else {
            output.push((a << 2) | (b >> 4));
            output.push((b << 4) | (c >> 2));
            output.push((c << 6) | d);
        }
        self.group.clear();
        Ok(())
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
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky {
            return Err(error.clone());
        }
        if self.closed {
            return Err(Error::AlreadyFinished);
        }

        let mut output = Vec::new();
        if let FrameState::NeedLf { cr_offset } = self.frame_state {
            return Err(self.fail(Error::BadWrap { offset: cr_offset }));
        }
        if let FrameState::NeedSymbol { separator_offset } = self.frame_state {
            return Err(self.fail(Error::BadWrap {
                offset: separator_offset,
            }));
        }

        if let Some(offset) = self.pending_second_padding {
            return Err(self.fail(Error::BadPadding { offset }));
        }
        if !self.group.is_empty() {
            match self.group.len() {
                1 => return Err(self.fail(Error::Truncated {
                    offset: self.group[0].1,
                })),
                2 | 3 if self.options.padding != Padding::Required => {
                    let a = self.group[0].0;
                    let b = self.group[1].0;
                    if self.options.strict && self.group.len() == 2 && b & 0x0f != 0 {
                        return Err(self.fail(Error::NonCanonical {
                            offset: self.group[1].1,
                        }));
                    }
                    output.push((a << 2) | (b >> 4));
                    if self.group.len() == 3 {
                        let c = self.group[2].0;
                        if self.options.strict && c & 0x03 != 0 {
                            return Err(self.fail(Error::NonCanonical {
                                offset: self.group[2].1,
                            }));
                        }
                        output.push((b << 4) | (c >> 2));
                    }
                }
                _ => return Err(self.fail(Error::Truncated {
                    offset: self.group[0].1,
                })),
            }
        }

        self.group.clear();
        self.closed = true;
        Ok(output)
    }
}

pub fn decode(input: &[u8], options: Options) -> Result<Vec<u8>, Error> {
    validate(options)?;
    let mut decoder = Decoder::new(options)?;
    let mut result = decoder.push(input)?;
    result.extend(decoder.finish()?);
    Ok(result)
}