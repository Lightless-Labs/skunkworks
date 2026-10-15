#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CsvError {
    UnexpectedQuote { record: usize, byte: usize },
    TrailingByte { record: usize, byte: usize },
    BareCr { record: usize, byte: usize },
    UnclosedQuote { record: usize, byte: usize },
    Nul { record: usize, byte: usize },
    InvalidConfig { component: ConfigPart },
    DuplicateHeader { name: String, first: usize, second: usize },
    HeaderWidth { record: usize, expected: usize, actual: usize },
    ReaderFinished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigPart {
    Delimiter,
    Quote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DuplicatePolicy {
    Reject,
    KeepFirst,
    KeepLast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    pub delimiter: u8,
    pub quote: u8,
    pub headers: bool,
    pub duplicates: DuplicatePolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    pub header: Option<Vec<String>>,
    pub rows: Vec<Vec<String>>,
    pub duplicates: DuplicatePolicy,
}

impl Table {
    pub fn column_index(&self, name: &str) -> Option<usize> {
        let header = self.header.as_ref()?;
        match self.duplicates {
            DuplicatePolicy::KeepLast => header.iter().rposition(|value| value == name),
            DuplicatePolicy::Reject | DuplicatePolicy::KeepFirst => {
                header.iter().position(|value| value == name)
            }
        }
    }
}

fn validate_config(config: Config) -> Result<(), CsvError> {
    if !(0x21..=0x7e).contains(&config.delimiter) {
        return Err(CsvError::InvalidConfig {
            component: ConfigPart::Delimiter,
        });
    }
    if !(0x21..=0x7e).contains(&config.quote) {
        return Err(CsvError::InvalidConfig {
            component: ConfigPart::Quote,
        });
    }
    if config.delimiter == config.quote {
        return Err(CsvError::InvalidConfig {
            component: ConfigPart::Quote,
        });
    }
    Ok(())
}

fn parse_with_dialect(
    input: &str,
    delimiter: u8,
    quote: u8,
) -> Result<Vec<Vec<String>>, CsvError> {
    let bytes = input.as_bytes();
    let mut i = 0;
    if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        i = 3;
    }

    let mut records = Vec::new();
    let mut fields = Vec::new();
    let mut field = Vec::new();
    let mut record_index = 0usize;
    let mut record_started = false;
    let mut field_started = false;

    #[derive(Clone, Copy)]
    enum State {
        Start,
        Unquoted,
        Quoted { opening: usize },
        Closed,
    }

    let mut state = State::Start;

    while i < bytes.len() {
        let b = bytes[i];

        if b == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: i,
            });
        }

        let outside = !matches!(state, State::Quoted { .. });
        if outside && b == b'\r' {
            if bytes.get(i + 1) != Some(&b'\n') {
                return Err(CsvError::BareCr {
                    record: record_index,
                    byte: i,
                });
            }
        }

        match state {
            State::Start => {
                if b == quote {
                    state = State::Quoted { opening: i };
                    field_started = true;
                    record_started = true;
                    i += 1;
                } else if b == delimiter {
                    fields.push(String::new());
                    record_started = true;
                    field_started = false;
                    i += 1;
                } else if b == b'\n' || b == b'\r' {
                    fields.push(String::new());
                    records.push(std::mem::take(&mut fields));
                    record_index += 1;
                    record_started = false;
                    field_started = false;
                    i += if b == b'\r' { 2 } else { 1 };
                } else {
                    field.push(b);
                    state = State::Unquoted;
                    field_started = true;
                    record_started = true;
                    i += 1;
                }
            }
            State::Unquoted => {
                if b == delimiter {
                    fields.push(String::from_utf8(std::mem::take(&mut field)).unwrap());
                    state = State::Start;
                    field_started = false;
                    record_started = true;
                    i += 1;
                } else if b == b'\n' || b == b'\r' {
                    fields.push(String::from_utf8(std::mem::take(&mut field)).unwrap());
                    records.push(std::mem::take(&mut fields));
                    state = State::Start;
                    field_started = false;
                    record_started = false;
                    record_index += 1;
                    i += if b == b'\r' { 2 } else { 1 };
                } else if b == quote {
                    return Err(CsvError::UnexpectedQuote {
                        record: record_index,
                        byte: i,
                    });
                } else {
                    field.push(b);
                    i += 1;
                }
            }
            State::Quoted { opening } => {
                if b == quote {
                    if bytes.get(i + 1) == Some(&quote) {
                        field.push(quote);
                        i += 2;
                    } else {
                        state = State::Closed;
                        i += 1;
                    }
                } else {
                    field.push(b);
                    i += 1;
                }
                let _ = opening;
            }
            State::Closed => {
                if b == delimiter {
                    fields.push(String::from_utf8(std::mem::take(&mut field)).unwrap());
                    state = State::Start;
                    field_started = false;
                    record_started = true;
                    i += 1;
                } else if b == b'\n' || b == b'\r' {
                    fields.push(String::from_utf8(std::mem::take(&mut field)).unwrap());
                    records.push(std::mem::take(&mut fields));
                    state = State::Start;
                    field_started = false;
                    record_started = false;
                    record_index += 1;
                    i += if b == b'\r' { 2 } else { 1 };
                } else {
                    return Err(CsvError::TrailingByte {
                        record: record_index,
                        byte: i,
                    });
                }
            }
        }
    }

    if let State::Quoted { opening } = state {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: opening,
        });
    }

    if record_started || field_started || !fields.is_empty() || !field.is_empty() {
        fields.push(String::from_utf8(field).unwrap());
        records.push(fields);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with_dialect(input, b',', b'"')
}

fn build_table(records: Vec<Vec<String>>, config: Config) -> Result<Table, CsvError> {
    let (header, rows) = if config.headers {
        let mut iter = records.into_iter();
        match iter.next() {
            Some(header) => (Some(header), iter.collect::<Vec<_>>()),
            None => (None, Vec::new()),
        }
    } else {
        (None, records)
    };

    if let Some(ref header_values) = header {
        if config.duplicates == DuplicatePolicy::Reject {
            let mut seen: Vec<(&str, usize)> = Vec::new();
            for (second, name) in header_values.iter().enumerate() {
                if let Some((_, first)) = seen.iter().find(|(value, _)| *value == name) {
                    return Err(CsvError::DuplicateHeader {
                        name: name.clone(),
                        first: *first,
                        second,
                    });
                }
                seen.push((name, second));
            }
        }

        for (index, row) in rows.iter().enumerate() {
            if row.len() != header_values.len() {
                return Err(CsvError::HeaderWidth {
                    record: index + 1,
                    expected: header_values.len(),
                    actual: row.len(),
                });
            }
        }
    }

    Ok(Table {
        header,
        rows,
        duplicates: config.duplicates,
    })
}

pub struct StreamReader {
    config: Config,
    input: String,
    cached: Option<Result<Table, CsvError>>,
}

impl StreamReader {
    pub fn new(config: Config) -> Result<Self, CsvError> {
        validate_config(config)?;
        Ok(Self {
            config,
            input: String::new(),
            cached: None,
        })
    }

    pub fn feed(&mut self, chunk: &str) -> Result<(), CsvError> {
        if self.cached.is_some() {
            return Err(CsvError::ReaderFinished);
        }
        self.input.push_str(chunk);
        Ok(())
    }

    pub fn finish(&mut self) -> Result<Table, CsvError> {
        if let Some(result) = &self.cached {
            return result.clone();
        }

        let result = parse_with_dialect(&self.input, self.config.delimiter, self.config.quote)
            .and_then(|records| build_table(records, self.config));
        self.cached = Some(result.clone());
        result
    }

    pub fn reset(&mut self) {
        self.input.clear();
        self.cached = None;
    }
}

pub fn read_csv(input: &str, config: Config) -> Result<Table, CsvError> {
    let mut reader = StreamReader::new(config)?;
    reader.feed(input)?;
    reader.finish()
}

fn main() {}