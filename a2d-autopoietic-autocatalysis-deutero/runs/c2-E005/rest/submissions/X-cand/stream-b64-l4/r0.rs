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

fn alphabet_digit(alphabet: Alphabet, byte: u8) -> Option<u8> {
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

fn is_foreign_alphabet_byte(alphabet: Alphabet, byte: u8) -> bool {
    match alphabet {
        Alphabet::Standard => byte == b'-' || byte == b'_',
        Alphabet::UrlSafe => byte == b'+' || byte == b'/',
    }
}

fn encoded_symbol(alphabet: Alphabet, value: u8) -> u8 {
    match value {
        0..=25 => b'A' + value,
        26..=51 => b'a' + value - 26,
        52..=61 => b'0' + value - 52,
        62 if alphabet == Alphabet::Standard => b'+',
        62 => b'-',
        63 if alphabet == Alphabet::Standard => b'/',
        _ => b'_',
    }
}

fn append_symbol(out: &mut String, byte: u8) {
    out.push(byte as char);
}

fn encode_raw(input: &[u8], options: Options) -> Vec<u8> {
    let mut out = Vec::new();
    let mut chunks = input.chunks_exact(3);
    for chunk in &mut chunks {
        let a = chunk[0];
        let b = chunk[1];
        let c = chunk[2];
        out.push(encoded_symbol(options.alphabet, a >> 2));
        out.push(encoded_symbol(options.alphabet, ((a & 0x03) << 4) | (b >> 4)));
        out.push(encoded_symbol(options.alphabet, ((b & 0x0f) << 2) | (c >> 6)));
        out.push(encoded_symbol(options.alphabet, c & 0x3f));
    }
    let tail = chunks.remainder();
    if !tail.is_empty() {
        let a = tail[0];
        out.push(encoded_symbol(options.alphabet, a >> 2));
        if tail.len() == 1 {
            out.push(encoded_symbol(options.alphabet, (a & 0x03) << 4));
            if options.padding != Padding::Forbidden {
                out.extend_from_slice(b"==");
            }
        } else {
            let b = tail[1];
            out.push(encoded_symbol(options.alphabet, ((a & 0x03) << 4) | (b >> 4)));
            out.push(encoded_symbol(options.alphabet, (b & 0x0f) << 2));
            if options.padding != Padding::Forbidden {
                out.push(b'=');
            }
        }
    }
    out
}

fn line_ending_bytes(ending: LineEnding) -> &'static [u8] {
    match ending {
        LineEnding::Lf => b"\n",
        LineEnding::CrLf => b"\r\n",
    }
}

fn wrap_bytes(raw: &[u8], width: usize, ending: LineEnding) -> Vec<u8> {
    let mut out = Vec::with_capacity(
        raw.len().saturating_add(raw.len() / width * line_ending_bytes(ending).len()),
    );
    for (i, byte) in raw.iter().enumerate() {
        if i != 0 && i % width == 0 {
            out.extend_from_slice(line_ending_bytes(ending));
        }
        out.push(*byte);
    }
    out
}

pub fn encode(input: &[u8], options: Options) -> Result<String, Error> {
    validate(options)?;
    let raw = encode_raw(input, options);
    let bytes = match options.whitespace {
        Whitespace::Wrapped { width, ending } => wrap_bytes(&raw, width, ending),
        Whitespace::None | Whitespace::IgnoreAscii => raw,
    };
    Ok(String::from_utf8(bytes).expect("base64 output is ASCII"))
}

struct EncoderState {
    options: Options,
    pending: Vec<u8>,
    row_symbols: usize,
    closed: bool,
}

pub struct Encoder {
    state: EncoderState,
}

fn emit_encoded_symbols(state: &mut EncoderState, symbols: &[u8], out: &mut String) {
    let wrapped = match state.options.whitespace {
        Whitespace::Wrapped { width, ending } => Some((width, ending)),
        _ => None,
    };
    for symbol in symbols {
        if let Some((width, ending)) = wrapped {
            if state.row_symbols == width {
                for byte in line_ending_bytes(ending) {
                    append_symbol(out, *byte);
                }
                state.row_symbols = 0;
            }
            state.row_symbols += 1;
        }
        append_symbol(out, *symbol);
    }
}

impl Encoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            state: EncoderState {
                options,
                pending: Vec::new(),
                row_symbols: 0,
                closed: false,
            },
        })
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.state.closed {
            return Err(Error::AlreadyFinished);
        }
        let mut all = std::mem::take(&mut self.state.pending);
        all.extend_from_slice(input);
        let complete_len = all.len() / 3 * 3;
        let raw = encode_raw(&all[..complete_len], self.state.options);
        self.state.pending.extend_from_slice(&all[complete_len..]);
        let mut out = String::new();
        emit_encoded_symbols(&mut self.state, &raw, &mut out);
        Ok(out)
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.state.closed {
            return Err(Error::AlreadyFinished);
        }
        self.state.closed = true;
        let pending = std::mem::take(&mut self.state.pending);
        let raw = encode_raw(&pending, self.state.options);
        let mut out = String::new();
        emit_encoded_symbols(&mut self.state, &raw, &mut out);
        Ok(out)
    }
}

#[derive(Clone, Copy)]
struct Digit {
    value: u8,
    offset: usize,
}

#[derive(Clone, Copy)]
enum FramingState {
    Normal,
    NeedLf { cr_offset: usize },
    NeedSymbol { separator_offset: usize },
}

struct DecoderState {
    options: Options,
    group: Vec<Digit>,
    pending_second_pad: Option<usize>,
    terminal_padding: bool,
    symbol_count: usize,
    physical_offset: usize,
    row_symbols: usize,
    framing: FramingState,
    finished: bool,
    sticky: Option<Error>,
}

pub struct Decoder {
    state: DecoderState,
}

fn emit_group(state: &mut DecoderState, out: &mut Vec<u8>, padded: bool) -> Result<(), Error> {
    let group = &state.group;
    if padded {
        if group.len() == 2 {
            if state.options.strict && group[1].value & 0x0f != 0 {
                return Err(Error::NonCanonical {
                    offset: group[1].offset,
                });
            }
            out.push((group[0].value << 2) | (group[1].value >> 4));
        } else {
            if state.options.strict && group[2].value & 0x03 != 0 {
                return Err(Error::NonCanonical {
                    offset: group[2].offset,
                });
            }
            out.push((group[0].value << 2) | (group[1].value >> 4));
            out.push((group[1].value << 4) | (group[2].value >> 2));
        }
    } else {
        out.push((group[0].value << 2) | (group[1].value >> 4));
        out.push((group[1].value << 4) | (group[2].value >> 2));
        out.push((group[2].value << 6) | group[3].value);
    }
    Ok(())
}

fn is_ignored_whitespace(byte: u8) -> bool {
    matches!(byte, 9 | 10 | 11 | 12 | 13 | 32)
}

fn decoder_scan_byte(
    state: &mut DecoderState,
    byte: u8,
    offset: usize,
    out: &mut Vec<u8>,
) -> Result<(), Error> {
    match state.options.whitespace {
        Whitespace::IgnoreAscii if is_ignored_whitespace(byte) => return Ok(()),
        Whitespace::Wrapped { width, ending } => {
            match state.framing {
                FramingState::NeedLf { cr_offset } => {
                    if byte != b'\n' {
                        return Err(Error::BadWrap { offset });
                    }
                    state.framing = FramingState::NeedSymbol {
                        separator_offset: cr_offset,
                    };
                    return Ok(());
                }
                FramingState::NeedSymbol { .. } => {
                    state.framing = FramingState::Normal;
                    state.row_symbols = 0;
                }
                FramingState::Normal => {}
            }
            if state.row_symbols == width {
                match ending {
                    LineEnding::Lf if byte == b'\n' => {
                        state.framing = FramingState::NeedSymbol {
                            separator_offset: offset,
                        };
                        return Ok(());
                    }
                    LineEnding::CrLf if byte == b'\r' => {
                        state.framing = FramingState::NeedLf { cr_offset: offset };
                        return Ok(());
                    }
                    _ => return Err(Error::BadWrap { offset }),
                }
            } else if matches!(byte, 9 | 10 | 11 | 12 | 13 | 32) {
                return Err(Error::BadWrap { offset });
            }
        }
        _ => {}
    }

    if let Some(limit) = state.options.max_symbols {
        if state.symbol_count >= limit {
            return Err(Error::Overlong { offset, limit });
        }
    }
    state.symbol_count = state.symbol_count.saturating_add(1);
    if matches!(state.options.whitespace, Whitespace::Wrapped { .. }) {
        state.row_symbols = state.row_symbols.saturating_add(1);
    }

    if state.terminal_padding {
        return Err(Error::BadPadding { offset });
    }
    if let Some(pad_offset) = state.pending_second_pad {
        if byte != b'=' {
            return Err(Error::BadPadding { offset });
        }
        let mut result = Vec::new();
        emit_group(state, &mut result, true)?;
        out.extend(result);
        state.group.clear();
        state.pending_second_pad = None;
        state.terminal_padding = true;
        return Ok(());
    }

    if byte == b'=' {
        match state.group.len() {
            0 | 1 => return Err(Error::BadPadding { offset }),
            2 => {
                if state.options.padding == Padding::Forbidden {
                    return Err(Error::BadPadding { offset });
                }
                state.pending_second_pad = Some(offset);
                return Ok(());
            }
            3 => {
                if state.options.padding == Padding::Forbidden {
                    return Err(Error::BadPadding { offset });
                }
                let mut result = Vec::new();
                emit_group(state, &mut result, true)?;
                out.extend(result);
                state.group.clear();
                state.terminal_padding = true;
                return Ok(());
            }
            _ => return Err(Error::BadPadding { offset }),
        }
    }

    if state.group.len() == 3 {
        let value = match alphabet_digit(state.options.alphabet, byte) {
            Some(value) => value,
            None if state.options.strict && is_foreign_alphabet_byte(state.options.alphabet, byte) => {
                return Err(Error::MixedAlphabet { offset });
            }
            None => return Err(Error::InvalidByte { offset, byte }),
        };
        state.group.push(Digit { value, offset });
        let mut result = Vec::new();
        emit_group(state, &mut result, false)?;
        out.extend(result);
        state.group.clear();
        return Ok(());
    }

    let value = match alphabet_digit(state.options.alphabet, byte) {
        Some(value) => value,
        None if state.options.strict && is_foreign_alphabet_byte(state.options.alphabet, byte) => {
            return Err(Error::MixedAlphabet { offset });
        }
        None => return Err(Error::InvalidByte { offset, byte }),
    };
    state.group.push(Digit { value, offset });
    Ok(())
}

impl Decoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            state: DecoderState {
                options,
                group: Vec::new(),
                pending_second_pad: None,
                terminal_padding: false,
                symbol_count: 0,
                physical_offset: 0,
                row_symbols: 0,
                framing: FramingState::Normal,
                finished: false,
                sticky: None,
            },
        })
    }

    pub fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.state.sticky {
            return Err(error.clone());
        }
        if self.state.finished {
            return Err(Error::AlreadyFinished);
        }
        let mut out = Vec::new();
        for byte in input {
            let offset = self.state.physical_offset;
            self.state.physical_offset = self.state.physical_offset.saturating_add(1);
            if let Err(error) = decoder_scan_byte(&mut self.state, *byte, offset, &mut out) {
                self.state.sticky = Some(error.clone());
                return Err(error);
            }
        }
        Ok(out)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.state.sticky {
            return Err(error.clone());
        }
        if self.state.finished {
            return Err(Error::AlreadyFinished);
        }

        let result = (|| {
            match self.state.framing {
                FramingState::NeedLf { cr_offset } => {
                    return Err(Error::BadWrap { offset: cr_offset });
                }
                FramingState::NeedSymbol { separator_offset } => {
                    return Err(Error::BadWrap {
                        offset: separator_offset,
                    });
                }
                FramingState::Normal => {}
            }
            if let Some(offset) = self.state.pending_second_pad {
                return Err(Error::BadPadding { offset });
            }
            let len = self.state.group.len();
            if len == 0 {
                return Ok(Vec::new());
            }
            if len == 1 || self.state.options.padding == Padding::Required {
                return Err(Error::Truncated {
                    offset: self.state.group[0].offset,
                });
            }
            if len == 2 {
                if self.state.options.strict && self.state.group[1].value & 0x0f != 0 {
                    return Err(Error::NonCanonical {
                        offset: self.state.group[1].offset,
                    });
                }
                return Ok(vec![
                    (self.state.group[0].value << 2) | (self.state.group[1].value >> 4),
                ]);
            }
            if self.state.options.strict && self.state.group[2].value & 0x03 != 0 {
                return Err(Error::NonCanonical {
                    offset: self.state.group[2].offset,
                });
            }
            Ok(vec![
                (self.state.group[0].value << 2) | (self.state.group[1].value >> 4),
                (self.state.group[1].value << 4) | (self.state.group[2].value >> 2),
            ])
        })();

        match result {
            Ok(bytes) => {
                self.state.finished = true;
                Ok(bytes)
            }
            Err(error) => {
                self.state.sticky = Some(error.clone());
                Err(error)
            }
        }
    }
}

pub fn decode(input: &[u8], options: Options) -> Result<Vec<u8>, Error> {
    validate(options)?;
    let mut decoder = Decoder::new(options)?;
    let mut output = decoder.push(input)?;
    output.extend(decoder.finish()?);
    Ok(output)
}