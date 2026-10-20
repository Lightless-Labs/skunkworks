use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alphabet { Standard, UrlSafe }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Padding { Required, Forbidden, Optional }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding { Lf, CrLf }
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
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{:?}", self) }
}
impl std::error::Error for Error {}

fn valid_width(o: Options) -> Result<(), Error> {
    if let Whitespace::Wrapped { width, .. } = o.whitespace {
        if !(4..=120).contains(&width) || width % 4 != 0 {
            return Err(Error::InvalidWidth { width });
        }
    }
    Ok(())
}
fn digit(b: u8, a: Alphabet) -> Option<u8> {
    match (a, b) {
        (_, b'A'..=b'Z') => Some(b - b'A'),
        (_, b'a'..=b'z') => Some(b - b'a' + 26),
        (_, b'0'..=b'9') => Some(b - b'0' + 52),
        (Alphabet::Standard, b'+') | (Alphabet::UrlSafe, b'-') => Some(62),
        (Alphabet::Standard, b'/') | (Alphabet::UrlSafe, b'_') => Some(63),
        _ => None,
    }
}
fn symbol(n: u8, a: Alphabet) -> u8 {
    match n {
        0..=25 => b'A' + n,
        26..=51 => b'a' + n - 26,
        52..=61 => b'0' + n - 52,
        62 => if a == Alphabet::Standard { b'+' } else { b'-' },
        _ => if a == Alphabet::Standard { b'/' } else { b'_' },
    }
}
fn raw_encode(input: &[u8], o: Options) -> Vec<u8> {
    let mut out = Vec::with_capacity((input.len() + 2) / 3 * 4);
    let mut chunks = input.chunks_exact(3);
    for c in &mut chunks {
        let n = ((c[0] as u32) << 16) | ((c[1] as u32) << 8) | c[2] as u32;
        for shift in [18, 12, 6, 0] { out.push(symbol(((n >> shift) & 63) as u8, o.alphabet)); }
    }
    let r = chunks.remainder();
    if !r.is_empty() {
        let n = ((r[0] as u32) << 16) | if r.len() == 2 { (r[1] as u32) << 8 } else { 0 };
        out.push(symbol(((n >> 18) & 63) as u8, o.alphabet));
        out.push(symbol(((n >> 12) & 63) as u8, o.alphabet));
        if r.len() == 2 { out.push(symbol(((n >> 6) & 63) as u8, o.alphabet)); }
        if o.padding != Padding::Forbidden {
            if r.len() == 1 { out.extend_from_slice(b"=="); } else { out.push(b'='); }
        }
    }
    out
}
pub fn encode(input: &[u8], options: Options) -> Result<String, Error> {
    valid_width(options)?;
    let raw = raw_encode(input, options);
    let bytes = match options.whitespace {
        Whitespace::Wrapped { width, ending } => {
            let sep: &[u8] = match ending { LineEnding::Lf => b"\n", LineEnding::CrLf => b"\r\n" };
            let mut v = Vec::with_capacity(raw.len() + raw.len() / width * sep.len());
            for (i, c) in raw.chunks(width).enumerate() {
                if i != 0 { v.extend_from_slice(sep); }
                v.extend_from_slice(c);
            }
            v
        }
        _ => raw,
    };
    Ok(String::from_utf8(bytes).unwrap())
}
#[derive(Clone, Copy)]
struct Sym { b: u8, off: usize }
struct DecoderCore {
    o: Options,
    pos: usize,
    group: Vec<Sym>,
    pending: Option<Sym>,
    terminal: bool,
    row: usize,
    sep: u8,
    sep_start: usize,
    need_symbol: Option<usize>,
    offset: usize,
    failed: Option<Error>,
    closed: bool,
}
impl DecoderCore {
    fn new(o: Options) -> Self {
        Self { o, pos: 0, group: Vec::with_capacity(4), pending: None,
            terminal: false, row: 0, sep: 0, sep_start: 0, need_symbol: None,
            offset: 0, failed: None, closed: false }
    }
    fn fail<T>(&mut self, e: Error) -> Result<T, Error> {
        self.failed = Some(e.clone()); Err(e)
    }
    fn process(&mut self, b: u8, off: usize, out: &mut Vec<u8>) -> Result<(), Error> {
        if self.terminal { return self.fail(Error::BadPadding { offset: off }); }
        if let Some(p) = self.pending {
            if b != b'=' { return self.fail(Error::BadPadding { offset: off }); }
            self.pending = None;
            self.group.push(Sym { b, off });
            self.emit_group(out);
            self.terminal = true;
            return Ok(());
        }
        if b == b'=' {
            match self.group.len() {
                0 | 1 => return self.fail(Error::BadPadding { offset: off }),
                2 if self.o.padding == Padding::Forbidden =>
                    return self.fail(Error::BadPadding { offset: off }),
                2 => { self.pending = Some(Sym { b, off }); return Ok(()); }
                3 => {
                    if self.o.padding == Padding::Forbidden {
                        return self.fail(Error::BadPadding { offset: off });
                    }
                    self.group.push(Sym { b, off });
                    self.emit_group(out);
                    self.terminal = true;
                    return Ok(());
                }
                _ => return self.fail(Error::BadPadding { offset: off }),
            }
        }
        let Some(_) = digit(b, self.o.alphabet) else {
            return self.fail(Error::InvalidByte { offset: off, byte: b });
        };
        self.group.push(Sym { b, off });
        if self.group.len() == 4 { self.emit_group(out); }
        Ok(())
    }
    fn emit_group(&mut self, out: &mut Vec<u8>) {
        let vals: Vec<u8> = self.group.iter().map(|s| if s.b == b'=' { 0 } else { digit(s.b, self.o.alphabet).unwrap() }).collect();
        if vals.len() >= 2 { out.push((vals[0] << 2) | (vals[1] >> 4)); }
        if vals.len() >= 3 && self.group[2].b != b'=' { out.push((vals[1] << 4) | (vals[2] >> 2)); }
        if vals.len() >= 4 && self.group[3].b != b'=' { out.push((vals[2] << 6) | vals[3]); }
        self.group.clear();
    }
    fn physical(&mut self, b: u8, off: usize, out: &mut Vec<u8>) -> Result<(), Error> {
        match self.o.whitespace {
            Whitespace::None => self.process(b, off, out),
            Whitespace::IgnoreAscii => {
                if matches!(b, 9 | 10 | 11 | 12 | 13 | 32) { Ok(()) } else { self.process(b, off, out) }
            }
            Whitespace::Wrapped { width, ending } => {
                if self.sep == 1 {
                    if b != b'\n' { return self.fail(Error::BadWrap { offset: off }); }
                    self.sep = 2;
                    return Ok(());
                }
                if self.need_symbol.is_some() {
                    self.need_symbol = None;
                    self.row = 0;
                    if matches!(b, b'\r' | b'\n' | 9 | 11 | 12 | 32) {
                        return self.fail(Error::BadWrap { offset: off });
                    }
                } else if self.row == width {
                    match ending {
                        LineEnding::Lf if b == b'\n' => {
                            self.sep_start = off; self.row = 0; self.need_symbol = Some(off); return Ok(());
                        }
                        LineEnding::CrLf if b == b'\r' => {
                            self.sep_start = off; self.sep = 1; return Ok(());
                        }
                        _ => return self.fail(Error::BadWrap { offset: off }),
                    }
                } else if matches!(b, b'\r' | b'\n' | 9 | 11 | 12 | 32) {
                    return self.fail(Error::BadWrap { offset: off });
                }
                self.row += 1;
                self.process(b, off, out)
            }
        }
    }
    fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if let Some(e) = &self.failed { return Err(e.clone()); }
        if self.closed { return Err(Error::AlreadyFinished); }
        let mut out = Vec::new();
        for &b in input {
            let off = self.offset;
            self.offset += 1;
            if let Err(e) = self.physical(b, off, &mut out) { return Err(e); }
        }
        Ok(out)
    }
    fn finish(&mut self) -> Result<Vec<u8>, Error> {
        if let Some(e) = &self.failed { return Err(e.clone()); }
        if self.closed { return Err(Error::AlreadyFinished); }
        let mut out = Vec::new();
        if self.sep == 1 { return self.fail(Error::BadWrap { offset: self.sep_start }); }
        if self.need_symbol.is_some() { return self.fail(Error::BadWrap { offset: self.sep_start }); }
        if self.pending.is_some() {
            return self.fail(Error::BadPadding { offset: self.pending.unwrap().off });
        }
        match self.group.len() {
            0 => {}
            1 => return self.fail(Error::Truncated { offset: self.group[0].off }),
            2 | 3 if self.o.padding != Padding::Required => self.emit_group(&mut out),
            _ => return self.fail(Error::Truncated { offset: self.group[0].off }),
        }
        self.closed = true;
        Ok(out)
    }
}
pub struct Encoder {
    o: Options, tail: Vec<u8>, emitted: usize, closed: bool,
}
impl Encoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        valid_width(options)?;
        Ok(Self { o: options, tail: Vec::new(), emitted: 0, closed: false })
    }
    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.closed { return Err(Error::AlreadyFinished); }
        let mut all = std::mem::take(&mut self.tail);
        all.extend_from_slice(input);
        let complete = all.len() / 3 * 3;
        let raw = raw_encode(&all[..complete], self.o);
        let mut s = String::from_utf8(raw).unwrap();
        if let Whitespace::Wrapped { width, ending } = self.o.whitespace {
            let sep = match ending { LineEnding::Lf => "\n", LineEnding::CrLf => "\r\n" };
            for _ in 0..s.len() {
                if self.emitted > 0 && self.emitted % width == 0 { /* handled below */ }
                self.emitted += 1;
            }
            let prev = self.emitted - s.len();
            let mut wrapped = String::new();
            for (i, c) in s.bytes().enumerate() {
                let n = prev + i;
                if n > 0 && n % width == 0 { wrapped.push_str(sep); }
                wrapped.push(c as char);
            }
            s = wrapped;
        }
        self.tail.extend_from_slice(&all[complete..]);
        Ok(s)
    }
    pub fn finish(&mut self) -> Result<String, Error> {
        if self.closed { return Err(Error::AlreadyFinished); }
        let s = raw_encode(&self.tail, self.o);
        self.tail.clear();
        self.closed = true;
        let mut s = String::from_utf8(s).unwrap();
        if let Whitespace::Wrapped { width, ending } = self.o.whitespace {
            let sep = match ending { LineEnding::Lf => "\n", LineEnding::CrLf => "\r\n" };
            let prev = self.emitted;
            let mut wrapped = String::new();
            for (i, c) in s.bytes().enumerate() {
                let n = prev + i;
                if n > 0 && n % width == 0 { wrapped.push_str(sep); }
                wrapped.push(c as char);
            }
            self.emitted += s.len();
            s = wrapped;
        }
        Ok(s)
    }
}
pub struct Decoder { core: DecoderCore }
impl Decoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        valid_width(options)?;
        Ok(Self { core: DecoderCore::new(options) })
    }
    pub fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> { self.core.push(input) }
    pub fn finish(&mut self) -> Result<Vec<u8>, Error> { self.core.finish() }
}
pub fn decode(input: &[u8], options: Options) -> Result<Vec<u8>, Error> {
    let mut d = Decoder::new(options)?;
    let mut out = d.push(input)?;
    out.extend(d.finish()?);
    Ok(out)
}
