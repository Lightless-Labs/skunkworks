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

fn foreign_alphabet_byte(alphabet: Alphabet, byte: u8) -> bool {
    match alphabet {
        Alphabet::Standard => byte == b'-' || byte == b'_',
        Alphabet::UrlSafe => byte == b'+' || byte == b'/',
    }
}

fn encode_digit(alphabet: Alphabet, digit: u8) -> u8 {
    match digit {
        0..=25 => b'A' + digit,
        26..=51 => b'a' + digit - 26,
        52..=61 => b'0' + digit - 52,
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

fn write_wrapped_symbol(out: &mut String, options: Options, column: &mut usize, byte: u8) {
    if let Whitespace::Wrapped { width, ending } = options.whitespace {
        if *column == width {
            match ending {
                LineEnding::Lf => out.push('\n'),
                LineEnding::CrLf => out.push_str("\r\n"),
            }
            *column = 0;
        }
        out.push(byte as char);
        *column += 1;
    } else {
        out.push(byte as char);
    }
}

fn write_encoded_group(out: &mut String, options: Options, column: &mut usize, bytes: &[u8]) {
    let a = bytes[0];
    let b = if bytes.len() > 1 { bytes[1] } else { 0 };
    let c = if bytes.len() > 2 { bytes[2] } else { 0 };
    let digits = [
        a >> 2,
        ((a & 0x03) << 4) | (b >> 4),
        ((b & 0x0f) << 2) | (c >> 6),
        c & 0x3f,
    ];
    let count = match bytes.len() {
        1 => 2,
        2 => 3,
        _ => 4,
    };

    for &digit in &digits[..count] {
        write_wrapped_symbol(out, options, column, encode_digit(options.alphabet, digit));
    }

    if bytes.len() < 3 && options.padding != Padding::Forbidden {
        for _ in count..4 {
            write_wrapped_symbol(out, options, column, b'=');
        }
    }
}

pub fn encode(input: &[u8], options: Options) -> Result<String, Error> {
    validate_options(options)?;
    let mut encoder = Encoder::new(options)?;
    let mut output = encoder.push(input)?;
    output.push_str(&encoder.finish()?);
    Ok(output)
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
    buffered: Vec<u8>,
    column: usize,
    finished: bool,
}

impl Encoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate_options(options)?;
        Ok(Self {
            options,
            buffered: Vec::with_capacity(2),
            column: 0,
            finished: false,
        })
    }

    pub fn push(&mut self, input: &[u8]) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }

        let mut output = String::new();
        let mut offset = 0;

        if !self.buffered.is_empty() {
            while self.buffered.len() < 3 && offset < input.len() {
                self.buffered.push(input[offset]);
                offset += 1;
            }
            if self.buffered.len() == 3 {
                write_encoded_group(&mut output, self.options, &mut self.column, &self.buffered);
                self.buffered.clear();
            }
        }

        while offset + 3 <= input.len() {
            write_encoded_group(
                &mut output,
                self.options,
                &mut self.column,
                &input[offset..offset + 3],
            );
            offset += 3;
        }

        self.buffered.extend_from_slice(&input[offset..]);
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<String, Error> {
        if self.finished {
            return Err(Error::AlreadyFinished);
        }

        let mut output = String::new();
        if !self.buffered.is_empty() {
            write_encoded_group(&mut output, self.options, &mut self.column, &self.buffered);
            self.buffered.clear();
        }
        self.finished = true;
        Ok(output)
    }
}

#[derive(Clone)]
struct Symbol {
    value: u8,
    offset: usize,
}

#[derive(Clone)]
enum ScanState {
    Open,
    PendingSecondPadding { offset: usize },
    FinishedPadded,
}

struct WrapState {
    column: usize,
    awaiting_lf: Option<usize>,
    needs_symbol_after_separator: Option<usize>,
}

pub struct Decoder {
    options: Options,
    offset: usize,
    symbol_count: usize,
    group: Vec<Symbol>,
    scan_state: ScanState,
    wrap: WrapState,
    sticky_error: Option<Error>,
    finished: bool,
}

impl Decoder {
    pub fn new(options: Options) -> Result<Self, Error> {
        validate_options(options)?;
        Ok(Self {
            options,
            offset: 0,
            symbol_count: 0,
            group: Vec::with_capacity(4),
            scan_state: ScanState::Open,
            wrap: WrapState {
                column: 0,
                awaiting_lf: None,
                needs_symbol_after_separator: None,
            },
            sticky_error: None,
            finished: false,
        })
    }

    fn remember_error(&mut self, error: Error) -> Error {
        self.sticky_error = Some(error.clone());
        error
    }

    fn emit_group(&mut self, out: &mut Vec<u8>) -> Result<(), Error> {
        if self.group.len() != 4 {
            return Ok(());
        }

        let a = self.group[0].value;
        let b = self.group[1].value;
        let c = self.group[2].value;
        let d = self.group[3].value;

        if c == 64 {
            if self.options.strict && (b & 0x0f) != 0 {
                return Err(Error::NonCanonical {
                    offset: self.group[1].offset,
                });
            }
            out.push((a << 2) | (b >> 4));
            self.scan_state = ScanState::FinishedPadded;
        } else if d == 64 {
            if self.options.strict && (c & 0x03) != 0 {
                return Err(Error::NonCanonical {
                    offset: self.group[2].offset,
                });
            }
            out.push((a << 2) | (b >> 4));
            out.push((b << 4) | (c >> 2));
            self.scan_state = ScanState::FinishedPadded;
        } else {
            out.push((a << 2) | (b >> 4));
            out.push((b << 4) | (c >> 2));
            out.push((c << 6) | d);
            self.scan_state = ScanState::Open;
        }

        self.group.clear();
        Ok(())
    }

    fn process_symbol(&mut self, byte: u8, at: usize, out: &mut Vec<u8>) -> Result<(), Error> {
        if let Some(limit) = self.options.max_symbols {
            if self.symbol_count >= limit {
                return Err(Error::Overlong { offset: at, limit });
            }
        }
        self.symbol_count += 1;

        if matches!(self.scan_state, ScanState::FinishedPadded) {
            return Err(Error::BadPadding { offset: at });
        }

        if let ScanState::PendingSecondPadding { .. } = self.scan_state {
            if byte != b'=' {
                return Err(Error::BadPadding { offset: at });
            }
            self.group.push(Symbol { value: 64, offset: at });
            return self.emit_group(out);
        }

        if byte == b'=' {
            match self.group.len() {
                0 | 1 => return Err(Error::BadPadding { offset: at }),
                2 => {
                    if self.options.padding == Padding::Forbidden {
                        return Err(Error::BadPadding { offset: at });
                    }
                    self.scan_state = ScanState::PendingSecondPadding { offset: at };
                    self.group.push(Symbol { value: 64, offset: at });
                    return Ok(());
                }
                3 => {
                    if self.options.padding == Padding::Forbidden {
                        return Err(Error::BadPadding { offset: at });
                    }
                    self.group.push(Symbol { value: 64, offset: at });
                    return self.emit_group(out);
                }
                _ => unreachable!(),
            }
        }

        let value = match alphabet_digit(self.options.alphabet, byte) {
            Some(value) => value,
            None if self.options.strict && foreign_alphabet_byte(self.options.alphabet, byte) => {
                return Err(Error::MixedAlphabet { offset: at });
            }
            None => return Err(Error::InvalidByte { offset: at, byte }),
        };

        self.group.push(Symbol { value, offset: at });
        if self.group.len() == 4 {
            self.emit_group(out)?;
        }
        Ok(())
    }

    fn process_wrapped_byte(&mut self, byte: u8, at: usize) -> Result<Option<u8>, Error> {
        let (width, ending) = match self.options.whitespace {
            Whitespace::Wrapped { width, ending } => (width, ending),
            _ => return Ok(Some(byte)),
        };

        if let Some(cr_offset) = self.wrap.awaiting_lf {
            if byte != b'\n' {
                return Err(Error::BadWrap { offset: at });
            }
            self.wrap.awaiting_lf = None;
            self.wrap.needs_symbol_after_separator = Some(cr_offset);
            return Ok(None);
        }

        if let Some(separator_offset) = self.wrap.needs_symbol_after_separator {
            if matches!(byte, b'\r' | b'\n' | 9 | 11 | 12 | 32) {
                return Err(Error::BadWrap { offset: at });
            }
            self.wrap.needs_symbol_after_separator = None;
            self.wrap.column = 0;
            self.wrap.column += 1;
            return Ok(Some(byte));
        }

        if self.wrap.column == width {
            let separator_start = at;
            match ending {
                LineEnding::Lf if byte == b'\n' => {
                    self.wrap.needs_symbol_after_separator = Some(separator_start);
                    return Ok(None);
                }
                LineEnding::CrLf if byte == b'\r' => {
                    self.wrap.awaiting_lf = Some(separator_start);
                    return Ok(None);
                }
                _ => return Err(Error::BadWrap { offset: at }),
            }
        }

        if matches!(byte, b'\r' | b'\n' | 9 | 11 | 12 | 32) {
            return Err(Error::BadWrap { offset: at });
        }

        self.wrap.column += 1;
        Ok(Some(byte))
    }

    pub fn push(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky_error {
            return Err(error.clone());
        }
        if self.finished {
            return Err(self.remember_error(Error::AlreadyFinished));
        }

        let mut output = Vec::new();

        for &byte in input {
            let at = self.offset;
            self.offset += 1;

            let framed = match self.options.whitespace {
                Whitespace::None => Some(byte),
                Whitespace::IgnoreAscii => {
                    if matches!(byte, 9 | 10 | 11 | 12 | 13 | 32) {
                        None
                    } else {
                        Some(byte)
                    }
                }
                Whitespace::Wrapped { .. } => match self.process_wrapped_byte(byte, at) {
                    Ok(value) => value,
                    Err(error) => return Err(self.remember_error(error)),
                },
            };

            if let Some(symbol) = framed {
                if let Err(error) = self.process_symbol(symbol, at, &mut output) {
                    return Err(self.remember_error(error));
                }
            }
        }

        Ok(output)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, Error> {
        if let Some(error) = &self.sticky_error {
            return Err(error.clone());
        }
        if self.finished {
            return Err(self.remember_error(Error::AlreadyFinished));
        }

        if let Whitespace::Wrapped { .. } = self.options.whitespace {
            if let Some(cr_offset) = self.wrap.awaiting_lf {
                return Err(self.remember_error(Error::BadWrap { offset: cr_offset }));
            }
            if let Some(separator_offset) = self.wrap.needs_symbol_after_separator {
                return Err(self.remember_error(Error::BadWrap {
                    offset: separator_offset,
                }));
            }
        }

        if let ScanState::PendingSecondPadding { offset } = self.scan_state {
            return Err(self.remember_error(Error::BadPadding { offset }));
        }

        let mut output = Vec::new();
        if !self.group.is_empty() {
            match self.options.padding {
                Padding::Required => {
                    return Err(self.remember_error(Error::Truncated {
                        offset: self.group[0].offset,
                    }));
                }
                Padding::Forbidden | Padding::Optional => match self.group.len() {
                    1 => {
                        return Err(self.remember_error(Error::Truncated {
                            offset: self.group[0].offset,
                        }));
                    }
                    2 => {
                        if self.options.strict && (self.group[1].value & 0x0f) != 0 {
                            return Err(self.remember_error(Error::NonCanonical {
                                offset: self.group[1].offset,
                            }));
                        }
                        output.push((self.group[0].value << 2) | (self.group[1].value >> 4));
                    }
                    3 => {
                        if self.options.strict && (self.group[2].value & 0x03) != 0 {
                            return Err(self.remember_error(Error::NonCanonical {
                                offset: self.group[2].offset,
                            }));
                        }
                        output.push((self.group[0].value << 2) | (self.group[1].value >> 4));
                        output.push((self.group[1].value << 4) | (self.group[2].value >> 2));
                    }
                    _ => unreachable!(),
                },
            }
        }

        self.group.clear();
        self.finished = true;
        Ok(output)
    }
}

fn main() {}