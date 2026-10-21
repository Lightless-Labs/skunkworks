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

fn parse_with(input: &str, delimiter: u8, quote: u8) -> Result<Vec<Vec<String>>, CsvError> {
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
    let mut record = Vec::new();
    let mut field = String::new();
    let mut record_index = 0usize;
    let mut field_started = false;
    let mut in_quotes = false;
    let mut after_quote = false;
    let mut quote_open = 0usize;

    while pos < bytes.len() {
        let byte = bytes[pos];

        if byte == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: pos,
            });
        }

        if !in_quotes && byte == b'\r' {
            if bytes.get(pos + 1) != Some(&b'\n') {
                return Err(CsvError::BareCr {
                    record: record_index,
                    byte: pos,
                });
            }
            record.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut record));
            pos += 2;
            record_index += 1;
            field_started = false;
            after_quote = false;
            continue;
        }

        if in_quotes {
            if byte == quote {
                if bytes.get(pos + 1) == Some(&quote) {
                    field.push(quote as char);
                    pos += 2;
                } else {
                    in_quotes = false;
                    after_quote = true;
                    pos += 1;
                }
            } else {
                let start = pos;
                while pos < bytes.len() && bytes[pos] != quote && bytes[pos] != 0 {
                    pos += 1;
                }
                field.push_str(&input[start..pos]);
            }
            continue;
        }

        if after_quote {
            if byte == delimiter {
                record.push(std::mem::take(&mut field));
                field_started = false;
                after_quote = false;
                pos += 1;
            } else if byte == b'\n' {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
                pos += 1;
                record_index += 1;
                field_started = false;
                after_quote = false;
            } else {
                return Err(CsvError::TrailingByte {
                    record: record_index,
                    byte: pos,
                });
            }
            continue;
        }

        if byte == delimiter {
            record.push(std::mem::take(&mut field));
            field_started = false;
            pos += 1;
        } else if byte == b'\n' {
            record.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut record));
            pos += 1;
            record_index += 1;
            field_started = false;
        } else if byte == quote {
            if field_started {
                return Err(CsvError::UnexpectedQuote {
                    record: record_index,
                    byte: pos,
                });
            }
            in_quotes = true;
            quote_open = pos;
            field_started = true;
            pos += 1;
        } else {
            let start = pos;
            while pos < bytes.len()
                && bytes[pos] != delimiter
                && bytes[pos] != quote
                && bytes[pos] != b'\n'
                && bytes[pos] != b'\r'
                && bytes[pos] != 0
            {
                pos += 1;
            }
            field.push_str(&input[start..pos]);
            field_started = true;
        }
    }

    if in_quotes {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: quote_open,
        });
    }

    if field_started || after_quote || !record.is_empty() {
        record.push(field);
        records.push(record);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with(input, b',', b'"')
}

fn build_table(records: Vec<Vec<String>>, config: Config) -> Result<Table, CsvError> {
    let mut records = records.into_iter();
    let mut header = None;
    let mut rows = Vec::new();

    if config.headers {
        if let Some(first) = records.next() {
            if config.duplicates == DuplicatePolicy::Reject {
                let mut seen = std::collections::HashMap::<String, usize>::new();
                for (second, name) in first.iter().enumerate() {
                    if let Some(&first_index) = seen.get(name) {
                        return Err(CsvError::DuplicateHeader {
                            name: name.clone(),
                            first: first_index,
                            second,
                        });
                    }
                    seen.insert(name.clone(), second);
                }
            }

            let expected = first.len();
            for (record_index, row) in records.enumerate() {
                let input_record = record_index + 1;
                if row.len() != expected {
                    return Err(CsvError::HeaderWidth {
                        record: input_record,
                        expected,
                        actual: row.len(),
                    });
                }
                rows.push(row);
            }
            header = Some(first);
        }
    } else {
        rows.extend(records);
    }

    Ok(Table {
        header,
        rows,
        duplicates: config.duplicates,
    })
}

fn read_with_config(input: &str, config: Config) -> Result<Table, CsvError> {
    validate_config(config)?;
    let records = parse_with(input, config.delimiter, config.quote)?;
    build_table(records, config)
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
        let result = read_with_config(&self.input, self.config);
        self.result = Some(result.clone());
        result
    }

    pub fn reset(&mut self) {
        self.input.clear();
        self.result = None;
    }
}

pub fn read_csv(input: &str, config: Config) -> Result<Table, CsvError> {
    read_with_config(input, config)
}