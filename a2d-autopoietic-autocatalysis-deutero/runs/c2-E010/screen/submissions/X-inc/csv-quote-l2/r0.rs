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
    let offset = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        3
    } else {
        0
    };

    if offset == bytes.len() {
        return Ok(Vec::new());
    }

    const FIELD_START: u8 = 0;
    const UNQUOTED: u8 = 1;
    const QUOTED: u8 = 2;
    const AFTER_QUOTE: u8 = 3;

    let mut records = Vec::new();
    let mut row = Vec::new();
    let mut field = Vec::new();
    let mut state = FIELD_START;
    let mut quote_start = 0usize;
    let mut record_index = 0usize;
    let mut i = offset;

    while i < bytes.len() {
        let byte = bytes[i];

        if byte == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: i,
            });
        }

        if state != QUOTED && byte == b'\r' {
            if bytes.get(i + 1) != Some(&b'\n') {
                return Err(CsvError::BareCr {
                    record: record_index,
                    byte: i,
                });
            }
            row.push(String::from_utf8(std::mem::take(&mut field)).unwrap());
            records.push(std::mem::take(&mut row));
            state = FIELD_START;
            i += 2;
            record_index += 1;
            continue;
        }

        if state != QUOTED && byte == b'\n' {
            row.push(String::from_utf8(std::mem::take(&mut field)).unwrap());
            records.push(std::mem::take(&mut row));
            state = FIELD_START;
            i += 1;
            record_index += 1;
            continue;
        }

        match state {
            FIELD_START => {
                if byte == delimiter {
                    row.push(String::new());
                } else if byte == quote {
                    quote_start = i;
                    state = QUOTED;
                } else {
                    field.push(byte);
                    state = UNQUOTED;
                }
            }
            UNQUOTED => {
                if byte == delimiter {
                    row.push(String::from_utf8(std::mem::take(&mut field)).unwrap());
                    state = FIELD_START;
                } else if byte == quote {
                    return Err(CsvError::UnexpectedQuote {
                        record: record_index,
                        byte: i,
                    });
                } else {
                    field.push(byte);
                }
            }
            QUOTED => {
                if byte == quote {
                    if bytes.get(i + 1) == Some(&quote) {
                        field.push(quote);
                        i += 1;
                    } else {
                        state = AFTER_QUOTE;
                    }
                } else {
                    field.push(byte);
                }
            }
            AFTER_QUOTE => {
                if byte == delimiter {
                    row.push(String::from_utf8(std::mem::take(&mut field)).unwrap());
                    state = FIELD_START;
                } else {
                    return Err(CsvError::TrailingByte {
                        record: record_index,
                        byte: i,
                    });
                }
            }
            _ => unreachable!(),
        }
        i += 1;
    }

    if state == QUOTED {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: quote_start,
        });
    }

    if !row.is_empty() || !field.is_empty() || state != FIELD_START {
        row.push(String::from_utf8(field).unwrap());
        records.push(row);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with_dialect(input, b',', b'"')
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

        let result = (|| {
            let mut records =
                parse_with_dialect(&self.input, self.config.delimiter, self.config.quote)?;
            let mut header = None;

            if self.config.headers && !records.is_empty() {
                header = Some(records.remove(0));
            }

            if let Some(columns) = &header {
                if self.config.duplicates == DuplicatePolicy::Reject {
                    let mut seen = std::collections::HashMap::<&str, usize>::new();
                    for (second, name) in columns.iter().enumerate() {
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

                for (index, row) in records.iter().enumerate() {
                    if row.len() != columns.len() {
                        return Err(CsvError::HeaderWidth {
                            record: index + 1,
                            expected: columns.len(),
                            actual: row.len(),
                        });
                    }
                }
            }

            Ok(Table {
                header,
                rows: records,
                duplicates: self.config.duplicates,
            })
        })();

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