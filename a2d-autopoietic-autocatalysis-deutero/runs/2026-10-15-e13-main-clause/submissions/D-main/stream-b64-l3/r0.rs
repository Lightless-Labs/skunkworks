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

fn encode_quad(bytes: &[u8], padding: Padding, alphabet: Alphabet) -> Vec<u8> {
    let table = match alphabet {
        Alphabet::Standard => b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/",
        Alphabet::UrlSafe => b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_",
    };
    let a = bytes[0];
    let b = if bytes.len() > 1 { bytes[1] } else { 0 };
    let c = if bytes.len() > 2 { bytes[2] } else { 0 };

    let mut out = vec![
        table[(a >> 2) as usize],
        table[(((a & 3) << 4) | (b >> 4)) as usize],
    ];

    if bytes.len() > 1 {
        out.push(table[(((b & 15) << 2) | (c >> 6)) as usize]);
    } else if padding != Padding::Forbidden {
        out.push(b'=');
    }

    if bytes.len() > 2 {
        out.push(table[(c & 63) as usize]);
    } else if padding != Padding::Forbidden {
        out.push(b'=');
    }

    out
}

pub struct Encoder {
    options: Options,
    tail: Vec<u8>,
    column: usize,
    finished: bool,
}

impl Encoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            tail: Vec::new(),
            column: 0,
            finished: false,
        })
    }

    fn emit(&mut self, symbols: &[u8], out: &mut String) {
        for &symbol in symbols {
            if let Whitespace::Wrapped { width, ending } = self.options.whitespace {
                if self.column == width {
                    match ending {
                        LineEnding::Lf => out.push('\n'),
                        LineEnding::CrLf => out.push_str("\r\n"),
                    }
                    self.column = 0;
                }
                self.column += 1;
            }
            out.push(symbol as char);
        }
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        let mut out = String::new();
        for &byte in input {
            self.tail.push(byte);
            if self.tail.len() == 3 {
                let symbols = encode_quad(&self.tail, self.options.padding, self.options.alphabet);
                self.emit(&symbols, &mut out);
                self.tail.clear();
            }
        }
        Ok(out)
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }
        let mut out = String::new();
        if !self.tail.is_empty() {
            let symbols = encode_quad(&self.tail, self.options.padding, self.options.alphabet);
            self.emit(&symbols, &mut out);
            self.tail.clear();
        }
        self.finished = true;
        Ok(out)
    }
}

pub fn encode(input: &[u8], options: Options) -> Result<String, Error> {
    let mut encoder = Encoder::new(options)?;
    let mut result = encoder.push(input)?;
    result.push_str(&encoder.finish()?);
    Ok(result)
}

#[derive(Clone, Copy)]
enum FrameState {
    Normal,
    NeedLf { cr_offset: usize },
    NeedSymbol { start_offset: usize },
}

pub struct Decoder {
    options: Options,
    group: Vec<(u8, usize)>,
    pending_eq: Option<usize>,
    terminal: bool,
    offset: usize,
    frame_state: FrameState,
    column: usize,
    finished: bool,
    sticky: Option<Error>,
}

impl Decoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate(options)?;
        Ok(Self {
            options,
            group: Vec::with_capacity(4),
            pending_eq: None,
            terminal: false,
            offset: 0,
            frame_state: FrameState::Normal,
            column: 0,
            finished: false,
            sticky: None,
        })
    }

    fn sticky_result<T>(&self) -> Option<Result<T, Error>> {
        self.sticky.clone().map(Err)
    }

    fn scan_symbol(&mut self, byte: u8, offset: usize, out: &mut Vec<u8>) -> Result<(), Error> {
        if self.terminal {
            return Err(Error::BadPadding { offset });
        }

        if let Some(eq_offset) = self.pending_eq {
            if byte != b'=' {
                return Err(Error::BadPadding { offset });
            }
            self.pending_eq = None;
            self.group.push((byte, offset));
            self.complete_group(out)?;
            let _ = eq_offset;
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
                    self.pending_eq = Some(offset);
                    self.group.push((byte, offset));
                    return Ok(());
                }
                3 => {
                    self.group.push((byte, offset));
                    self.complete_group(out)?;
                    return Ok(());
                }
                _ => unreachable!(),
            }
        }

        let value = digit(byte, self.options.alphabet)
            .ok_or(Error::InvalidByte { offset, byte })?;
        self.group.push((value, offset));
        if self.group.len() == 4 {
            self.complete_group(out)?;
        }
        Ok(())
    }

    fn complete_group(&mut self, out: &mut Vec<u8>) -> Result<(), Error> {
        let g = &self.group;
        if g.len() != 4 {
            return Ok(());
        }
        let (a, _) = g[0];
        let (b, _) = g[1];
        if g[2].0 == b'=' {
            if g[3].0 != b'=' {
                return Err(Error::BadPadding { offset: g[3].1 });
            }
            out.push((a << 2) | (b >> 4));
            self.terminal = true;
        } else if g[3].0 == b'=' {
            let c = g[2].0;
            out.push((a << 2) | (b >> 4));
            out.push((b << 4) | (c >> 2));
            self.terminal = true;
        } else {
            let c = g[2].0;
            let d = g[3].0;
            out.push((a << 2) | (b >> 4));
            out.push((b << 4) | (c >> 2));
            out.push((c << 6) | d);
        }
        self.group.clear();
        Ok(())
    }

    fn frame_byte(&mut self, byte: u8, offset: usize) -> Result<bool, Error> {
        match self.options.whitespace {
            Whitespace::None => Ok(true),
            Whitespace::IgnoreAscii => Ok(!matches!(byte, 9 | 10 | 11 | 12 | 13 | 32)),
            Whitespace::Wrapped { width, ending } => {
                match self.frame_state {
                    FrameState::NeedLf { cr_offset } => {
                        if byte != b'\n' {
                            return Err(Error::BadWrap { offset });
                        }
                        self.frame_state = FrameState::NeedSymbol {
                            start_offset: cr_offset,
                        };
                        Ok(false)
                    }
                    FrameState::NeedSymbol { start_offset } => {
                        self.frame_state = FrameState::Normal;
                        if matches!(byte, 9 | 10 | 11 | 12 | 13 | 32) {
                            return Err(Error::BadWrap { offset });
                        }
                        self.column = 0;
                        let _ = start_offset;
                        Ok(true)
                    }
                    FrameState::Normal => {
                        if self.column == width {
                            match ending {
                                LineEnding::Lf => {
                                    if byte != b'\n' {
                                        return Err(Error::BadWrap { offset });
                                    }
                                    self.column = 0;
                                    self.frame_state = FrameState::NeedSymbol {
                                        start_offset: offset,
                                    };
                                    Ok(false)
                                }
                                LineEnding::CrLf => {
                                    if byte != b'\r' {
                                        return Err(Error::BadWrap { offset });
                                    }
                                    self.frame_state = FrameState::NeedLf {
                                        cr_offset: offset,
                                    };
                                    Ok(false)
                                }
                            }
                        } else if matches!(byte, 9 | 10 | 11 | 12 | 13 | 32) {
                            Err(Error::BadWrap { offset })
                        } else {
                            self.column += 1;
                            Ok(true)
                        }
                    }
                }
            }
        }
    }

    pub fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if let Some(result) = self.sticky_result() {
            return result;
        }
        if self.finished {
            return Err(Error::AlreadyFinished);
        }

        let mut out = Vec::new();
        for &byte in input {
            let offset = self.offset;
            self.offset += 1;
            let result = self
                .frame_byte(byte, offset)
                .and_then(|is_symbol| {
                    if is_symbol {
                        self.scan_symbol(byte, offset, &mut out)
                    } else {
                        Ok(())
                    }
                });
            if let Err(error) = result {
                self.sticky = Some(error.clone());
                return Err(error);
            }
        }
        Ok(out)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, Error> {
        if let Some(result) = self.sticky_result() {
            return result;
        }
        if self.finished {
            return Err(Error::AlreadyFinished);
        }

        let error = match self.frame_state {
            FrameState::NeedLf { cr_offset } => Some(Error::BadWrap { offset: cr_offset }),
            FrameState::NeedSymbol { start_offset } => {
                Some(Error::BadWrap { offset: start_offset })
            }
            FrameState::Normal => None,
        };

        if let Some(error) = error {
            self.sticky = Some(error.clone());
            return Err(error);
        }

        if let Some(offset) = self.pending_eq {
            let error = Error::BadPadding { offset };
            self.sticky = Some(error.clone());
            return Err(error);
        }

        if !self.group.is_empty() {
            let first_offset = self.group[0].1;
            let residual = self.group.len();
            let may_be_unpadded = matches!(
                self.options.padding,
                Padding::Forbidden | Padding::Optional
            );
            if may_be_unpadded && (residual == 2 || residual == 3) {
                let a = self.group[0].0;
                let b = self.group[1].0;
                let mut out = vec![(a << 2) | (b >> 4)];
                if residual == 3 {
                    let c = self.group[2].0;
                    out.push((b << 4) | (c >> 2));
                }
                self.group.clear();
                self.finished = true;
                return Ok(out);
            }
            let error = Error::Truncated {
                offset: first_offset,
            };
            self.sticky = Some(error.clone());
            return Err(error);
        }

        self.finished = true;
        Ok(Vec::new())
    }
}

pub fn decode(input: &[u8], options: Options) -> Result<Vec<u8>, Error> {
    let mut decoder = Decoder::new(options)?;
    let mut result = decoder.push(input)?;
    result.extend(decoder.finish()?);
    Ok(result)
}

fn main() {}