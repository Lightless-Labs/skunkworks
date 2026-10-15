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
    let mut i = 0;
    if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        i = 3;
    }

    if i == bytes.len() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    let mut row = Vec::new();
    let mut field = Vec::new();
    let mut state = ParseState::FieldStart;
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

        if state != ParseState::Quoted && b == b'\r' {
            if i + 1 >= bytes.len() || bytes[i + 1] != b'\n' {
                return Err(CsvError::BareCr {
                    record: record_index,
                    byte: i,
                });
            }

            if state == ParseState::AfterQuote || state == ParseState::FieldStart {
                row.push(String::from_utf8(field).unwrap());
                field = Vec::new();
            } else if state == ParseState::Unquoted {
                row.push(String::from_utf8(field).unwrap());
                field = Vec::new();
            }
            records.push(row);
            row = Vec::new();
            state = ParseState::FieldStart;
            record_started = false;
            record_index += 1;
            i += 2;
            continue;
        }

        if state != ParseState::Quoted && b == b'\n' {
            if state == ParseState::FieldStart || state == ParseState::AfterQuote {
                row.push(String::from_utf8(field).unwrap());
                field = Vec::new();
            } else if state == ParseState::Unquoted {
                row.push(String::from_utf8(field).unwrap());
                field = Vec::new();
            }
            records.push(row);
            row = Vec::new();
            state = ParseState::FieldStart;
            record_started = false;
            record_index += 1;
            i += 1;
            continue;
        }

        match state {
            ParseState::FieldStart => {
                if b == delimiter {
                    row.push(String::new());
                    record_started = true;
                    i += 1;
                } else if b == quote {
                    state = ParseState::Quoted;
                    quote_open = i;
                    record_started = true;
                    i += 1;
                } else {
                    field.push(b);
                    state = ParseState::Unquoted;
                    record_started = true;
                    i += 1;
                }
            }
            ParseState::Unquoted => {
                if b == delimiter {
                    row.push(String::from_utf8(field).unwrap());
                    field = Vec::new();
                    state = ParseState::FieldStart;
                    record_started = true;
                    i += 1;
                } else if b == quote {
                    return Err(CsvError::UnexpectedQuote {
                        record: record_index,
                        byte: i,
                    });
                } else {
                    field.push(b);
                    i += 1;
                }
            }
            ParseState::Quoted => {
                if b == quote {
                    if i + 1 < bytes.len() && bytes[i + 1] == quote {
                        field.push(quote);
                        i += 2;
                    } else {
                        state = ParseState::AfterQuote;
                        i += 1;
                    }
                } else {
                    field.push(b);
                    i += 1;
                }
            }
            ParseState::AfterQuote => {
                if b == delimiter {
                    row.push(String::from_utf8(field).unwrap());
                    field = Vec::new();
                    state = ParseState::FieldStart;
                    record_started = true;
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
            byte: quote_open,
        });
    }

    if record_started {
        row.push(String::from_utf8(field).unwrap());
        records.push(row);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with_dialect(input, b',', b'"')
}

fn build_table(
    parsed: Vec<Vec<String>>,
    config: Config,
) -> Result<Table, CsvError> {
    if !config.headers || parsed.is_empty() {
        return Ok(Table {
            header: None,
            rows: parsed,
            duplicates: config.duplicates,
        });
    }

    let mut records = parsed.into_iter();
    let header = records.next().unwrap();

    if config.duplicates == DuplicatePolicy::Reject {
        let mut seen: Vec<(&str, usize)> = Vec::new();
        for (second, name) in header.iter().enumerate() {
            if let Some((_, first)) = seen.iter().find(|(existing, _)| *existing == name) {
                return Err(CsvError::DuplicateHeader {
                    name: name.clone(),
                    first: *first,
                    second,
                });
            }
            seen.push((name.as_str(), second));
        }
    }

    let mut rows = Vec::new();
    for (record_index, row) in records.enumerate() {
        if row.len() != header.len() {
            return Err(CsvError::HeaderWidth {
                record: record_index + 1,
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
    finished: Option<Result<Table, CsvError>>,
}

impl StreamReader {
    pub fn new(config: Config) -> Result<Self, CsvError> {
        validate_config(config)?;
        Ok(Self {
            config,
            input: String::new(),
            finished: None,
        })
    }

    pub fn feed(&mut self, chunk: &str) -> Result<(), CsvError> {
        if self.finished.is_some() {
            return Err(CsvError::ReaderFinished);
        }
        self.input.push_str(chunk);
        Ok(())
    }

    pub fn finish(&mut self) -> Result<Table, CsvError> {
        if let Some(result) = &self.finished {
            return result.clone();
        }

        let result = parse_with_dialect(&self.input, self.config.delimiter, self.config.quote)
            .and_then(|parsed| build_table(parsed, self.config));
        self.finished = Some(result.clone());
        result
    }

    pub fn reset(&mut self) {
        self.input.clear();
        self.finished = None;
    }
}

pub fn read_csv(input: &str, config: Config) -> Result<Table, CsvError> {
    let mut reader = StreamReader::new(config)?;
    reader.feed(input)?;
    reader.finish()
}

fn main() {}