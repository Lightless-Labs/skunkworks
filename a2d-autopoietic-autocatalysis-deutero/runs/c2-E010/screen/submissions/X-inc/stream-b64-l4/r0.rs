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

fn is_ignored(byte: u8) -> bool {
    matches!(byte, 9 | 10 | 11 | 12 | 13 | 32)
}

fn digit(byte: u8, alphabet: Alphabet, strict: bool, offset: usize) -> Result<u8, Error> {
    let value = match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' if alphabet == Alphabet::Standard => Some(62),
        b'/' if alphabet == Alphabet::Standard => Some(63),
        b'-' if alphabet == Alphabet::UrlSafe => Some(62),
        b'_' if alphabet == Alphabet::UrlSafe => Some(63),
        b'+' | b'/' | b'-' | b'_' if strict => {
            return Err(Error::MixedAlphabet { offset });
        }
        _ => None,
    };
    value.ok_or(Error::InvalidByte { offset, byte })
}

fn encode_digit(v: u8, alphabet: Alphabet) -> u8 {
    match v {
        0..=25 => b'A' + v,
        26..=51 => b'a' + (v - 26),
        52..=61 => b'0' + (v - 52),
        62 => if alphabet == Alphabet::Standard { b'+' } else { b'-' },
        _ => if alphabet == Alphabet::Standard { b'/' } else { b'_' },
    }
}

fn raw_encode(input: &[u8], options: Options) -> Vec<u8> {
    let mut out = Vec::new();
    let mut chunks = input.chunks_exact(3);
    for c in &mut chunks {
        let n = ((c[0] as u32) << 16) | ((c[1] as u32) << 8) | c[2] as u32;
        for shift in [18, 12, 6, 0] {
            out.push(encode_digit(((n >> shift) & 63) as u8, options.alphabet));
        }
    }
    let rem = chunks.remainder();
    if !rem.is_empty() {
        let a = rem[0];
        let b = if rem.len() == 2 { rem[1] } else { 0 };
        out.push(encode_digit(a >> 2, options.alphabet));
        out.push(encode_digit(((a & 3) << 4) | (b >> 4), options.alphabet));
        if rem.len() == 2 {
            out.push(encode_digit((b & 15) << 2, options.alphabet));
            if options.padding != Padding::Forbidden {
                out.push(b'=');
            }
        } else if options.padding != Padding::Forbidden {
            out.extend_from_slice(b"==");
        }
    }
    out
}

fn layout(raw: &[u8], whitespace: Whitespace) -> Vec<u8> {
    let Whitespace::Wrapped { width, ending } = whitespace else {
        return raw.to_vec();
    };
    let mut out = Vec::with_capacity(raw.len() + raw.len() / width * 2);
    for (i, &b) in raw.iter().enumerate() {
        if i != 0 && i % width == 0 {
            match ending {
                LineEnding::Lf => out.push(b'\n'),
                LineEnding::CrLf => out.extend_from_slice(b"\r\n"),
            }
        }
        out.push(b);
    }
    out
}

pub fn encode(input: &[u8], options: Options) -> Result<String, Error> {
    validate(options)?;
    let raw = raw_encode(input, options);
    Ok(String::from_utf8(layout(&raw, options.whitespace)).unwrap())
}

pub struct Encoder {
    options: Options,
    buffered: Vec<u8>,
    symbols_in_row: usize,
    closed: bool,
}

impl Encoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            buffered: Vec::new(),
            symbols_in_row: 0,
            closed: false,
        })
    }

    fn emit_symbols(&mut self, symbols: &[u8], out: &mut Vec<u8>) {
        for &b in symbols {
            if let Whitespace::Wrapped { width, ending } = self.options.whitespace {
                if self.symbols_in_row == width {
                    match ending {
                        LineEnding::Lf => out.push(b'\n'),
                        LineEnding::CrLf => out.extend_from_slice(b"\r\n"),
                    }
                    self.symbols_in_row = 0;
                }
                self.symbols_in_row += 1;
            }
            out.push(b);
        }
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        self.buffered.extend_from_slice(input);
        let complete = self.buffered.len() / 3 * 3;
        if complete == 0 {
            return Ok(String::new());
        }
        let bytes: Vec<u8> = self.buffered.drain(..complete).collect();
        let mut raw = Vec::with_capacity(bytes.len() / 3 * 4);
        for c in bytes.chunks_exact(3) {
            let n = ((c[0] as u32) << 16) | ((c[1] as u32) << 8) | c[2] as u32;
            for shift in [18, 12, 6, 0] {
                raw.push(encode_digit(((n >> shift) & 63) as u8, self.options.alphabet));
            }
        }
        let mut out = Vec::new();
        self.emit_symbols(&raw, &mut out);
        Ok(String::from_utf8(out).unwrap())
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        self.closed = true;
        let raw = raw_encode(&self.buffered, self.options);
        self.buffered.clear();
        let mut out = Vec::new();
        self.emit_symbols(&raw, &mut out);
        Ok(String::from_utf8(out).unwrap())
    }
}

#[derive(Clone, Copy)]
enum FrameState {
    Row,
    NeedLf { cr_offset: usize, sep_offset: usize },
    NeedSymbol { sep_offset: usize },
}

pub struct Decoder {
    options: Options,
    offset: usize,
    symbols: usize,
    group: Vec<(u8, usize, u8)>,
    pending_second_pad: Option<usize>,
    padded: bool,
    closed: bool,
    sticky: Option<Error>,
    row_symbols: usize,
    frame: FrameState,
}

impl Decoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            offset: 0,
            symbols: 0,
            group: Vec::new(),
            pending_second_pad: None,
            padded: false,
            closed: false,
            sticky: None,
            row_symbols: 0,
            frame: FrameState::Row,
        })
    }

    fn fail<T>(&mut self, error: Error) -> Result<T, Error> {
        self.sticky = Some(error.clone());
        Err(error)
    }

    fn frame_byte(&mut self, byte: u8, offset: usize) -> Result<bool, Error> {
        match self.options.whitespace {
            Whitespace::None => Ok(true),
            Whitespace::IgnoreAscii => Ok(!is_ignored(byte)),
            Whitespace::Wrapped { width, ending } => match self.frame {
                FrameState::NeedLf { cr_offset, sep_offset } => {
                    if byte != b'\n' {
                        return Err(Error::BadWrap { offset });
                    }
                    self.frame = FrameState::NeedSymbol { sep_offset };
                    let _ = cr_offset;
                    Ok(false)
                }
                FrameState::NeedSymbol { .. } => {
                    if is_ignored(byte) {
                        Err(Error::BadWrap { offset })
                    } else {
                        self.frame = FrameState::Row;
                        self.row_symbols = 0;
                        Ok(true)
                    }
                }
                FrameState::Row => {
                    if self.row_symbols == width {
                        let sep_offset = offset;
                        match ending {
                            LineEnding::Lf if byte == b'\n' => {
                                self.frame = FrameState::NeedSymbol { sep_offset };
                                Ok(false)
                            }
                            LineEnding::CrLf if byte == b'\r' => {
                                self.frame = FrameState::NeedLf {
                                    cr_offset: offset,
                                    sep_offset,
                                };
                                Ok(false)
                            }
                            _ => Err(Error::BadWrap { offset }),
                        }
                    } else if is_ignored(byte) {
                        Err(Error::BadWrap { offset })
                    } else {
                        self.row_symbols += 1;
                        Ok(true)
                    }
                }
            },
        }
    }

    fn process_symbol(&mut self, byte: u8, offset: usize, out: &mut Vec<u8>) -> Result<(), Error> {
        if let Some(limit) = self.options.max_symbols {
            if self.symbols >= limit {
                return Err(Error::Overlong { offset, limit });
            }
        }
        self.symbols += 1;

        if self.padded {
            return Err(Error::BadPadding { offset });
        }
        if let Some(pad_offset) = self.pending_second_pad.take() {
            if byte != b'=' {
                return Err(Error::BadPadding { offset });
            }
            if self.options.padding == Padding::Forbidden {
                return Err(Error::BadPadding { offset: pad_offset });
            }
            let vals = &self.group;
            let second = vals[1].2;
            if self.options.strict && second & 15 != 0 {
                return Err(Error::NonCanonical { offset: vals[1].1 });
            }
            out.push((vals[0].2 << 2) | (second >> 4));
            self.group.clear();
            self.padded = true;
            return Ok(());
        }

        if byte == b'=' {
            match self.group.len() {
                0 | 1 => return Err(Error::BadPadding { offset }),
                2 => {
                    if self.options.padding == Padding::Forbidden {
                        return Err(Error::BadPadding { offset });
                    }
                    self.pending_second_pad = Some(offset);
                    return Ok(());
                }
                3 => {
                    if self.options.padding == Padding::Forbidden {
                        return Err(Error::BadPadding { offset });
                    }
                    let third = self.group[2].2;
                    if self.options.strict && third & 3 != 0 {
                        return Err(Error::NonCanonical { offset: self.group[2].1 });
                    }
                    out.push((self.group[0].2 << 2) | (self.group[1].2 >> 4));
                    out.push((self.group[1].2 << 4) | (third >> 2));
                    self.group.clear();
                    self.padded = true;
                    return Ok(());
                }
                _ => unreachable!(),
            }
        }

        if self.group.len() == 3 {
            let d = digit(byte, self.options.alphabet, self.options.strict, offset)?;
            let a = self.group[0].2;
            let b = self.group[1].2;
            let c = self.group[2].2;
            out.extend_from_slice(&[
                (a << 2) | (b >> 4),
                (b << 4) | (c >> 2),
                (c << 6) | d,
            ]);
            self.group.clear();
        } else {
            let d = digit(byte, self.options.alphabet, self.options.strict, offset)?;
            self.group.push((byte, offset, d));
        }
        Ok(())
    }

    pub fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if let Some(e) = &self.sticky {
            return Err(e.clone());
        }
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        let mut out = Vec::new();
        for &byte in input {
            let offset = self.offset;
            self.offset += 1;
            let symbol = match self.frame_byte(byte, offset) {
                Ok(v) => v,
                Err(e) => return self.fail(e),
            };
            if !symbol {
                continue;
            }
            if let Err(e) = self.process_symbol(byte, offset, &mut out) {
                return self.fail(e);
            }
        }
        Ok(out)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, Error> {
        if let Some(e) = &self.sticky {
            return Err(e.clone());
        }
        if self.closed {
            return Err(Error::AlreadyFinished);
        }
        if let Whitespace::Wrapped { ending, .. } = self.options.whitespace {
            match self.frame {
                FrameState::NeedLf { cr_offset, .. } => {
                    return self.fail(Error::BadWrap { offset: cr_offset });
                }
                FrameState::NeedSymbol { sep_offset } => {
                    return self.fail(Error::BadWrap { offset: sep_offset });
                }
                FrameState::Row => {
                    let _ = ending;
                }
            }
        }
        if let Some(offset) = self.pending_second_pad {
            return self.fail(Error::BadPadding { offset });
        }

        let mut out = Vec::new();
        match self.group.len() {
            0 => {}
            1 => return self.fail(Error::Truncated { offset: self.group[0].1 }),
            2 | 3 if self.options.padding != Padding::Required => {
                if self.options.strict {
                    let (idx, mask) = if self.group.len() == 2 { (1, 15) } else { (2, 3) };
                    if self.group[idx].2 & mask != 0 {
                        return self.fail(Error::NonCanonical { offset: self.group[idx].1 });
                    }
                }
                let a = self.group[0].2;
                let b = self.group[1].2;
                out.push((a << 2) | (b >> 4));
                if self.group.len() == 3 {
                    out.push((b << 4) | (self.group[2].2 >> 2));
                }
            }
            _ => return self.fail(Error::Truncated { offset: self.group[0].1 }),
        }
        self.closed = true;
        Ok(out)
    }
}

pub fn decode(input: &[u8], options: Options) -> Result<Vec<u8>, Error> {
    let mut decoder = Decoder::new(options)?;
    let mut out = decoder.push(input)?;
    out.extend(decoder.finish()?);
    Ok(out)
}