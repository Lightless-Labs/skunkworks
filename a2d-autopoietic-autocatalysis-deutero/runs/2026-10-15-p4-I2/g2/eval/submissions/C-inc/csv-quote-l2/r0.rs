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
    let start = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        3
    } else {
        0
    };

    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut index = start;
    let mut record_index = 0usize;
    let mut record_started = false;
    let mut field_started = false;
    let mut state = 0u8; // 0: field start, 1: unquoted, 2: quoted, 3: after quote
    let mut quote_open = 0usize;

    while index < bytes.len() {
        let byte = bytes[index];

        if byte == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: index,
            });
        }

        if state != 2 && byte == b'\r' && bytes.get(index + 1) != Some(&b'\n') {
            return Err(CsvError::BareCr {
                record: record_index,
                byte: index,
            });
        }

        match state {
            0 => {
                if byte == delimiter {
                    record.push(std::mem::take(&mut field));
                    record_started = true;
                    field_started = false;
                    index += 1;
                } else if byte == quote {
                    state = 2;
                    quote_open = index;
                    record_started = true;
                    field_started = true;
                    index += 1;
                } else if byte == b'\n' || byte == b'\r' {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    record_started = false;
                    field_started = false;
                    state = 0;
                    index += if byte == b'\r' { 2 } else { 1 };
                } else {
                    let ch = input[index..].chars().next().unwrap();
                    field.push(ch);
                    state = 1;
                    record_started = true;
                    field_started = true;
                    index += ch.len_utf8();
                }
            }
            1 => {
                if byte == delimiter {
                    record.push(std::mem::take(&mut field));
                    state = 0;
                    record_started = true;
                    field_started = false;
                    index += 1;
                } else if byte == quote {
                    return Err(CsvError::UnexpectedQuote {
                        record: record_index,
                        byte: index,
                    });
                } else if byte == b'\n' || byte == b'\r' {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    record_started = false;
                    field_started = false;
                    state = 0;
                    index += if byte == b'\r' { 2 } else { 1 };
                } else {
                    let ch = input[index..].chars().next().unwrap();
                    field.push(ch);
                    index += ch.len_utf8();
                }
            }
            2 => {
                if byte == quote {
                    if bytes.get(index + 1) == Some(&quote) {
                        field.push(quote as char);
                        index += 2;
                    } else {
                        state = 3;
                        index += 1;
                    }
                } else {
                    let ch = input[index..].chars().next().unwrap();
                    field.push(ch);
                    index += ch.len_utf8();
                }
            }
            _ => {
                if byte == delimiter {
                    record.push(std::mem::take(&mut field));
                    state = 0;
                    record_started = true;
                    field_started = false;
                    index += 1;
                } else if byte == b'\n' || byte == b'\r' {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    record_index += 1;
                    record_started = false;
                    field_started = false;
                    state = 0;
                    index += if byte == b'\r' { 2 } else { 1 };
                } else {
                    return Err(CsvError::TrailingByte {
                        record: record_index,
                        byte: index,
                    });
                }
            }
        }
    }

    if state == 2 {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: quote_open,
        });
    }

    if record_started || field_started || !record.is_empty() {
        record.push(field);
        records.push(record);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with_dialect(input, b',', b'"')
}

fn build_table(records: Vec<Vec<String>>, config: Config) -> Result<Table, CsvError> {
    let mut header = None;
    let mut rows = records;

    if config.headers && !rows.is_empty() {
        let parsed_header = rows.remove(0);

        if config.duplicates == DuplicatePolicy::Reject {
            let mut seen: Vec<(String, usize)> = Vec::new();
            for (second, name) in parsed_header.iter().enumerate() {
                if let Some((_, first)) = seen.iter().find(|(seen_name, _)| seen_name == name) {
                    return Err(CsvError::DuplicateHeader {
                        name: name.clone(),
                        first: *first,
                        second,
                    });
                }
                seen.push((name.clone(), second));
            }
        }

        for (offset, row) in rows.iter().enumerate() {
            if row.len() != parsed_header.len() {
                return Err(CsvError::HeaderWidth {
                    record: offset + 1,
                    expected: parsed_header.len(),
                    actual: row.len(),
                });
            }
        }

        header = Some(parsed_header);
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

fn main() {}