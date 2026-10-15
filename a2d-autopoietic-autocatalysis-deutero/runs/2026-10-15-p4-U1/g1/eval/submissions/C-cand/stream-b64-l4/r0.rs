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

fn alphabet_char(alphabet: Alphabet, value: u8) -> u8 {
    const BASE: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    match (alphabet, value) {
        (Alphabet::UrlSafe, 62) => b'-',
        (Alphabet::UrlSafe, 63) => b'_',
        _ => BASE[value as usize],
    }
}

fn encode_raw(input: &[u8], alphabet: Alphabet, padding: Padding) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len().saturating_add(2) / 3 * 4);
    let mut chunks = input.chunks_exact(3);
    for chunk in &mut chunks {
        let a = chunk[0];
        let b = chunk[1];
        let c = chunk[2];
        out.push(alphabet_char(alphabet, a >> 2));
        out.push(alphabet_char(alphabet, ((a & 0x03) << 4) | (b >> 4)));
        out.push(alphabet_char(alphabet, ((b & 0x0f) << 2) | (c >> 6)));
        out.push(alphabet_char(alphabet, c & 0x3f));
    }
    let tail = chunks.remainder();
    if !tail.is_empty() {
        let a = tail[0];
        out.push(alphabet_char(alphabet, a >> 2));
        if tail.len() == 1 {
            out.push(alphabet_char(alphabet, (a & 0x03) << 4));
            if padding != Padding::Forbidden {
                out.extend_from_slice(b"==");
            }
        } else {
            let b = tail[1];
            out.push(alphabet_char(alphabet, ((a & 0x03) << 4) | (b >> 4)));
            out.push(alphabet_char(alphabet, (b & 0x0f) << 2));
            if padding != Padding::Forbidden {
                out.push(b'=');
            }
        }
    }
    out
}

fn wrap_encoded(raw: &[u8], whitespace: Whitespace) -> Vec<u8> {
    let Whitespace::Wrapped { width, ending } = whitespace else {
        return raw.to_vec();
    };
    let separator: &[u8] = match ending {
        LineEnding::Lf => b"\n",
        LineEnding::CrLf => b"\r\n",
    };
    let mut out = Vec::with_capacity(raw.len() + raw.len() / width * separator.len());
    for (i, chunk) in raw.chunks(width).enumerate() {
        if i != 0 {
            out.extend_from_slice(separator);
        }
        out.extend_from_slice(chunk);
    }
    out
}

pub fn encode(input: &[u8], options: Options) -> Result<String, Error> {
    validate_options(options)?;
    let raw = encode_raw(input, options.alphabet, options.padding);
    let bytes = wrap_encoded(&raw, options.whitespace);
    Ok(String::from_utf8(bytes).expect("base64 output is ASCII"))
}

fn decode_digit(alphabet: Alphabet, byte: u8) -> Option<u8> {
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

fn is_foreign_alphabet_char(alphabet: Alphabet, byte: u8) -> bool {
    match alphabet {
        Alphabet::Standard => byte == b'-' || byte == b'_',
        Alphabet::UrlSafe => byte == b'+' || byte == b'/',
    }
}

#[derive(Clone)]
struct Framing {
    whitespace: Whitespace,
    symbols_in_row: usize,
    need_separator: bool,
    separator_start: Option<usize>,
    need_symbol_after_separator: Option<usize>,
}

impl Framing {
    fn new(whitespace: Whitespace) -> Self {
        Self {
            whitespace,
            symbols_in_row: 0,
            need_separator: false,
            separator_start: None,
            need_symbol_after_separator: None,
        }
    }

    fn is_ignored_ascii(byte: u8) -> bool {
        matches!(byte, 9 | 10 | 11 | 12 | 13 | 32)
    }

    // Returns true for a symbol, false for a skipped byte.
    fn consume(&mut self, byte: u8, offset: usize) -> Result<bool, Error> {
        match self.whitespace {
            Whitespace::None => Ok(true),
            Whitespace::IgnoreAscii => Ok(!Self::is_ignored_ascii(byte)),
            Whitespace::Wrapped { width, ending } => {
                if let Some(start) = self.need_symbol_after_separator {
                    if let Some(expected) = self.separator_start {
                        let _ = expected;
                    }
                    if byte == b'\r' || byte == b'\n' {
                        return Err(Error::BadWrap { offset });
                    }
                    self.need_symbol_after_separator = None;
                    self.separator_start = None;
                    self.symbols_in_row = 0;
                    self.need_separator = false;
                    let _ = start;
                    return Ok(true);
                }

                if self.need_separator {
                    match ending {
                        LineEnding::Lf => {
                            if byte != b'\n' {
                                return Err(Error::BadWrap { offset });
                            }
                            self.need_separator = false;
                            self.need_symbol_after_separator = Some(offset);
                            self.separator_start = Some(offset);
                            return Ok(false);
                        }
                        LineEnding::CrLf => {
                            if byte != b'\r' {
                                return Err(Error::BadWrap { offset });
                            }
                            self.need_separator = false;
                            self.separator_start = Some(offset);
                            self.need_symbol_after_separator = Some(usize::MAX);
                            return Ok(false);
                        }
                    }
                }

                if ending == LineEnding::CrLf && self.need_symbol_after_separator == Some(usize::MAX) {
                    if byte != b'\n' {
                        return Err(Error::BadWrap { offset });
                    }
                    self.need_symbol_after_separator = Some(offset);
                    return Ok(false);
                }

                if byte == b'\r' || byte == b'\n' {
                    return Err(Error::BadWrap { offset });
                }

                if self.symbols_in_row == width {
                    self.need_separator = true;
                    return self.consume(byte, offset);
                }

                self.symbols_in_row += 1;
                if self.symbols_in_row == width {
                    self.need_separator = true;
                }
                Ok(true)
            }
        }
    }

    fn finish(&self, physical_len: usize) -> Result<(), Error> {
        match self.whitespace {
            Whitespace::Wrapped { ending, .. } => {
                if ending == LineEnding::CrLf
                    && self.need_symbol_after_separator == Some(usize::MAX)
                {
                    return Err(Error::BadWrap {
                        offset: self.separator_start.unwrap_or(physical_len),
                    });
                }
                if self.need_symbol_after_separator.is_some() {
                    return Err(Error::BadWrap {
                        offset: self.separator_start.unwrap_or(physical_len),
                    });
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

#[derive(Clone)]
struct DecodeState {
    options: Options,
    framing: Framing,
    group: Vec<(u8, usize)>,
    pending_second_pad: Option<usize>,
    terminal_padding: bool,
    symbol_count: usize,
    offset: usize,
    finished: bool,
    sticky: Option<Error>,
}

impl DecodeState {
    fn new(options: Options) -> Self {
        Self {
            options,
            framing: Framing::new(options.whitespace),
            group: Vec::with_capacity(4),
            pending_second_pad: None,
            terminal_padding: false,
            symbol_count: 0,
            offset: 0,
            finished: false,
            sticky: None,
        }
    }

    fn fail<T>(&mut self, error: Error) -> Result<T, Error> {
        self.sticky = Some(error.clone());
        Err(error)
    }

    fn process_symbol(&mut self, byte: u8, offset: usize, output: &mut Vec<u8>) -> Result<(), Error> {
        if let Some(limit) = self.options.max_symbols {
            if self.symbol_count >= limit {
                return Err(Error::Overlong { offset, limit });
            }
        }
        self.symbol_count += 1;

        if self.terminal_padding {
            return Err(Error::BadPadding { offset });
        }
        if let Some(first_pad) = self.pending_second_pad {
            if byte != b'=' {
                return Err(Error::BadPadding { offset });
            }
            if self.options.padding == Padding::Forbidden {
                return Err(Error::BadPadding { offset });
            }
            self.group.push((byte, offset));
            self.pending_second_pad = None;
            self.emit_group(output)?;
            self.terminal_padding = true;
            let _ = first_pad;
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
                    self.group.push((byte, offset));
                    self.pending_second_pad = Some(offset);
                    return Ok(());
                }
                3 => {
                    self.group.push((byte, offset));
                    self.emit_group(output)?;
                    self.terminal_padding = true;
                    return Ok(());
                }
                _ => return Err(Error::BadPadding { offset }),
            }
        }

        let digit = decode_digit(self.options.alphabet, byte).ok_or_else(|| {
            if self.options.strict && is_foreign_alphabet_char(self.options.alphabet, byte) {
                Error::MixedAlphabet { offset }
            } else {
                Error::InvalidByte { offset, byte }
            }
        })?;
        self.group.push((digit, offset));
        if self.group.len() == 4 {
            self.emit_group(output)?;
        }
        Ok(())
    }

    fn emit_group(&mut self, output: &mut Vec<u8>) -> Result<(), Error> {
        if self.group.len() != 4 {
            return Ok(());
        }
        let one_byte_tail = self.group[2].0 == b'=' && self.group[3].0 == b'=';
        let two_byte_tail = self.group[3].0 == b'=' && self.group[2].0 != b'=';
        if (self.group[2].0 == b'=' && !one_byte_tail) || (self.group[3].0 == b'=' && !one_byte_tail && !two_byte_tail) {
            return Err(Error::BadPadding { offset: self.group[3].1 });
        }

        let a = self.group[0].0;
        let b = self.group[1].0;
        let c = self.group[2].0;
        let d = self.group[3].0;

        if one_byte_tail {
            if self.options.strict && (b & 0x0f) != 0 {
                return Err(Error::NonCanonical { offset: self.group[1].1 });
            }
            output.push((a << 2) | (b >> 4));
        } else if two_byte_tail {
            if self.options.strict && (c & 0x03) != 0 {
                return Err(Error::NonCanonical { offset: self.group[2].1 });
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

    fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky {
            return Err(error.clone());
        }
        if self.finished {
            return self.fail(Error::AlreadyFinished);
        }
        let mut output = Vec::new();
        for &byte in input {
            let offset = self.offset;
            self.offset += 1;
            match self.framing.consume(byte, offset) {
                Err(error) => return self.fail(error),
                Ok(false) => continue,
                Ok(true) => {}
            }
            if let Err(error) = self.process_symbol(byte, offset, &mut output) {
                return self.fail(error);
            }
        }
        Ok(output)
    }

    fn finish(&mut self) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky {
            return Err(error.clone());
        }
        if self.finished {
            return self.fail(Error::AlreadyFinished);
        }
        let mut output = Vec::new();
        if let Err(error) = self.framing.finish(self.offset) {
            return self.fail(error);
        }
        if let Some(offset) = self.pending_second_pad {
            return self.fail(Error::BadPadding { offset });
        }

        if !self.group.is_empty() {
            match self.options.padding {
                Padding::Required => {
                    return self.fail(Error::Truncated {
                        offset: self.group[0].1,
                    });
                }
                Padding::Forbidden | Padding::Optional => {
                    if self.group.len() == 1 {
                        return self.fail(Error::Truncated {
                            offset: self.group[0].1,
                        });
                    }
                    if self.group.iter().any(|(v, _)| *v == b'=') {
                        return self.fail(Error::BadPadding {
                            offset: self.group
                                .iter()
                                .find(|(v, _)| *v == b'=')
                                .map(|(_, o)| *o)
                                .unwrap_or(self.group[0].1),
                        });
                    }
                    if self.options.strict {
                        if self.group.len() == 2 && (self.group[1].0 & 0x0f) != 0 {
                            return self.fail(Error::NonCanonical {
                                offset: self.group[1].1,
                            });
                        }
                        if self.group.len() == 3 && (self.group[2].0 & 0x03) != 0 {
                            return self.fail(Error::NonCanonical {
                                offset: self.group[2].1,
                            });
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

pub struct Encoder {
    options: Options,
    pending: Vec<u8>,
    symbols_in_row: usize,
    closed: bool,
}

impl Encoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate_options(options)?;
        Ok(Self {
            options,
            pending: Vec::with_capacity(2),
            symbols_in_row: 0,
            closed: false,
        })
    }

    fn append_symbols(&mut self, symbols: &[u8], output: &mut String) {
        for &symbol in symbols {
            if let Whitespace::Wrapped { width, ending } = self.options.whitespace {
                if self.symbols_in_row == width {
                    match ending {
                        LineEnding::Lf => output.push('\n'),
                        LineEnding::CrLf => output.push_str("\r\n"),
                    }
                    self.symbols_in_row = 0;
                }
                output.push(symbol as char);
                self.symbols_in_row += 1;
            } else {
                output.push(symbol as char);
            }
        }
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        let mut bytes = Vec::with_capacity(self.pending.len() + input.len());
        bytes.extend_from_slice(&self.pending);
        bytes.extend_from_slice(input);
        let complete_len = bytes.len() / 3 * 3;
        let symbols = encode_raw(&bytes[..complete_len], self.options.alphabet, Padding::Forbidden);
        let mut output = String::new();
        self.append_symbols(&symbols, &mut output);
        self.pending.clear();
        self.pending.extend_from_slice(&bytes[complete_len..]);
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        let symbols = encode_raw(&self.pending, self.options.alphabet, self.options.padding);
        let mut output = String::new();
        self.append_symbols(&symbols, &mut output);
        self.pending.clear();
        self.closed = true;
        Ok(output)
    }
}

pub struct Decoder {
    state: DecodeState,
}

impl Decoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate_options(options)?;
        Ok(Self {
            state: DecodeState::new(options),
        })
    }

    pub fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        self.state.push(input)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, Error> {
        self.state.finish()
    }
}

fn main() {}