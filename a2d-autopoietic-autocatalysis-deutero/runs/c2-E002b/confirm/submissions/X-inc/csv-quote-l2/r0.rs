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
    cached: Option<Result<Table, CsvError>>,
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
    let start = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        3
    } else {
        0
    };
    if start == bytes.len() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut i = start;
    let mut record = 0usize;
    let mut in_quotes = false;
    let mut after_quote = false;
    let mut quote_open = 0usize;
    let mut field_started = false;

    while i < bytes.len() {
        let b = bytes[i];

        if b == 0 {
            return Err(CsvError::Nul { record, byte: i });
        }

        if !in_quotes && b == b'\r' {
            if bytes.get(i + 1) == Some(&b'\n') {
                fields.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut fields));
                i += 2;
                record += 1;
                after_quote = false;
                field_started = false;
                continue;
            }
            return Err(CsvError::BareCr { record, byte: i });
        }

        if in_quotes {
            if b == quote {
                if bytes.get(i + 1) == Some(&quote) {
                    field.push(quote as char);
                    i += 2;
                    continue;
                }
                in_quotes = false;
                after_quote = true;
                i += 1;
                continue;
            }
            let ch = input[i..].chars().next().unwrap();
            field.push(ch);
            i += ch.len_utf8();
            continue;
        }

        if after_quote {
            if b == delimiter {
                fields.push(std::mem::take(&mut field));
                after_quote = false;
                field_started = false;
                i += 1;
                continue;
            }
            if b == b'\n' {
                fields.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut fields));
                record += 1;
                after_quote = false;
                field_started = false;
                i += 1;
                continue;
            }
            return Err(CsvError::TrailingByte { record, byte: i });
        }

        if b == delimiter {
            fields.push(std::mem::take(&mut field));
            field_started = false;
            i += 1;
            continue;
        }
        if b == b'\n' {
            fields.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut fields));
            record += 1;
            field_started = false;
            i += 1;
            continue;
        }
        if b == quote {
            if field_started {
                return Err(CsvError::UnexpectedQuote { record, byte: i });
            }
            in_quotes = true;
            quote_open = i;
            field_started = true;
            i += 1;
            continue;
        }

        let ch = input[i..].chars().next().unwrap();
        field.push(ch);
        field_started = true;
        i += ch.len_utf8();
    }

    if in_quotes {
        return Err(CsvError::UnclosedQuote {
            record,
            byte: quote_open,
        });
    }

    if after_quote || field_started || !field.is_empty() || !fields.is_empty() {
        fields.push(field);
        records.push(fields);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with(input, b',', b'"')
}

fn make_table(records: Vec<Vec<String>>, config: Config) -> Result<Table, CsvError> {
    let mut iter = records.into_iter();
    let header = if config.headers { iter.next() } else { None };

    if let Some(ref columns) = header {
        if config.duplicates == DuplicatePolicy::Reject {
            let mut seen: Vec<(&str, usize)> = Vec::new();
            for (second, name) in columns.iter().enumerate() {
                if let Some((_, first)) = seen.iter().find(|(prior, _)| *prior == name) {
                    return Err(CsvError::DuplicateHeader {
                        name: name.clone(),
                        first: *first,
                        second,
                    });
                }
                seen.push((name, second));
            }
        }
    }

    let mut rows = Vec::new();
    for (record_index, row) in iter.enumerate() {
        if let Some(ref columns) = header {
            if row.len() != columns.len() {
                return Err(CsvError::HeaderWidth {
                    record: record_index + 1,
                    expected: columns.len(),
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
        let result = parse_with(&self.input, self.config.delimiter, self.config.quote)
            .and_then(|records| make_table(records, self.config));
        self.cached = Some(result.clone());
        result
    }

    pub fn reset(&mut self) {
        self.input.clear();
        self.cached = None;
    }
}

pub fn read_csv(input: &str, config: Config) -> Result<Table, CsvError> {
    validate_config(config)?;
    let records = parse_with(input, config.delimiter, config.quote)?;
    make_table(records, config)
}