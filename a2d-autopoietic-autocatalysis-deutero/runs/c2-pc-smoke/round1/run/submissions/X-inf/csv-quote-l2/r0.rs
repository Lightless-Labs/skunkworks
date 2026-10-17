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

#[derive(Clone, Copy, PartialEq, Eq)]
enum ParseState {
    FieldStart,
    Unquoted,
    Quoted,
    AfterQuote,
}

fn parse_with_dialect(
    input: &str,
    delimiter: u8,
    quote: u8,
) -> Result<Vec<Vec<String>>, CsvError> {
    let bytes = input.as_bytes();
    let mut i = if input.starts_with('\u{feff}') { 3 } else { 0 };
    if i == bytes.len() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = Vec::new();
    let mut state = ParseState::FieldStart;
    let mut quote_start = 0usize;
    let mut record_index = 0usize;

    while i < bytes.len() {
        let byte = bytes[i];

        if byte == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: i,
            });
        }

        if state != ParseState::Quoted && byte == b'\r' {
            if bytes.get(i + 1) == Some(&b'\n') {
                record.push(String::from_utf8(field).expect("CSV field is valid UTF-8"));
                records.push(record);
                record = Vec::new();
                field = Vec::new();
                state = ParseState::FieldStart;
                record_index += 1;
                i += 2;
                continue;
            }
            return Err(CsvError::BareCr {
                record: record_index,
                byte: i,
            });
        }

        match state {
            ParseState::FieldStart => {
                if byte == delimiter {
                    record.push(String::new());
                    i += 1;
                } else if byte == quote {
                    quote_start = i;
                    state = ParseState::Quoted;
                    i += 1;
                } else if byte == b'\n' {
                    record.push(String::new());
                    records.push(record);
                    record = Vec::new();
                    field = Vec::new();
                    state = ParseState::FieldStart;
                    record_index += 1;
                    i += 1;
                } else {
                    field.push(byte);
                    state = ParseState::Unquoted;
                    i += 1;
                }
            }
            ParseState::Unquoted => {
                if byte == delimiter {
                    record.push(String::from_utf8(field).expect("CSV field is valid UTF-8"));
                    field = Vec::new();
                    state = ParseState::FieldStart;
                    i += 1;
                } else if byte == b'\n' {
                    record.push(String::from_utf8(field).expect("CSV field is valid UTF-8"));
                    records.push(record);
                    record = Vec::new();
                    field = Vec::new();
                    state = ParseState::FieldStart;
                    record_index += 1;
                    i += 1;
                } else if byte == quote {
                    return Err(CsvError::UnexpectedQuote {
                        record: record_index,
                        byte: i,
                    });
                } else {
                    field.push(byte);
                    i += 1;
                }
            }
            ParseState::Quoted => {
                if byte == quote {
                    if bytes.get(i + 1) == Some(&quote) {
                        field.push(quote);
                        i += 2;
                    } else {
                        state = ParseState::AfterQuote;
                        i += 1;
                    }
                } else {
                    field.push(byte);
                    i += 1;
                }
            }
            ParseState::AfterQuote => {
                if byte == delimiter {
                    record.push(String::from_utf8(field).expect("CSV field is valid UTF-8"));
                    field = Vec::new();
                    state = ParseState::FieldStart;
                    i += 1;
                } else if byte == b'\n' {
                    record.push(String::from_utf8(field).expect("CSV field is valid UTF-8"));
                    records.push(record);
                    record = Vec::new();
                    field = Vec::new();
                    state = ParseState::FieldStart;
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

    if state == ParseState::Quoted {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: quote_start,
        });
    }

    if state == ParseState::FieldStart {
        if !record.is_empty() {
            record.push(String::new());
            records.push(record);
        }
    } else {
        record.push(String::from_utf8(field).expect("CSV field is valid UTF-8"));
        records.push(record);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with_dialect(input, b',', b'"')
}

fn build_table(
    records: Vec<Vec<String>>,
    config: Config,
) -> Result<Table, CsvError> {
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
        for second in 0..header.len() {
            if let Some(first) = header[..second]
                .iter()
                .position(|name| name == &header[second])
            {
                return Err(CsvError::DuplicateHeader {
                    name: header[second].clone(),
                    first,
                    second,
                });
            }
        }
    }

    let mut rows = Vec::new();
    for (record_index, row) in iter.enumerate() {
        let input_record = record_index + 1;
        if row.len() != header.len() {
            return Err(CsvError::HeaderWidth {
                record: input_record,
                expected: header.len(),
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