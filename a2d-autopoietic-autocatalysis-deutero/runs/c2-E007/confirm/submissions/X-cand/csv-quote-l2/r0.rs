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
        let result = parse_configured(&self.input, self.config);
        self.result = Some(result.clone());
        result
    }

    pub fn reset(&mut self) {
        self.input.clear();
        self.result = None;
    }
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_records(input, b',', b'"')
}

pub fn read_csv(input: &str, config: Config) -> Result<Table, CsvError> {
    let mut reader = StreamReader::new(config)?;
    reader.feed(input)?;
    reader.finish()
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

fn parse_configured(input: &str, config: Config) -> Result<Table, CsvError> {
    validate_config(config)?;
    let records = parse_records(input, config.delimiter, config.quote)?;

    let (header, rows) = if config.headers {
        match records.split_first() {
            None => (None, Vec::new()),
            Some((header, data_rows)) => {
                if config.duplicates == DuplicatePolicy::Reject {
                    let mut seen = std::collections::HashMap::<&str, usize>::new();
                    for (second, name) in header.iter().enumerate() {
                        if let Some(&first) = seen.get(name.as_str()) {
                            return Err(CsvError::DuplicateHeader {
                                name: name.clone(),
                                first,
                                second,
                            });
                        }
                        seen.insert(name.as_str(), second);
                    }
                }

                for (offset, row) in data_rows.iter().enumerate() {
                    if row.len() != header.len() {
                        return Err(CsvError::HeaderWidth {
                            record: offset + 1,
                            expected: header.len(),
                            actual: row.len(),
                        });
                    }
                }
                (Some(header.clone()), data_rows.to_vec())
            }
        }
    } else {
        (None, records)
    };

    Ok(Table {
        header,
        rows,
        duplicates: config.duplicates,
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    FieldStart,
    Unquoted,
    Quoted,
    AfterQuote,
}

fn parse_records(
    input: &str,
    delimiter: u8,
    quote: u8,
) -> Result<Vec<Vec<String>>, CsvError> {
    let bytes = input.as_bytes();
    let start = if input.starts_with('\u{feff}') { 3 } else { 0 };
    if start == bytes.len() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut state = State::FieldStart;
    let mut quote_start = 0usize;
    let mut record_index = 0usize;
    let mut i = start;
    let mut record_has_content = false;

    while i < bytes.len() {
        let byte = bytes[i];

        if byte == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: i,
            });
        }

        if state != State::Quoted && byte == b'\r' {
            if i + 1 >= bytes.len() || bytes[i + 1] != b'\n' {
                return Err(CsvError::BareCr {
                    record: record_index,
                    byte: i,
                });
            }
            row.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut row));
            i += 2;
            record_index += 1;
            state = State::FieldStart;
            record_has_content = false;
            continue;
        }

        if state != State::Quoted && byte == b'\n' {
            row.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut row));
            i += 1;
            record_index += 1;
            state = State::FieldStart;
            record_has_content = false;
            continue;
        }

        match state {
            State::FieldStart => {
                if byte == delimiter {
                    row.push(std::mem::take(&mut field));
                    record_has_content = true;
                    i += 1;
                } else if byte == quote {
                    quote_start = i;
                    state = State::Quoted;
                    record_has_content = true;
                    i += 1;
                } else {
                    let character = input[i..].chars().next().unwrap();
                    field.push(character);
                    i += character.len_utf8();
                    state = State::Unquoted;
                    record_has_content = true;
                }
            }
            State::Unquoted => {
                if byte == delimiter {
                    row.push(std::mem::take(&mut field));
                    state = State::FieldStart;
                    record_has_content = true;
                    i += 1;
                } else if byte == quote {
                    return Err(CsvError::UnexpectedQuote {
                        record: record_index,
                        byte: i,
                    });
                } else {
                    let character = input[i..].chars().next().unwrap();
                    field.push(character);
                    i += character.len_utf8();
                }
            }
            State::Quoted => {
                if byte == quote {
                    if i + 1 < bytes.len() && bytes[i + 1] == quote {
                        field.push(quote as char);
                        i += 2;
                    } else {
                        state = State::AfterQuote;
                        i += 1;
                    }
                } else {
                    let character = input[i..].chars().next().unwrap();
                    field.push(character);
                    i += character.len_utf8();
                }
            }
            State::AfterQuote => {
                if byte == delimiter {
                    row.push(std::mem::take(&mut field));
                    state = State::FieldStart;
                    record_has_content = true;
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

    if state == State::Quoted {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: quote_start,
        });
    }

    if record_has_content {
        row.push(field);
        records.push(row);
    }

    Ok(records)
}