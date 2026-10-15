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

fn parse_csv_with(
    input: &str,
    delimiter: u8,
    quote: u8,
) -> Result<Vec<Vec<String>>, CsvError> {
    let bytes = input.as_bytes();
    let mut i = if input.starts_with('\u{feff}') { 3 } else { 0 };
    let mut records = Vec::new();

    if i == bytes.len() {
        return Ok(records);
    }

    let mut record = Vec::new();
    let mut field = String::new();
    let mut state = 0u8; // 0: field start, 1: unquoted, 2: quoted, 3: after quote
    let mut quote_open = 0usize;
    let mut record_index = 0usize;
    let mut record_started = false;

    while i < bytes.len() {
        let b = bytes[i];

        if b == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: i,
            });
        }

        if state != 2 && b == b'\r' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
                state = 0;
                record_started = false;
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
            0 => {
                if b == delimiter {
                    record.push(String::new());
                    record_started = true;
                    i += 1;
                } else if b == quote {
                    quote_open = i;
                    state = 2;
                    record_started = true;
                    i += 1;
                } else if b == b'\n' {
                    record.push(String::new());
                    records.push(std::mem::take(&mut record));
                    state = 0;
                    record_started = false;
                    record_index += 1;
                    i += 1;
                } else {
                    let start = i;
                    while i < bytes.len()
                        && bytes[i] != delimiter
                        && bytes[i] != quote
                        && bytes[i] != b'\n'
                        && bytes[i] != b'\r'
                        && bytes[i] != 0
                    {
                        i += 1;
                    }
                    field.push_str(&input[start..i]);
                    if i > start {
                        state = 1;
                        record_started = true;
                    }
                }
            }
            1 => {
                if b == quote {
                    return Err(CsvError::UnexpectedQuote {
                        record: record_index,
                        byte: i,
                    });
                } else if b == delimiter {
                    record.push(std::mem::take(&mut field));
                    state = 0;
                    record_started = true;
                    i += 1;
                } else if b == b'\n' {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    state = 0;
                    record_started = false;
                    record_index += 1;
                    i += 1;
                } else {
                    let start = i;
                    while i < bytes.len()
                        && bytes[i] != delimiter
                        && bytes[i] != quote
                        && bytes[i] != b'\n'
                        && bytes[i] != b'\r'
                        && bytes[i] != 0
                    {
                        i += 1;
                    }
                    field.push_str(&input[start..i]);
                }
            }
            2 => {
                if b == quote {
                    if i + 1 < bytes.len() && bytes[i + 1] == quote {
                        field.push(quote as char);
                        i += 2;
                    } else {
                        state = 3;
                        i += 1;
                    }
                } else {
                    let start = i;
                    while i < bytes.len() && bytes[i] != quote && bytes[i] != 0 {
                        i += 1;
                    }
                    field.push_str(&input[start..i]);
                }
            }
            3 => {
                if b == delimiter {
                    record.push(std::mem::take(&mut field));
                    state = 0;
                    record_started = true;
                    i += 1;
                } else if b == b'\n' {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    state = 0;
                    record_started = false;
                    record_index += 1;
                    i += 1;
                } else {
                    return Err(CsvError::TrailingByte {
                        record: record_index,
                        byte: i,
                    });
                }
            }
            _ => unreachable!(),
        }
    }

    if state == 2 {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: quote_open,
        });
    }

    if record_started || !record.is_empty() || state != 0 {
        record.push(field);
        records.push(record);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_csv_with(input, b',', b'"')
}

fn parse_table(input: &str, config: Config) -> Result<Table, CsvError> {
    validate_config(config)?;
    let mut records = parse_csv_with(input, config.delimiter, config.quote)?;

    let (header, rows) = if config.headers && !records.is_empty() {
        let header = records.remove(0);
        (Some(header), records)
    } else {
        (None, records)
    };

    if let Some(ref header_values) = header {
        if config.duplicates == DuplicatePolicy::Reject {
            let mut seen: Vec<(&str, usize)> = Vec::new();
            for (second, name) in header_values.iter().enumerate() {
                if let Some((_, first)) = seen.iter().find(|(seen_name, _)| *seen_name == name) {
                    return Err(CsvError::DuplicateHeader {
                        name: name.clone(),
                        first: *first,
                        second,
                    });
                }
                seen.push((name, second));
            }
        }

        for (offset, row) in rows.iter().enumerate() {
            if row.len() != header_values.len() {
                return Err(CsvError::HeaderWidth {
                    record: offset + 1,
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
        if self.result.is_none() {
            self.result = Some(parse_table(&self.input, self.config));
        }
        self.result.as_ref().unwrap().clone()
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