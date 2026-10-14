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

fn value(byte: u8, alphabet: Alphabet, strict: bool, offset: usize) -> Result<u8, Error> {
    let v = match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' if alphabet == Alphabet::Standard => Some(62),
        b'/' if alphabet == Alphabet::Standard => Some(63),
        b'-' if alphabet == Alphabet::UrlSafe => Some(62),
        b'_' if alphabet == Alphabet::UrlSafe => Some(63),
        b'+' | b'/' if alphabet == Alphabet::UrlSafe && strict => {
            return Err(Error::MixedAlphabet { offset });
        }
        b'-' | b'_' if alphabet == Alphabet::Standard && strict => {
            return Err(Error::MixedAlphabet { offset });
        }
        _ => None,
    };
    v.ok_or(Error::InvalidByte { offset, byte })
}

fn digit(v: u8, alphabet: Alphabet) -> u8 {
    match v {
        0..=25 => b'A' + v,
        26..=51 => b'a' + (v - 26),
        52..=61 => b'0' + (v - 52),
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

pub fn encode(input: &[u8], options: Options) -> Result<String, Error> {
    let mut encoder = Encoder::new(options)?;
    let mut result = encoder.push(input)?;
    result.push_str(&encoder.finish()?);
    Ok(result)
}

pub fn decode(input: &[u8], options: Options) -> Result<Vec<u8>, Error> {
    let mut decoder = Decoder::new(options)?;
    let mut result = decoder.push(input)?;
    result.extend(decoder.finish()?);
    Ok(result)
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

    fn emit_full(&mut self, out: &mut String, chunk: &[u8]) {
        let a = chunk[0];
        let b = chunk[1];
        let c = chunk[2];
        let values = [
            a >> 2,
            ((a & 0x03) << 4) | (b >> 4),
            ((b & 0x0f) << 2) | (c >> 6),
            c & 0x3f,
        ];
        for v in values {
            self.emit_symbol(out, digit(v, self.options.alphabet));
        }
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        let mut out = String::new();
        let mut pos = 0;

        if !self.tail.is_empty() {
            while self.tail.len() < 3 && pos < input.len() {
                self.tail.push(input[pos]);
                pos += 1;
            }
            if self.tail.len() == 3 {
                let chunk = self.tail.clone();
                self.emit_full(&mut out, &chunk);
                self.tail.clear();
            }
        }

        while pos + 3 <= input.len() {
            self.emit_full(&mut out, &input[pos..pos + 3]);
            pos += 3;
        }
        self.tail.extend_from_slice(&input[pos..]);
        Ok(out)
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        self.closed = true;
        let mut out = String::new();
        if self.tail.len() == 1 {
            let a = self.tail[0];
            self.emit_symbol(&mut out, digit(a >> 2, self.options.alphabet));
            self.emit_symbol(
                &mut out,
                digit((a & 0x03) << 4, self.options.alphabet),
            );
            if self.options.padding != Padding::Forbidden {
                self.emit_symbol(&mut out, b'=');
                self.emit_symbol(&mut out, b'=');
            }
        } else if self.tail.len() == 2 {
            let a = self.tail[0];
            let b = self.tail[1];
            self.emit_symbol(&mut out, digit(a >> 2, self.options.alphabet));
            self.emit_symbol(
                &mut out,
                digit(((a & 0x03) << 4) | (b >> 4), self.options.alphabet),
            );
            self.emit_symbol(
                &mut out,
                digit((b & 0x0f) << 2, self.options.alphabet),
            );
            if self.options.padding != Padding::Forbidden {
                self.emit_symbol(&mut out, b'=');
            }
        }
        self.tail.clear();
        Ok(out)
    }
}

#[derive(Clone, Copy)]
struct Symbol {
    byte: u8,
    value: u8,
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
    offset: usize,
    symbols_seen: usize,
    group: Vec<Symbol>,
    pending_second_pad: Option<usize>,
    terminal_padding: bool,
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
            symbols_seen: 0,
            group: Vec::with_capacity(4),
            pending_second_pad: None,
            terminal_padding: false,
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

    fn is_ignored(&self, byte: u8) -> bool {
        self.options.whitespace == Whitespace::IgnoreAscii
            && matches!(byte, 9 | 10 | 11 | 12 | 13 | 32)
    }

    fn complete_group(&mut self, out: &mut Vec<u8>) -> Result<(), Error> {
        let g = &self.group;
        if g.len() != 4 {
            return Ok(());
        }

        if g[2].byte == b'=' {
            if self.options.strict && (g[1].value & 0x0f) != 0 {
                return Err(Error::NonCanonical { offset: g[1].offset });
            }
            out.push((g[0].value << 2) | (g[1].value >> 4));
            self.terminal_padding = true;
        } else if g[3].byte == b'=' {
            if self.options.strict && (g[2].value & 0x03) != 0 {
                return Err(Error::NonCanonical { offset: g[2].offset });
            }
            out.push((g[0].value << 2) | (g[1].value >> 4));
            out.push((g[1].value << 4) | (g[2].value >> 2));
            self.terminal_padding = true;
        } else {
            out.push((g[0].value << 2) | (g[1].value >> 4));
            out.push((g[1].value << 4) | (g[2].value >> 2));
            out.push((g[2].value << 6) | g[3].value);
        }
        self.group.clear();
        self.pending_second_pad = None;
        Ok(())
    }

    fn process_symbol(&mut self, byte: u8, offset: usize, out: &mut Vec<u8>) -> Result<(), Error> {
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

        if self.group.is_empty() || self.group.len() == 1 {
            if byte == b'=' {
                return Err(Error::BadPadding { offset });
            }
        } else if self.group.len() == 2 {
            if byte == b'=' {
                if self.options.padding == Padding::Forbidden {
                    return Err(Error::BadPadding { offset });
                }
                self.pending_second_pad = Some(offset);
                self.group.push(Symbol {
                    byte,
                    value: 0,
                    offset,
                });
                return Ok(());
            }
        } else if self.group.len() == 3 {
            if self.group[2].byte == b'=' {
                if byte != b'=' {
                    return Err(Error::BadPadding { offset });
                }
                self.group.push(Symbol {
                    byte,
                    value: 0,
                    offset,
                });
                self.complete_group(out)?;
                return Ok(());
            }
            if byte == b'=' {
                if self.options.padding == Padding::Forbidden {
                    return Err(Error::BadPadding { offset });
                }
                self.group.push(Symbol {
                    byte,
                    value: 0,
                    offset,
                });
                self.complete_group(out)?;
                return Ok(());
            }
        }

        if byte == b'=' {
            return Err(Error::BadPadding { offset });
        }
        let v = value(byte, self.options.alphabet, self.options.strict, offset)?;
        self.group.push(Symbol { byte, value: v, offset });
        if self.group.len() == 4 {
            self.complete_group(out)?;
        }
        Ok(())
    }

    fn frame_byte(&mut self, byte: u8, offset: usize) -> Result<bool, Error> {
        match self.options.whitespace {
            Whitespace::None => Ok(true),
            Whitespace::IgnoreAscii => Ok(!self.is_ignored(byte)),
            Whitespace::Wrapped { width, ending } => {
                match self.frame_state {
                    FrameState::NeedLf { cr_offset } => {
                        if byte != b'\n' {
                            return Err(Error::BadWrap { offset });
                        }
                        self.frame_state = FrameState::NeedSymbol {
                            separator_offset: cr_offset,
                        };
                        return Ok(false);
                    }
                    FrameState::NeedSymbol { .. } => {
                        self.frame_state = FrameState::Normal;
                        self.row_symbols = 0;
                    }
                    FrameState::Normal => {}
                }

                if self.row_symbols == width {
                    match ending {
                        LineEnding::Lf if byte == b'\n' => {
                            self.frame_state = FrameState::NeedSymbol {
                                separator_offset: offset,
                            };
                            return Ok(false);
                        }
                        LineEnding::CrLf if byte == b'\r' => {
                            self.frame_state = FrameState::NeedLf { cr_offset: offset };
                            return Ok(false);
                        }
                        _ => return Err(Error::BadWrap { offset }),
                    }
                } else if matches!(byte, 9 | 10 | 11 | 12 | 13 | 32) {
                    return Err(Error::BadWrap { offset });
                }

                self.row_symbols += 1;
                Ok(true)
            }
        }
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

            let symbol = match self.frame_byte(byte, offset) {
                Ok(symbol) => symbol,
                Err(error) => return Err(self.fail(error)),
            };
            if !symbol {
                continue;
            }

            if let Err(error) = self.process_symbol(byte, offset, &mut out) {
                return Err(self.fail(error));
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

        if let Some(offset) = self.pending_second_pad {
            return Err(self.fail(Error::BadPadding { offset }));
        }

        let mut out = Vec::new();
        match self.group.len() {
            0 => {}
            1 => {
                let offset = self.group[0].offset;
                return Err(self.fail(Error::Truncated { offset }));
            }
            2 if self.options.padding != Padding::Required => {
                if self.options.strict && (self.group[1].value & 0x0f) != 0 {
                    let error = Error::NonCanonical {
                        offset: self.group[1].offset,
                    };
                    return Err(self.fail(error));
                }
                out.push((self.group[0].value << 2) | (self.group[1].value >> 4));
            }
            3 if self.options.padding != Padding::Required => {
                if self.options.strict && (self.group[2].value & 0x03) != 0 {
                    let error = Error::NonCanonical {
                        offset: self.group[2].offset,
                    };
                    return Err(self.fail(error));
                }
                out.push((self.group[0].value << 2) | (self.group[1].value >> 4));
                out.push((self.group[1].value << 4) | (self.group[2].value >> 2));
            }
            _ => {
                let offset = self.group[0].offset;
                return Err(self.fail(Error::Truncated { offset }));
            }
        }

        self.group.clear();
        self.closed = true;
        Ok(out)
    }
}