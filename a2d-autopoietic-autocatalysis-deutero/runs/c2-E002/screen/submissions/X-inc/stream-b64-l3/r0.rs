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

fn symbols(options: Options) -> &'static [u8; 64] {
    const STANDARD: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    const URL: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    match options.alphabet {
        Alphabet::Standard => STANDARD,
        Alphabet::UrlSafe => URL,
    }
}

fn encode_group(bytes: &[u8], options: Options, out: &mut String) {
    let table = symbols(options);
    let a = bytes[0];
    let b = if bytes.len() > 1 { bytes[1] } else { 0 };
    let c = if bytes.len() > 2 { bytes[2] } else { 0 };

    let mut encoded = [0u8; 4];
    encoded[0] = table[(a >> 2) as usize];
    encoded[1] = table[(((a & 0x03) << 4) | (b >> 4)) as usize];
    encoded[2] = table[(((b & 0x0f) << 2) | (c >> 6)) as usize];
    encoded[3] = table[(c & 0x3f) as usize];

    let count = match bytes.len() {
        1 => 2,
        2 => 3,
        _ => 4,
    };
    for &byte in &encoded[..count] {
        out.push(byte as char);
    }
    if options.padding != Padding::Forbidden {
        for _ in count..4 {
            out.push('=');
        }
    }
}

fn wrap_encoded(raw: String, whitespace: Whitespace) -> String {
    let Whitespace::Wrapped { width, ending } = whitespace else {
        return raw;
    };
    if raw.is_empty() {
        return raw;
    }
    let separator = match ending {
        LineEnding::Lf => "\n",
        LineEnding::CrLf => "\r\n",
    };
    let mut result = String::with_capacity(raw.len() + raw.len() / width * separator.len());
    for (i, chunk) in raw.as_bytes().chunks(width).enumerate() {
        if i != 0 {
            result.push_str(separator);
        }
        for &byte in chunk {
            result.push(byte as char);
        }
    }
    result
}

pub fn encode(input: &[u8], options: Options) -> Result<String, Error> {
    validate(options)?;
    let mut raw = String::new();
    for chunk in input.chunks(3) {
        encode_group(chunk, options, &mut raw);
    }
    Ok(wrap_encoded(raw, options.whitespace))
}

pub struct Encoder {
    options: Options,
    buffered: Vec<u8>,
    closed: bool,
}

impl Encoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            buffered: Vec::with_capacity(2),
            closed: false,
        })
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        let mut all = Vec::with_capacity(self.buffered.len() + input.len());
        all.extend_from_slice(&self.buffered);
        all.extend_from_slice(input);

        let complete_len = all.len() / 3 * 3;
        let mut raw = String::new();
        for chunk in all[..complete_len].chunks(3) {
            encode_group(chunk, self.options, &mut raw);
        }
        self.buffered.clear();
        self.buffered.extend_from_slice(&all[complete_len..]);
        Ok(wrap_encoded(raw, self.options.whitespace))
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        self.closed = true;
        let mut raw = String::new();
        if !self.buffered.is_empty() {
            encode_group(&self.buffered, self.options, &mut raw);
        }
        self.buffered.clear();
        Ok(wrap_encoded(raw, self.options.whitespace))
    }
}

#[derive(Clone, Copy)]
struct Symbol {
    byte: u8,
    offset: usize,
}

#[derive(Clone, Copy)]
enum FrameState {
    Normal,
    NeedLf { cr_offset: usize },
    NeedSymbol { separator_offset: usize },
}

pub struct Decoder {
    options: Options,
    group: Vec<Symbol>,
    offset: usize,
    symbol_count: usize,
    frame_state: FrameState,
    row_symbols: usize,
    terminal_padding: bool,
    pending_second_padding: Option<usize>,
    closed: bool,
    sticky_error: Option<Error>,
}

impl Decoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            group: Vec::with_capacity(4),
            offset: 0,
            symbol_count: 0,
            frame_state: FrameState::Normal,
            row_symbols: 0,
            terminal_padding: false,
            pending_second_padding: None,
            closed: false,
            sticky_error: None,
        })
    }

    fn fail(&mut self, error: Error) -> Error {
        self.sticky_error = Some(error.clone());
        error
    }

    fn process_symbol(&mut self, byte: u8, offset: usize, out: &mut Vec<u8>) -> Result<(), Error> {
        if self.terminal_padding {
            return Err(Error::BadPadding { offset });
        }
        if let Some(pending_offset) = self.pending_second_padding {
            if byte != b'=' {
                return Err(Error::BadPadding { offset });
            }
            self.pending_second_padding = None;
            self.group.push(Symbol { byte, offset });
            self.emit_group(out);
            return Ok(());
        }

        let pos = self.group.len();
        if byte == b'=' {
            if pos <= 1 || self.options.padding == Padding::Forbidden {
                return Err(Error::BadPadding { offset });
            }
            if pos == 2 {
                self.pending_second_padding = Some(offset);
                self.group.push(Symbol { byte, offset });
                return Ok(());
            }
            self.group.push(Symbol { byte, offset });
            self.emit_group(out);
            return Ok(());
        }

        let value = match decode_digit(byte, self.options.alphabet) {
            Some(v) => v,
            None => return Err(Error::InvalidByte { offset, byte }),
        };
        let _ = value;
        self.group.push(Symbol { byte, offset });
        if self.group.len() == 4 {
            self.emit_group(out);
        }
        Ok(())
    }

    fn emit_group(&mut self, out: &mut Vec<u8>) {
        let values: Vec<Option<u8>> = self.group.iter().map(|s| {
            if s.byte == b'=' {
                None
            } else {
                decode_digit(s.byte, self.options.alphabet)
            }
        }).collect();

        match self.group.len() {
            4 if values[2].is_none() && values[3].is_none() => {
                let a = values[0].unwrap();
                let b = values[1].unwrap();
                out.push((a << 2) | (b >> 4));
                self.terminal_padding = true;
            }
            4 if values[3].is_none() => {
                let a = values[0].unwrap();
                let b = values[1].unwrap();
                let c = values[2].unwrap();
                out.push((a << 2) | (b >> 4));
                out.push((b << 4) | (c >> 2));
                self.terminal_padding = true;
            }
            4 => {
                let a = values[0].unwrap();
                let b = values[1].unwrap();
                let c = values[2].unwrap();
                let d = values[3].unwrap();
                out.push((a << 2) | (b >> 4));
                out.push((b << 4) | (c >> 2));
                out.push((c << 6) | d);
            }
            _ => {}
        }
        self.group.clear();
    }

    fn handle_wrapped_byte(&mut self, byte: u8, offset: usize) -> Result<bool, Error> {
        let Whitespace::Wrapped { width, ending } = self.options.whitespace else {
            return Ok(false);
        };

        match self.frame_state {
            FrameState::NeedLf { cr_offset } => {
                if byte != b'\n' {
                    return Err(Error::BadWrap { offset });
                }
                self.frame_state = FrameState::NeedSymbol {
                    separator_offset: cr_offset,
                };
                return Ok(true);
            }
            FrameState::NeedSymbol { .. } => {
                self.frame_state = FrameState::Normal;
                self.row_symbols = 0;
                if byte == b'\r' || byte == b'\n' || is_ascii_whitespace(byte) {
                    return Err(Error::BadWrap { offset });
                }
                return Ok(false);
            }
            FrameState::Normal => {}
        }

        if self.row_symbols == width {
            let first = match ending {
                LineEnding::Lf => {
                    if byte != b'\n' {
                        return Err(Error::BadWrap { offset });
                    }
                    offset
                }
                LineEnding::CrLf => {
                    if byte != b'\r' {
                        return Err(Error::BadWrap { offset });
                    }
                    self.frame_state = FrameState::NeedLf { cr_offset: offset };
                    return Ok(true);
                }
            };
            self.frame_state = FrameState::NeedSymbol {
                separator_offset: first,
            };
            return Ok(true);
        }

        if byte == b'\r' || byte == b'\n' || is_ascii_whitespace(byte) {
            return Err(Error::BadWrap { offset });
        }
        Ok(false)
    }

    pub fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky_error {
            return Err(error.clone());
        }
        if self.closed {
            return Err(Error::AlreadyFinished);
        }

        let mut out = Vec::new();
        for &byte in input {
            let current_offset = self.offset;
            self.offset += 1;

            if self.options.whitespace == Whitespace::IgnoreAscii && is_ascii_whitespace(byte) {
                continue;
            }

            match self.handle_wrapped_byte(byte, current_offset) {
                Ok(true) => continue,
                Ok(false) => {}
                Err(error) => return Err(self.fail(error)),
            }

            self.symbol_count += 1;
            if let Err(error) = self.process_symbol(byte, current_offset, &mut out) {
                return Err(self.fail(error));
            }
            if let Whitespace::Wrapped { width, .. } = self.options.whitespace {
                self.row_symbols += 1;
                if self.row_symbols > width {
                    return Err(self.fail(Error::BadWrap { offset: current_offset }));
                }
            }
        }
        Ok(out)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky_error {
            return Err(error.clone());
        }
        if self.closed {
            return Err(Error::AlreadyFinished);
        }

        if let FrameState::NeedLf { cr_offset } = self.frame_state {
            let error = Error::BadWrap { offset: cr_offset };
            return Err(self.fail(error));
        }
        if let FrameState::NeedSymbol { separator_offset } = self.frame_state {
            let error = Error::BadWrap {
                offset: separator_offset,
            };
            return Err(self.fail(error));
        }

        if let Some(offset) = self.pending_second_padding {
            let error = Error::BadPadding { offset };
            return Err(self.fail(error));
        }

        let mut out = Vec::new();
        match self.group.len() {
            0 => {}
            1 => {
                let error = Error::Truncated {
                    offset: self.group[0].offset,
                };
                return Err(self.fail(error));
            }
            2 | 3 if self.options.padding != Padding::Required => {
                let a = decode_digit(self.group[0].byte, self.options.alphabet).unwrap();
                let b = decode_digit(self.group[1].byte, self.options.alphabet).unwrap();
                out.push((a << 2) | (b >> 4));
                if self.group.len() == 3 {
                    let c = decode_digit(self.group[2].byte, self.options.alphabet).unwrap();
                    out.push((b << 4) | (c >> 2));
                }
            }
            _ => {
                let error = Error::Truncated {
                    offset: self.group[0].offset,
                };
                return Err(self.fail(error));
            }
        }

        self.group.clear();
        self.closed = true;
        Ok(out)
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

fn is_ascii_whitespace(byte: u8) -> bool {
    matches!(byte, 9 | 10 | 11 | 12 | 13 | 32)
}

pub fn decode(input: &[u8], options: Options) -> Result<Vec<u8>, Error> {
    validate(options)?;
    let mut decoder = Decoder::new(options)?;
    let mut output = decoder.push(input)?;
    output.extend(decoder.finish()?);
    Ok(output)
}