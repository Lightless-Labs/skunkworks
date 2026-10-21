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
            DuplicatePolicy::KeepLast => header.iter().rposition(|column| column == name),
            DuplicatePolicy::Reject | DuplicatePolicy::KeepFirst => {
                header.iter().position(|column| column == name)
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
    let mut i = if input.starts_with('\u{feff}') { 3 } else { 0 };
    let mut records = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut record_index = 0usize;
    let mut record_started = false;

    #[derive(Clone, Copy)]
    enum State {
        FieldStart,
        Unquoted,
        Quoted { opening: usize },
        AfterQuote,
    }

    let mut state = State::FieldStart;

    while i < bytes.len() {
        let b = bytes[i];

        if b == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: i,
            });
        }

        let outside_quotes = !matches!(state, State::Quoted { .. });
        if outside_quotes && b == b'\r' && bytes.get(i + 1) != Some(&b'\n') {
            return Err(CsvError::BareCr {
                record: record_index,
                byte: i,
            });
        }

        match state {
            State::Quoted { opening } => {
                if b == quote {
                    if bytes.get(i + 1) == Some(&quote) {
                        field.push(quote as char);
                        i += 2;
                    } else {
                        state = State::AfterQuote;
                        i += 1;
                    }
                } else {
                    let ch = input[i..].chars().next().unwrap();
                    field.push(ch);
                    i += ch.len_utf8();
                }
                record_started = true;
                let _ = opening;
            }
            State::FieldStart => {
                if b == delimiter {
                    row.push(String::new());
                    record_started = true;
                    i += 1;
                } else if b == quote {
                    state = State::Quoted { opening: i };
                    record_started = true;
                    i += 1;
                } else if b == b'\n' {
                    row.push(String::new());
                    records.push(std::mem::take(&mut row));
                    record_index += 1;
                    record_started = false;
                    i += 1;
                } else if b == b'\r' {
                    row.push(String::new());
                    records.push(std::mem::take(&mut row));
                    record_index += 1;
                    record_started = false;
                    i += 2;
                } else {
                    let ch = input[i..].chars().next().unwrap();
                    field.push(ch);
                    state = State::Unquoted;
                    record_started = true;
                    i += ch.len_utf8();
                }
            }
            State::Unquoted => {
                if b == quote {
                    return Err(CsvError::UnexpectedQuote {
                        record: record_index,
                        byte: i,
                    });
                } else if b == delimiter {
                    row.push(std::mem::take(&mut field));
                    state = State::FieldStart;
                    record_started = true;
                    i += 1;
                } else if b == b'\n' {
                    row.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut row));
                    record_index += 1;
                    state = State::FieldStart;
                    record_started = false;
                    i += 1;
                } else if b == b'\r' {
                    row.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut row));
                    record_index += 1;
                    state = State::FieldStart;
                    record_started = false;
                    i += 2;
                } else {
                    let ch = input[i..].chars().next().unwrap();
                    field.push(ch);
                    record_started = true;
                    i += ch.len_utf8();
                }
            }
            State::AfterQuote => {
                if b == delimiter {
                    row.push(std::mem::take(&mut field));
                    state = State::FieldStart;
                    record_started = true;
                    i += 1;
                } else if b == b'\n' {
                    row.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut row));
                    record_index += 1;
                    state = State::FieldStart;
                    record_started = false;
                    i += 1;
                } else if b == b'\r' {
                    row.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut row));
                    record_index += 1;
                    state = State::FieldStart;
                    record_started = false;
                    i += 2;
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

    if record_started || !row.is_empty() || !field.is_empty() {
        row.push(field);
        records.push(row);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with_dialect(input, b',', b'"')
}

fn build_table(records: Vec<Vec<String>>, config: Config) -> Result<Table, CsvError> {
    if !config.headers {
        return Ok(Table {
            header: None,
            rows: records,
            duplicates: config.duplicates,
        });
    }

    let mut iter = records.into_iter();
    let Some(header) = iter.next() else {
        return Ok(Table {
            header: None,
            rows: Vec::new(),
            duplicates: config.duplicates,
        });
    };

    if config.duplicates == DuplicatePolicy::Reject {
        let mut seen = std::collections::HashMap::<String, usize>::new();
        for (second, name) in header.iter().enumerate() {
            if let Some(&first) = seen.get(name) {
                return Err(CsvError::DuplicateHeader {
                    name: name.clone(),
                    first,
                    second,
                });
            }
            seen.insert(name.clone(), second);
        }
    }

    let expected = header.len();
    let mut rows = Vec::new();
    for (record, row) in iter.enumerate() {
        if row.len() != expected {
            return Err(CsvError::HeaderWidth {
                record: record + 1,
                expected,
                actual: row.len(),
            });
        }
        rows.push(row);
    }

    Ok(Table {
        header: Some(header),
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