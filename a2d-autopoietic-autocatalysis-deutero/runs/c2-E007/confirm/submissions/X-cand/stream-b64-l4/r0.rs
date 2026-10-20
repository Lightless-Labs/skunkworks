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

fn foreign_alphabet_symbol(byte: u8, alphabet: Alphabet) -> bool {
    match alphabet {
        Alphabet::Standard => byte == b'-' || byte == b'_',
        Alphabet::UrlSafe => byte == b'+' || byte == b'/',
    }
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

fn symbol_bytes(input: &[u8], options: Options) -> Vec<u8> {
    let mut out = Vec::new();
    for chunk in input.chunks(3) {
        let a = chunk[0];
        let b = *chunk.get(1).unwrap_or(&0);
        let c = *chunk.get(2).unwrap_or(&0);

        out.push(encode_digit(a >> 2, options.alphabet));
        out.push(encode_digit(((a & 0x03) << 4) | (b >> 4), options.alphabet));

        if chunk.len() >= 2 {
            out.push(encode_digit(((b & 0x0f) << 2) | (c >> 6), options.alphabet));
        } else if options.padding != Padding::Forbidden {
            out.push(b'=');
        }

        if chunk.len() >= 3 {
            out.push(encode_digit(c & 0x3f, options.alphabet));
        } else if options.padding != Padding::Forbidden {
            out.push(b'=');
        }
    }
    out
}

fn wrap_symbols(symbols: &[u8], whitespace: Whitespace) -> String {
    match whitespace {
        Whitespace::Wrapped { width, ending } => {
            let mut out = Vec::new();
            for (i, &b) in symbols.iter().enumerate() {
                if i > 0 && i % width == 0 {
                    match ending {
                        LineEnding::Lf => out.push(b'\n'),
                        LineEnding::CrLf => {
                            out.push(b'\r');
                            out.push(b'\n');
                        }
                    }
                }
                out.push(b);
            }
            String::from_utf8(out).unwrap()
        }
        _ => String::from_utf8(symbols.to_vec()).unwrap(),
    }
}

pub fn encode(input: &[u8], options: Options) -> Result<String, Error> {
    validate(options)?;
    Ok(wrap_symbols(&symbol_bytes(input, options), options.whitespace))
}

#[derive(Clone, Copy)]
enum FrameState {
    Normal,
    NeedLf { cr_offset: usize, sep_offset: usize },
    NeedSymbol { sep_offset: usize },
}

pub struct Encoder {
    options: Options,
    tail: Vec<u8>,
    symbols_in_row: usize,
    closed: bool,
}

impl Encoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            tail: Vec::new(),
            symbols_in_row: 0,
            closed: false,
        })
    }

    fn emit_symbol(&mut self, out: &mut String, byte: u8) {
        if let Whitespace::Wrapped { width, ending } = self.options.whitespace {
            if self.symbols_in_row == width {
                match ending {
                    LineEnding::Lf => out.push('\n'),
                    LineEnding::CrLf => out.push_str("\r\n"),
                }
                self.symbols_in_row = 0;
            }
            out.push(byte as char);
            self.symbols_in_row += 1;
        } else {
            out.push(byte as char);
        }
    }

    fn emit_group(&mut self, out: &mut String, bytes: &[u8]) {
        let symbols = symbol_bytes(bytes, self.options);
        for byte in symbols {
            self.emit_symbol(out, byte);
        }
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        if input.is_empty() {
            return Ok(String::new());
        }

        let mut out = String::new();
        let mut data = std::mem::take(&mut self.tail);
        data.extend_from_slice(input);

        let complete = data.len() / 3 * 3;
        for chunk in data[..complete].chunks_exact(3) {
            self.emit_group(&mut out, chunk);
        }
        self.tail.extend_from_slice(&data[complete..]);
        Ok(out)
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        self.closed = true;
        let mut out = String::new();
        if !self.tail.is_empty() {
            let tail = std::mem::take(&mut self.tail);
            self.emit_group(&mut out, &tail);
        }
        Ok(out)
    }
}

pub struct Decoder {
    options: Options,
    offset: usize,
    symbols_seen: usize,
    group: Vec<(u8, usize)>,
    terminal_padding: bool,
    pending_second_padding: Option<usize>,
    frame: FrameState,
    row_symbols: usize,
    emitted_after_separator: bool,
    sticky: Option<Error>,
    closed: bool,
}

impl Decoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            offset: 0,
            symbols_seen: 0,
            group: Vec::with_capacity(4),
            terminal_padding: false,
            pending_second_padding: None,
            frame: FrameState::Normal,
            row_symbols: 0,
            emitted_after_separator: false,
            sticky: None,
            closed: false,
        })
    }

    fn fail(&mut self, error: Error) -> Result<Vec<u8>, Error> {
        self.sticky = Some(error.clone());
        Err(error)
    }

    fn canonical_error(&self, group: &[(u8, usize)]) -> Option<Error> {
        if !self.options.strict {
            return None;
        }
        if group.len() == 4 {
            if group[2].0 == b'=' && group[3].0 == b'=' {
                let value = digit(group[1].0, self.options.alphabet)?;
                if value & 0x0f != 0 {
                    return Some(Error::NonCanonical { offset: group[1].1 });
                }
            } else if group[3].0 == b'=' {
                let value = digit(group[2].0, self.options.alphabet)?;
                if value & 0x03 != 0 {
                    return Some(Error::NonCanonical { offset: group[2].1 });
                }
            }
        }
        None
    }

    fn complete_group(&mut self, out: &mut Vec<u8>) -> Result<(), Error> {
        let group = self.group.clone();
        let a = digit(group[0].0, self.options.alphabet).unwrap();
        let b = digit(group[1].0, self.options.alphabet).unwrap();

        if group[2].0 == b'=' {
            if let Some(err) = self.canonical_error(&group) {
                return Err(err);
            }
            out.push((a << 2) | (b >> 4));
            self.terminal_padding = true;
        } else {
            let c = digit(group[2].0, self.options.alphabet).unwrap();
            if group[3].0 == b'=' {
                if let Some(err) = self.canonical_error(&group) {
                    return Err(err);
                }
                out.push((a << 2) | (b >> 4));
                out.push((b << 4) | (c >> 2));
                self.terminal_padding = true;
            } else {
                let d = digit(group[3].0, self.options.alphabet).unwrap();
                out.push((a << 2) | (b >> 4));
                out.push((b << 4) | (c >> 2));
                out.push((c << 6) | d);
            }
        }
        self.group.clear();
        self.pending_second_padding = None;
        Ok(())
    }

    fn consume_symbol(&mut self, byte: u8, offset: usize, out: &mut Vec<u8>) -> Result<(), Error> {
        if let Some(limit) = self.options.max_symbols {
            if self.symbols_seen >= limit {
                return Err(Error::Overlong { offset, limit });
            }
        }
        self.symbols_seen += 1;

        if self.terminal_padding {
            return Err(Error::BadPadding { offset });
        }

        if let Some(pending_offset) = self.pending_second_padding {
            if byte != b'=' {
                return Err(Error::BadPadding { offset });
            }
            self.group.push((byte, offset));
            self.pending_second_padding = None;
            return self.complete_group(out);
        }

        let pos = self.group.len();
        if byte == b'=' {
            if self.options.padding == Padding::Forbidden || pos < 2 || pos > 3 {
                return Err(Error::BadPadding { offset });
            }
            self.group.push((byte, offset));
            if pos == 2 {
                self.pending_second_padding = Some(offset);
            } else {
                self.complete_group(out)?;
            }
            return Ok(());
        }

        if let Some(value) = digit(byte, self.options.alphabet) {
            let _ = value;
            self.group.push((byte, offset));
            if self.group.len() == 4 {
                self.complete_group(out)?;
            }
            return Ok(());
        }

        if self.options.strict && foreign_alphabet_symbol(byte, self.options.alphabet) {
            return Err(Error::MixedAlphabet { offset });
        }
        Err(Error::InvalidByte { offset, byte })
    }

    fn is_ignored(byte: u8) -> bool {
        matches!(byte, 9 | 10 | 11 | 12 | 13 | 32)
    }

    fn process_wrapped(&mut self, byte: u8, offset: usize) -> Result<bool, Error> {
        let (width, ending) = match self.options.whitespace {
            Whitespace::Wrapped { width, ending } => (width, ending),
            _ => return Ok(false),
        };

        match self.frame {
            FrameState::NeedLf { cr_offset, .. } => {
                if byte != b'\n' {
                    return Err(Error::BadWrap { offset });
                }
                self.frame = FrameState::NeedSymbol { sep_offset: cr_offset };
                return Ok(true);
            }
            FrameState::NeedSymbol { sep_offset } => {
                if byte == b'\n' || byte == b'\r' {
                    return Err(Error::BadWrap { offset });
                }
                self.frame = FrameState::Normal;
                self.row_symbols = 0;
                self.emitted_after_separator = true;
                let _ = sep_offset;
            }
            FrameState::Normal => {}
        }

        if self.row_symbols == width {
            let first_sep = offset;
            match ending {
                LineEnding::Lf => {
                    if byte != b'\n' {
                        return Err(Error::BadWrap { offset });
                    }
                    self.frame = FrameState::NeedSymbol { sep_offset: first_sep };
                    return Ok(true);
                }
                LineEnding::CrLf => {
                    if byte != b'\r' {
                        return Err(Error::BadWrap { offset });
                    }
                    self.frame = FrameState::NeedLf {
                        cr_offset: offset,
                        sep_offset: first_sep,
                    };
                    return Ok(true);
                }
            }
        }

        if byte == b'\r' || byte == b'\n' || Self::is_ignored(byte) {
            return Err(Error::BadWrap { offset });
        }

        self.row_symbols += 1;
        Ok(false)
    }

    fn scan_byte(&mut self, byte: u8, offset: usize, out: &mut Vec<u8>) -> Result<(), Error> {
        match self.options.whitespace {
            Whitespace::IgnoreAscii if Self::is_ignored(byte) => return Ok(()),
            Whitespace::None => {}
            Whitespace::IgnoreAscii => {}
            Whitespace::Wrapped { .. } => {
                if self.process_wrapped(byte, offset)? {
                    return Ok(());
                }
            }
        }
        self.consume_symbol(byte, offset, out)
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
            let offset = self.offset;
            self.offset += 1;
            if let Err(error) = self.scan_byte(byte, offset, &mut out) {
                return self.fail(error);
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

        if let FrameState::NeedLf { cr_offset, .. } = self.frame {
            return self.fail(Error::BadWrap { offset: cr_offset });
        }
        if let FrameState::NeedSymbol { sep_offset } = self.frame {
            return self.fail(Error::BadWrap { offset: sep_offset });
        }

        if let Some(offset) = self.pending_second_padding {
            return self.fail(Error::BadPadding { offset });
        }

        if !self.group.is_empty() {
            let start = self.group[0].1;
            match self.options.padding {
                Padding::Required => return self.fail(Error::Truncated { offset: start }),
                Padding::Forbidden | Padding::Optional => {
                    if self.group.len() == 1 {
                        return self.fail(Error::Truncated { offset: start });
                    }
                    if self.group.iter().any(|(b, _)| *b == b'=') {
                        return self.fail(Error::BadPadding {
                            offset: self.group.iter().find(|(b, _)| *b == b'=').unwrap().1,
                        });
                    }
                    let mut out = Vec::new();
                    let a = digit(self.group[0].0, self.options.alphabet).unwrap();
                    let b = digit(self.group[1].0, self.options.alphabet).unwrap();
                    if self.group.len() == 2 {
                        if self.options.strict && b & 0x0f != 0 {
                            return self.fail(Error::NonCanonical { offset: self.group[1].1 });
                        }
                        out.push((a << 2) | (b >> 4));
                    } else {
                        let c = digit(self.group[2].0, self.options.alphabet).unwrap();
                        if self.options.strict && c & 0x03 != 0 {
                            return self.fail(Error::NonCanonical { offset: self.group[2].1 });
                        }
                        out.push((a << 2) | (b >> 4));
                        out.push((b << 4) | (c >> 2));
                    }
                    self.group.clear();
                    self.closed = true;
                    return Ok(out);
                }
            }
        }

        self.closed = true;
        Ok(Vec::new())
    }
}

pub fn decode(input: &[u8], options: Options) -> Result<Vec<u8>, Error> {
    validate(options)?;
    let mut decoder = Decoder::new(options)?;
    let mut output = decoder.push(input)?;
    output.extend(decoder.finish()?);
    Ok(output)
}