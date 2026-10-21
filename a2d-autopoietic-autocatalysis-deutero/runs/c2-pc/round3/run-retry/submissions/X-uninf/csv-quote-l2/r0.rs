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
            DuplicatePolicy::KeepFirst | DuplicatePolicy::Reject => {
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
    let mut i = 0usize;
    if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        i = 3;
    }

    let mut records = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();

    #[derive(Clone, Copy)]
    enum State {
        FieldStart,
        Unquoted,
        Quoted { opening: usize },
        AfterQuote,
    }

    let mut state = State::FieldStart;
    let mut record_index = 0usize;

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
            row.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut row));
            state = State::FieldStart;
            record_index += 1;
            i += 2;
            continue;
        }

        match state {
            State::FieldStart => {
                if b == delimiter {
                    row.push(String::new());
                    i += 1;
                } else if b == quote {
                    state = State::Quoted { opening: i };
                    i += 1;
                } else if b == b'\n' {
                    row.push(String::new());
                    records.push(std::mem::take(&mut row));
                    record_index += 1;
                    i += 1;
                } else {
                    let ch = input[i..].chars().next().unwrap();
                    field.push(ch);
                    state = State::Unquoted;
                    i += ch.len_utf8();
                }
            }
            State::Unquoted => {
                if b == delimiter {
                    row.push(std::mem::take(&mut field));
                    state = State::FieldStart;
                    i += 1;
                } else if b == b'\n' {
                    row.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut row));
                    state = State::FieldStart;
                    record_index += 1;
                    i += 1;
                } else if b == quote {
                    return Err(CsvError::UnexpectedQuote {
                        record: record_index,
                        byte: i,
                    });
                } else {
                    let ch = input[i..].chars().next().unwrap();
                    field.push(ch);
                    i += ch.len_utf8();
                }
            }
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
                    let _ = opening;
                }
            }
            State::AfterQuote => {
                if b == delimiter {
                    row.push(std::mem::take(&mut field));
                    state = State::FieldStart;
                    i += 1;
                } else if b == b'\n' {
                    row.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut row));
                    state = State::FieldStart;
                    record_index += 1;
                    i += 1;
                } else {
                    return Err(CsvError::TrailingByte {
                        record: record_index,
                        byte: i,
                    });
                }
            }
        }
    }

    match state {
        State::Quoted { opening } => {
            return Err(CsvError::UnclosedQuote {
                record: record_index,
                byte: opening,
            });
        }
        State::FieldStart => {
            if !row.is_empty() {
                row.push(String::new());
                records.push(row);
            }
        }
        State::Unquoted | State::AfterQuote => {
            row.push(field);
            records.push(row);
        }
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with_dialect(input, b',', b'"')
}

fn build_table(records: Vec<Vec<String>>, config: Config) -> Result<Table, CsvError> {
    let mut iter = records.into_iter();
    let header = if config.headers {
        iter.next()
    } else {
        None
    };

    if let Some(ref names) = header {
        if config.duplicates == DuplicatePolicy::Reject {
            for second in 0..names.len() {
                if let Some(first) = names[..second].iter().position(|name| name == &names[second])
                {
                    return Err(CsvError::DuplicateHeader {
                        name: names[second].clone(),
                        first,
                        second,
                    });
                }
            }
        }
    }

    let mut rows = Vec::new();
    for (offset, row) in iter.enumerate() {
        if let Some(ref names) = header {
            if row.len() != names.len() {
                return Err(CsvError::HeaderWidth {
                    record: offset + 1,
                    expected: names.len(),
                    actual: row.len(),
                });
            }
        }
        rows.push(row);
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