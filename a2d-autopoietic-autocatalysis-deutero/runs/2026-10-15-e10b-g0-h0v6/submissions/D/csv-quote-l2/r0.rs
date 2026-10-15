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

pub struct StreamReader {
    config: Config,
    input: String,
    result: Option<Result<Table, CsvError>>,
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
    let start = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        3
    } else {
        0
    };

    if start == bytes.len() {
        return Ok(Vec::new());
    }

    #[derive(Clone, Copy)]
    enum State {
        FieldStart,
        Unquoted,
        Quoted { opening: usize },
        AfterQuote,
    }

    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut state = State::FieldStart;
    let mut index = start;
    let mut record_index = 0usize;

    while index < bytes.len() {
        let byte = bytes[index];

        if byte == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: index,
            });
        }

        if !matches!(state, State::Quoted { .. }) && byte == b'\r' {
            if bytes.get(index + 1) != Some(&b'\n') {
                return Err(CsvError::BareCr {
                    record: record_index,
                    byte: index,
                });
            }

            record.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut record));
            state = State::FieldStart;
            index += 2;
            record_index += 1;
            continue;
        }

        match state {
            State::FieldStart => {
                if byte == delimiter {
                    record.push(std::mem::take(&mut field));
                    index += 1;
                } else if byte == quote {
                    state = State::Quoted { opening: index };
                    index += 1;
                } else if byte == b'\n' {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    state = State::FieldStart;
                    index += 1;
                    record_index += 1;
                } else {
                    let ch = input[index..].chars().next().unwrap();
                    field.push(ch);
                    index += ch.len_utf8();
                    state = State::Unquoted;
                }
            }
            State::Unquoted => {
                if byte == delimiter {
                    record.push(std::mem::take(&mut field));
                    state = State::FieldStart;
                    index += 1;
                } else if byte == b'\n' {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    state = State::FieldStart;
                    index += 1;
                    record_index += 1;
                } else if byte == quote {
                    return Err(CsvError::UnexpectedQuote {
                        record: record_index,
                        byte: index,
                    });
                } else {
                    let ch = input[index..].chars().next().unwrap();
                    field.push(ch);
                    index += ch.len_utf8();
                }
            }
            State::Quoted { opening } => {
                if byte == quote {
                    if bytes.get(index + 1) == Some(&quote) {
                        field.push(quote as char);
                        index += 2;
                    } else {
                        state = State::AfterQuote;
                        index += 1;
                    }
                } else {
                    let ch = input[index..].chars().next().unwrap();
                    field.push(ch);
                    index += ch.len_utf8();
                    let _ = opening;
                }
            }
            State::AfterQuote => {
                if byte == delimiter {
                    record.push(std::mem::take(&mut field));
                    state = State::FieldStart;
                    index += 1;
                } else if byte == b'\n' {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    state = State::FieldStart;
                    index += 1;
                    record_index += 1;
                } else {
                    return Err(CsvError::TrailingByte {
                        record: record_index,
                        byte: index,
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

    // A record terminator at EOF already emitted its record. Avoid adding a
    // spurious empty record while still retaining a final delimiter's field.
    if !matches!(state, State::FieldStart) || !record.is_empty() || !field.is_empty() {
        record.push(field);
        records.push(record);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with_dialect(input, b',', b'"')
}

fn table_from_records(
    records: Vec<Vec<String>>,
    config: Config,
) -> Result<Table, CsvError> {
    let mut iter = records.into_iter();
    let header = if config.headers { iter.next() } else { None };

    if let Some(header_values) = header.as_ref() {
        if config.duplicates == DuplicatePolicy::Reject {
            let mut seen: Vec<(&str, usize)> = Vec::new();
            for (second, name) in header_values.iter().enumerate() {
                if let Some((_, first)) = seen.iter().find(|(previous, _)| *previous == name) {
                    return Err(CsvError::DuplicateHeader {
                        name: name.clone(),
                        first: *first,
                        second,
                    });
                }
                seen.push((name.as_str(), second));
            }
        }

        let expected = header_values.len();
        let rows: Vec<Vec<String>> = iter.collect();
        for (offset, row) in rows.iter().enumerate() {
            if row.len() != expected {
                return Err(CsvError::HeaderWidth {
                    record: offset + 1,
                    expected,
                    actual: row.len(),
                });
            }
        }

        Ok(Table {
            header,
            rows,
            duplicates: config.duplicates,
        })
    } else {
        Ok(Table {
            header: None,
            rows: iter.collect(),
            duplicates: config.duplicates,
        })
    }
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
        if let Some(result) = self.result.as_ref() {
            return result.clone();
        }

        let result = parse_with_dialect(&self.input, self.config.delimiter, self.config.quote)
            .and_then(|records| table_from_records(records, self.config));
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