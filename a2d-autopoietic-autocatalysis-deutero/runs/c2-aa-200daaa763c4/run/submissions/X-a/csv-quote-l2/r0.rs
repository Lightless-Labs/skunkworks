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
    let mut i = if input.starts_with('\u{feff}') { 3 } else { 0 };

    let mut records = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut record_index = 0usize;
    let mut in_quotes = false;
    let mut after_quote = false;
    let mut quote_open = 0usize;
    let mut field_started = false;
    let mut record_started = false;

    while i < bytes.len() {
        let byte = bytes[i];

        if byte == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: i,
            });
        }

        if !in_quotes && byte == b'\r' {
            if bytes.get(i + 1) != Some(&b'\n') {
                return Err(CsvError::BareCr {
                    record: record_index,
                    byte: i,
                });
            }
            row.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut row));
            record_index += 1;
            i += 2;
            after_quote = false;
            field_started = false;
            record_started = false;
            continue;
        }

        if in_quotes {
            if byte == quote {
                if bytes.get(i + 1) == Some(&quote) {
                    field.push(quote as char);
                    i += 2;
                } else {
                    in_quotes = false;
                    after_quote = true;
                    i += 1;
                }
            } else {
                let ch = input[i..].chars().next().expect("valid UTF-8");
                field.push(ch);
                i += ch.len_utf8();
            }
            continue;
        }

        if after_quote {
            if byte == delimiter {
                row.push(std::mem::take(&mut field));
                after_quote = false;
                field_started = false;
                record_started = true;
                i += 1;
            } else if byte == b'\n' {
                row.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut row));
                record_index += 1;
                after_quote = false;
                field_started = false;
                record_started = false;
                i += 1;
            } else {
                return Err(CsvError::TrailingByte {
                    record: record_index,
                    byte: i,
                });
            }
            continue;
        }

        if byte == delimiter {
            row.push(std::mem::take(&mut field));
            field_started = false;
            record_started = true;
            i += 1;
        } else if byte == b'\n' {
            row.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut row));
            record_index += 1;
            field_started = false;
            record_started = false;
            i += 1;
        } else if byte == quote {
            if !field_started {
                in_quotes = true;
                quote_open = i;
                field_started = true;
                record_started = true;
                i += 1;
            } else {
                return Err(CsvError::UnexpectedQuote {
                    record: record_index,
                    byte: i,
                });
            }
        } else {
            let ch = input[i..].chars().next().expect("valid UTF-8");
            field.push(ch);
            field_started = true;
            record_started = true;
            i += ch.len_utf8();
        }
    }

    if in_quotes {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: quote_open,
        });
    }

    if record_started || field_started || !row.is_empty() || after_quote {
        row.push(field);
        records.push(row);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with_dialect(input, b',', b'"')
}

fn make_table(records: Vec<Vec<String>>, config: Config) -> Result<Table, CsvError> {
    if !config.headers {
        return Ok(Table {
            header: None,
            rows: records,
            duplicates: config.duplicates,
        });
    }

    let mut records = records.into_iter();
    let Some(header) = records.next() else {
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

    let expected = header.len();
    let mut rows = Vec::new();
    for (index, row) in records.enumerate() {
        if row.len() != expected {
            return Err(CsvError::HeaderWidth {
                record: index + 1,
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
            .and_then(|records| make_table(records, self.config));
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