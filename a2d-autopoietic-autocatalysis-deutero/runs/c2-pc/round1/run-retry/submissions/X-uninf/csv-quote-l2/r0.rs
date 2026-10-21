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
            DuplicatePolicy::KeepLast => header.iter().rposition(|field| field == name),
            DuplicatePolicy::Reject | DuplicatePolicy::KeepFirst => {
                header.iter().position(|field| field == name)
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum FieldState {
    Start,
    Unquoted,
    Quoted,
    Closed,
}

fn parse_with_dialect(
    input: &str,
    delimiter: u8,
    quote: u8,
) -> Result<Vec<Vec<String>>, CsvError> {
    let bytes = input.as_bytes();
    let mut pos = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        3
    } else {
        0
    };

    if pos == bytes.len() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut state = FieldState::Start;
    let mut quote_open = 0usize;
    let mut record_index = 0usize;

    while pos < bytes.len() {
        let byte = bytes[pos];

        if byte == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: pos,
            });
        }

        if state != FieldState::Quoted && byte == b'\r' {
            if bytes.get(pos + 1) != Some(&b'\n') {
                return Err(CsvError::BareCr {
                    record: record_index,
                    byte: pos,
                });
            }
            pos += 2;
            row.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut row));
            state = FieldState::Start;
            record_index += 1;
            continue;
        }

        if state != FieldState::Quoted && byte == b'\n' {
            pos += 1;
            row.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut row));
            state = FieldState::Start;
            record_index += 1;
            continue;
        }

        match state {
            FieldState::Start => {
                if byte == delimiter {
                    row.push(std::mem::take(&mut field));
                    pos += 1;
                } else if byte == quote {
                    quote_open = pos;
                    state = FieldState::Quoted;
                    pos += 1;
                } else {
                    let ch = input[pos..].chars().next().unwrap();
                    field.push(ch);
                    state = FieldState::Unquoted;
                    pos += ch.len_utf8();
                }
            }
            FieldState::Unquoted => {
                if byte == delimiter {
                    row.push(std::mem::take(&mut field));
                    state = FieldState::Start;
                    pos += 1;
                } else if byte == quote {
                    return Err(CsvError::UnexpectedQuote {
                        record: record_index,
                        byte: pos,
                    });
                } else {
                    let ch = input[pos..].chars().next().unwrap();
                    field.push(ch);
                    pos += ch.len_utf8();
                }
            }
            FieldState::Quoted => {
                if byte == quote {
                    if bytes.get(pos + 1) == Some(&quote) {
                        field.push(quote as char);
                        pos += 2;
                    } else {
                        state = FieldState::Closed;
                        pos += 1;
                    }
                } else {
                    let ch = input[pos..].chars().next().unwrap();
                    field.push(ch);
                    pos += ch.len_utf8();
                }
            }
            FieldState::Closed => {
                if byte == delimiter {
                    row.push(std::mem::take(&mut field));
                    state = FieldState::Start;
                    pos += 1;
                } else {
                    return Err(CsvError::TrailingByte {
                        record: record_index,
                        byte: pos,
                    });
                }
            }
        }
    }

    if state == FieldState::Quoted {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: quote_open,
        });
    }

    if state != FieldState::Start || !row.is_empty() {
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
    result: Option<Result<Table, CsvError>>,
}

impl StreamReader {
    pub fn new(config: Config) -> Result<Self, CsvError> {
        validate_config(config)?;
        Ok(Self {
            config,
            input: String::new(),
            result: None,
        })
    }

    pub fn feed(&mut self, chunk: &str) -> Result<(), CsvError> {
        if self.result.is_some() {
            return Err(CsvError::ReaderFinished);
        }
        self.input.push_str(chunk);
        Ok(())
    }

    pub fn finish(&mut self) -> Result<Table, CsvError> {
        if let Some(result) = &self.result {
            return result.clone();
        }

        let result = parse_with_dialect(&self.input, self.config.delimiter, self.config.quote)
            .and_then(|records| build_table(records, self.config));
        self.result = Some(result.clone());
        result
    }

    pub fn reset(&mut self) {
        self.input.clear();
        self.result = None;
    }
}

pub fn read_csv(input: &str, config: Config) -> Result<Table, CsvError> {
    let mut reader = StreamReader::new(config)?;
    reader.feed(input)?;
    reader.finish()
}