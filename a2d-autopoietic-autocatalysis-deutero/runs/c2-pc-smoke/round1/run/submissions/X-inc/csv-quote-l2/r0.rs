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
    let mut i = if input.starts_with('\u{feff}') { 3 } else { 0 };
    let mut records = Vec::new();
    if i == bytes.len() {
        return Ok(records);
    }

    let mut record = 0usize;
    let mut fields = Vec::new();
    let mut field = Vec::new();
    let mut at_field_start = true;
    let mut quoted = false;
    let mut after_quote = false;
    let mut quote_start = 0usize;

    while i < bytes.len() {
        let b = bytes[i];

        if b == 0 {
            return Err(CsvError::Nul { record, byte: i });
        }

        if quoted {
            if b == quote {
                if i + 1 < bytes.len() && bytes[i + 1] == quote {
                    field.push(quote);
                    i += 2;
                } else {
                    quoted = false;
                    after_quote = true;
                    i += 1;
                }
            } else {
                field.push(b);
                i += 1;
            }
            continue;
        }

        if b == b'\r' && (i + 1 >= bytes.len() || bytes[i + 1] != b'\n') {
            return Err(CsvError::BareCr { record, byte: i });
        }

        if after_quote {
            if b == delimiter {
                fields.push(String::from_utf8(field).unwrap());
                field = Vec::new();
                at_field_start = true;
                after_quote = false;
                i += 1;
            } else if b == b'\n' || b == b'\r' {
                fields.push(String::from_utf8(field).unwrap());
                field = Vec::new();
                records.push(fields);
                fields = Vec::new();
                record += 1;
                at_field_start = true;
                after_quote = false;
                i += if b == b'\r' { 2 } else { 1 };
            } else {
                return Err(CsvError::TrailingByte { record, byte: i });
            }
            continue;
        }

        if b == delimiter {
            fields.push(String::from_utf8(field).unwrap());
            field = Vec::new();
            at_field_start = true;
            i += 1;
        } else if b == b'\n' || b == b'\r' {
            fields.push(String::from_utf8(field).unwrap());
            field = Vec::new();
            records.push(fields);
            fields = Vec::new();
            record += 1;
            at_field_start = true;
            i += if b == b'\r' { 2 } else { 1 };
        } else if b == quote {
            if at_field_start {
                quoted = true;
                quote_start = i;
                at_field_start = false;
                i += 1;
            } else {
                return Err(CsvError::UnexpectedQuote { record, byte: i });
            }
        } else {
            field.push(b);
            at_field_start = false;
            i += 1;
        }
    }

    if quoted {
        return Err(CsvError::UnclosedQuote {
            record,
            byte: quote_start,
        });
    }

    if after_quote || !at_field_start || !fields.is_empty() || !field.is_empty() {
        fields.push(String::from_utf8(field).unwrap());
        records.push(fields);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with(input, b',', b'"')
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
            let mut seen: std::collections::HashMap<&str, usize> =
                std::collections::HashMap::new();
            for (second, name) in names.iter().enumerate() {
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
    }

    let mut rows = Vec::new();
    for (record_index, row) in iter.enumerate() {
        if let Some(ref names) = header {
            if row.len() != names.len() {
                return Err(CsvError::HeaderWidth {
                    record: record_index + 1,
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
        let result = parse_with(&self.input, self.config.delimiter, self.config.quote)
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