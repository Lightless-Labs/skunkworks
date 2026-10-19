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

fn alphabet_bytes(alphabet: Alphabet) -> &'static [u8; 64] {
    match alphabet {
        Alphabet::Standard => b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/",
        Alphabet::UrlSafe => b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_",
    }
}

fn encode_digits(bytes: &[u8], alphabet: Alphabet) -> [u8; 4] {
    let table = alphabet_bytes(alphabet);
    let a = bytes[0];
    let b = if bytes.len() > 1 { bytes[1] } else { 0 };
    let c = if bytes.len() > 2 { bytes[2] } else { 0 };

    [
        table[(a >> 2) as usize],
        table[(((a & 0x03) << 4) | (b >> 4)) as usize],
        if bytes.len() > 1 {
            table[(((b & 0x0f) << 2) | (c >> 6)) as usize]
        } else {
            b'='
        },
        if bytes.len() > 2 {
            table[(c & 0x3f) as usize]
        } else {
            b'='
        },
    ]
}

fn encoded_symbol_count(bytes: &[u8], padding: Padding) -> usize {
    let whole = bytes.len() / 3 * 4;
    let tail = match bytes.len() % 3 {
        0 => 0,
        1 => if padding == Padding::Forbidden { 2 } else { 4 },
        _ => if padding == Padding::Forbidden { 3 } else { 4 },
    };
    whole + tail
}

fn append_wrapped_symbol(
    out: &mut String,
    symbol: u8,
    options: Options,
    column: &mut usize,
    has_symbol: &mut bool,
) {
    if let Whitespace::Wrapped { width, ending } = options.whitespace {
        if *has_symbol && *column == width {
            match ending {
                LineEnding::Lf => out.push('\n'),
                LineEnding::CrLf => out.push_str("\r\n"),
            }
            *column = 0;
        }
        out.push(symbol as char);
        *column += 1;
        *has_symbol = true;
    } else {
        out.push(symbol as char);
    }
}

fn encode_all(input: &[u8], options: Options) -> String {
    let mut out = String::new();
    let mut column = 0;
    let mut has_symbol = false;
    let mut i = 0;

    while i + 3 <= input.len() {
        let digits = encode_digits(&input[i..i + 3], options.alphabet);
        for &digit in &digits {
            append_wrapped_symbol(&mut out, digit, options, &mut column, &mut has_symbol);
        }
        i += 3;
    }

    let tail = &input[i..];
    if !tail.is_empty() {
        let mut digits = encode_digits(tail, options.alphabet);
        if options.padding == Padding::Forbidden {
            let count = if tail.len() == 1 { 2 } else { 3 };
            for &digit in &digits[..count] {
                append_wrapped_symbol(&mut out, digit, options, &mut column, &mut has_symbol);
            }
        } else {
            for &digit in &digits {
                append_wrapped_symbol(&mut out, digit, options, &mut column, &mut has_symbol);
            }
        }
        digits.fill(0);
    }

    out
}

pub fn encode(input: &[u8], options: Options) -> Result<String, Error> {
    validate_options(options)?;
    Ok(encode_all(input, options))
}

pub struct Encoder {
    options: Options,
    tail: Vec<u8>,
    column: usize,
    has_symbol: bool,
    finished: bool,
}

impl Encoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate_options(options)?;
        Ok(Self {
            options,
            tail: Vec::with_capacity(2),
            column: 0,
            has_symbol: false,
            finished: false,
        })
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        if input.is_empty() {
            return Ok(String::new());
        }

        let mut data = Vec::with_capacity(self.tail.len() + input.len());
        data.extend_from_slice(&self.tail);
        data.extend_from_slice(input);
        self.tail.clear();

        let complete_len = data.len() / 3 * 3;
        let mut out = String::new();
        for chunk in data[..complete_len].chunks_exact(3) {
            let digits = encode_digits(chunk, self.options.alphabet);
            for &digit in &digits {
                append_wrapped_symbol(
                    &mut out,
                    digit,
                    self.options,
                    &mut self.column,
                    &mut self.has_symbol,
                );
            }
        }
        self.tail.extend_from_slice(&data[complete_len..]);
        Ok(out)
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        self.finished = true;

        let mut out = String::new();
        if !self.tail.is_empty() {
            let digits = encode_digits(&self.tail, self.options.alphabet);
            let count = if self.options.padding == Padding::Forbidden {
                if self.tail.len() == 1 { 2 } else { 3 }
            } else {
                4
            };
            for &digit in &digits[..count] {
                append_wrapped_symbol(
                    &mut out,
                    digit,
                    self.options,
                    &mut self.column,
                    &mut self.has_symbol,
                );
            }
        }
        self.tail.clear();
        Ok(out)
    }
}

#[derive(Clone, Copy)]
struct Symbol {
    value: u8,
    offset: usize,
}

#[derive(Clone, Copy)]
enum WrapState {
    Normal,
    NeedLf { cr_offset: usize },
    NeedSymbol { separator_offset: usize },
}

struct DecoderState {
    options: Options,
    offset: usize,
    symbols_seen: usize,
    group: Vec<Symbol>,
    pending_second_pad: Option<usize>,
    padded_finished: bool,
    wrap_column: usize,
    wrap_state: WrapState,
    at_row_start: bool,
    finished: bool,
    sticky: Option<Error>,
}

impl DecoderState {
    fn new(options: Options) -> Self {
        Self {
            options,
            offset: 0,
            symbols_seen: 0,
            group: Vec::with_capacity(4),
            pending_second_pad: None,
            padded_finished: false,
            wrap_column: 0,
            wrap_state: WrapState::Normal,
            at_row_start: false,
            finished: false,
            sticky: None,
        }
    }

    fn fail<T>(&mut self, error: Error) -> Result<T, Error> {
        self.sticky = Some(error.clone());
        Err(error)
    }

    fn skipped(&self, byte: u8) -> bool {
        self.options.whitespace == Whitespace::IgnoreAscii
            && matches!(byte, 9 | 10 | 11 | 12 | 13 | 32)
    }

    fn foreign_alphabet(&self, byte: u8) -> bool {
        match self.options.alphabet {
            Alphabet::Standard => matches!(byte, b'-' | b'_'),
            Alphabet::UrlSafe => matches!(byte, b'+' | b'/'),
        }
    }

    fn digit(&self, byte: u8) -> Option<u8> {
        alphabet_bytes(self.options.alphabet)
            .iter()
            .position(|&b| b == byte)
            .map(|v| v as u8)
    }

    fn accept_symbol(&mut self, byte: u8, offset: usize, out: &mut Vec<u8>) -> Result<(), Error> {
        if let Some(limit) = self.options.max_symbols {
            if self.symbols_seen >= limit {
                return Err(Error::Overlong { offset, limit });
            }
        }
        self.symbols_seen = self.symbols_seen.saturating_add(1);

        if self.padded_finished {
            return Err(Error::BadPadding { offset });
        }
        if let Some(pad_offset) = self.pending_second_pad {
            if byte != b'=' {
                return Err(Error::BadPadding { offset });
            }
            self.group.push(Symbol { value: 64, offset });
            self.pending_second_pad = None;
            return self.complete_group(out, true, pad_offset);
        }
        if byte == b'=' {
            if self.options.padding == Padding::Forbidden {
                return Err(Error::BadPadding { offset });
            }
            match self.group.len() {
                0 | 1 => return Err(Error::BadPadding { offset }),
                2 => {
                    self.group.push(Symbol { value: 64, offset });
                    self.pending_second_pad = Some(offset);
                    return Ok(());
                }
                3 => {
                    self.group.push(Symbol { value: 64, offset });
                    return self.complete_group(out, true, offset);
                }
                _ => return Err(Error::BadPadding { offset }),
            }
        }

        let value = match self.digit(byte) {
            Some(v) => v,
            None if self.options.strict && self.foreign_alphabet(byte) => {
                return Err(Error::MixedAlphabet { offset });
            }
            None => return Err(Error::InvalidByte { offset, byte }),
        };
        self.group.push(Symbol { value, offset });
        if self.group.len() == 4 {
            self.complete_group(out, false, offset)?;
        }
        Ok(())
    }

    fn complete_group(
        &mut self,
        out: &mut Vec<u8>,
        padded: bool,
        pad_offset: usize,
    ) -> Result<(), Error> {
        if self.group.len() != 4 {
            return Err(Error::BadPadding { offset: pad_offset });
        }
        let a = self.group[0];
        let b = self.group[1];
        let c = self.group[2];
        let d = self.group[3];

        if c.value == 64 {
            if d.value != 64 {
                return Err(Error::BadPadding { offset: d.offset });
            }
            if self.options.strict && (b.value & 0x0f) != 0 {
                return Err(Error::NonCanonical { offset: b.offset });
            }
            out.push((a.value << 2) | (b.value >> 4));
            self.padded_finished = true;
        } else if d.value == 64 {
            if self.options.strict && (c.value & 0x03) != 0 {
                return Err(Error::NonCanonical { offset: c.offset });
            }
            out.push((a.value << 2) | (b.value >> 4));
            out.push((b.value << 4) | (c.value >> 2));
            self.padded_finished = true;
        } else {
            out.push((a.value << 2) | (b.value >> 4));
            out.push((b.value << 4) | (c.value >> 2));
            out.push((c.value << 6) | d.value);
        }

        self.group.clear();
        if padded {
            self.padded_finished = true;
        }
        Ok(())
    }

    fn process_wrapped_byte(&mut self, byte: u8, offset: usize) -> Result<bool, Error> {
        let (width, ending) = match self.options.whitespace {
            Whitespace::Wrapped { width, ending } => (width, ending),
            _ => return Ok(false),
        };

        match self.wrap_state {
            WrapState::NeedLf { cr_offset } => {
                if byte != b'\n' {
                    return Err(Error::BadWrap { offset });
                }
                self.wrap_state = WrapState::NeedSymbol {
                    separator_offset: cr_offset,
                };
                return Ok(true);
            }
            WrapState::NeedSymbol { .. } => {
                self.wrap_state = WrapState::Normal;
                self.wrap_column = 0;
                self.at_row_start = false;
            }
            WrapState::Normal => {}
        }

        if self.wrap_column == width {
            match ending {
                LineEnding::Lf if byte == b'\n' => {
                    self.wrap_state = WrapState::NeedSymbol {
                        separator_offset: offset,
                    };
                    return Ok(true);
                }
                LineEnding::CrLf if byte == b'\r' => {
                    self.wrap_state = WrapState::NeedLf { cr_offset: offset };
                    return Ok(true);
                }
                _ => return Err(Error::BadWrap { offset }),
            }
        }

        if matches!(byte, 9 | 10 | 11 | 12 | 13 | 32) {
            return Err(Error::BadWrap { offset });
        }
        self.wrap_column += 1;
        Ok(false)
    }

    fn process_byte(&mut self, byte: u8, offset: usize, out: &mut Vec<u8>) -> Result<(), Error> {
        if self.skipped(byte) {
            return Ok(());
        }
        if matches!(self.options.whitespace, Whitespace::Wrapped { .. }) {
            if self.process_wrapped_byte(byte, offset)? {
                return Ok(());
            }
        }
        self.accept_symbol(byte, offset, out)
    }

    fn finish(&mut self) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky {
            return Err(error.clone());
        }
        if self.finished {
            return self.fail(Error::AlreadyFinished);
        }

        let wrap_error = match self.wrap_state {
            WrapState::NeedLf { cr_offset } => Some(cr_offset),
            WrapState::NeedSymbol { separator_offset } => Some(separator_offset),
            WrapState::Normal => None,
        };
        if let Some(offset) = wrap_error {
            return self.fail(Error::BadWrap { offset });
        }

        let mut out = Vec::new();
        if let Some(offset) = self.pending_second_pad {
            return self.fail(Error::BadPadding { offset });
        }
        match self.group.len() {
            0 => {}
            1 => {
                let offset = self.group[0].offset;
                return self.fail(Error::Truncated { offset });
            }
            2 | 3 => {
                if self.options.padding == Padding::Required {
                    let offset = self.group[0].offset;
                    return self.fail(Error::Truncated { offset });
                }
                if self.group[0].value >= 64 || self.group[1].value >= 64 {
                    return self.fail(Error::BadPadding {
                        offset: self.group[0].offset,
                    });
                }
                if self.group.len() == 2 {
                    if self.options.strict && (self.group[1].value & 0x0f) != 0 {
                        return self.fail(Error::NonCanonical {
                            offset: self.group[1].offset,
                        });
                    }
                    out.push((self.group[0].value << 2) | (self.group[1].value >> 4));
                } else {
                    if self.group[2].value >= 64 {
                        return self.fail(Error::BadPadding {
                            offset: self.group[2].offset,
                        });
                    }
                    if self.options.strict && (self.group[2].value & 0x03) != 0 {
                        return self.fail(Error::NonCanonical {
                            offset: self.group[2].offset,
                        });
                    }
                    out.push((self.group[0].value << 2) | (self.group[1].value >> 4));
                    out.push((self.group[1].value << 4) | (self.group[2].value >> 2));
                }
            }
            _ => {}
        }

        self.group.clear();
        self.finished = true;
        Ok(out)
    }
}

pub struct Decoder {
    state: DecoderState,
}

impl Decoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate_options(options)?;
        Ok(Self {
            state: DecoderState::new(options),
        })
    }

    pub fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.state.sticky {
            return Err(error.clone());
        }
        if self.state.finished {
            return self.state.fail(Error::AlreadyFinished);
        }

        let mut out = Vec::new();
        for &byte in input {
            let offset = self.state.offset;
            self.state.offset = self.state.offset.saturating_add(1);
            if let Err(error) = self.state.process_byte(byte, offset, &mut out) {
                return self.state.fail(error);
            }
        }
        Ok(out)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, Error> {
        self.state.finish()
    }
}

pub fn decode(input: &[u8], options: Options) -> Result<Vec<u8>, Error> {
    let mut decoder = Decoder::new(options)?;
    let mut out = decoder.push(input)?;
    out.extend(decoder.finish()?);
    Ok(out)
}