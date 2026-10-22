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
        let result = parse_table(&self.input, self.config);
        self.result = Some(result.clone());
        result
    }

    pub fn reset(&mut self) {
        self.input.clear();
        self.result = None;
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

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_records(input, b',', b'"')
}

fn parse_records(input: &str, delimiter: u8, quote: u8) -> Result<Vec<Vec<String>>, CsvError> {
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
    let mut row = Vec::new();
    let mut field = String::new();
    let mut i = start;
    let mut record_index = 0usize;
    let mut in_quotes = false;
    let mut after_quote = false;
    let mut quote_start = 0usize;
    let mut field_started = false;

    while i < bytes.len() {
        let byte = bytes[i];

        if byte == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: i,
            });
        }

        if !in_quotes && byte == b'\r' && bytes.get(i + 1) != Some(&b'\n') {
            return Err(CsvError::BareCr {
                record: record_index,
                byte: i,
            });
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
                let ch = input[i..].chars().next().expect("valid UTF-8 boundary");
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
                i += 1;
            } else if byte == b'\n' || byte == b'\r' {
                row.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut row));
                record_index += 1;
                after_quote = false;
                field_started = false;
                if byte == b'\r' {
                    i += 2;
                } else {
                    i += 1;
                }
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
            i += 1;
        } else if byte == b'\n' || byte == b'\r' {
            row.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut row));
            record_index += 1;
            field_started = false;
            if byte == b'\r' {
                i += 2;
            } else {
                i += 1;
            }
        } else if byte == quote {
            if field_started {
                return Err(CsvError::UnexpectedQuote {
                    record: record_index,
                    byte: i,
                });
            }
            in_quotes = true;
            quote_start = i;
            field_started = true;
            i += 1;
        } else {
            let ch = input[i..].chars().next().expect("valid UTF-8 boundary");
            field.push(ch);
            field_started = true;
            i += ch.len_utf8();
        }
    }

    if in_quotes {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: quote_start,
        });
    }

    if !row.is_empty() || !field.is_empty() || field_started || after_quote {
        row.push(field);
        records.push(row);
    }

    Ok(records)
}

fn parse_table(input: &str, config: Config) -> Result<Table, CsvError> {
    validate_config(config)?;
    let mut records = parse_records(input, config.delimiter, config.quote)?;

    if !config.headers {
        return Ok(Table {
            header: None,
            rows: records,
            duplicates: config.duplicates,
        });
    }

    if records.is_empty() {
        return Ok(Table {
            header: None,
            rows: Vec::new(),
            duplicates: config.duplicates,
        });
    }

    let header = records.remove(0);

    if config.duplicates == DuplicatePolicy::Reject {
        let mut seen: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
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

    for (record, row) in records.iter().enumerate() {
        if row.len() != header.len() {
            return Err(CsvError::HeaderWidth {
                record: record + 1,
                expected: header.len(),
                actual: row.len(),
            });
        }
    }

    Ok(Table {
        header: Some(header),
        rows: records,
        duplicates: config.duplicates,
    })
}

pub fn read_csv(input: &str, config: Config) -> Result<Table, CsvError> {
    let mut reader = StreamReader::new(config)?;
    reader.feed(input)?;
    reader.finish()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColumnRef {
    Index(usize),
    Name(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnType {
    Text,
    Integer,
    Decimal,
    Boolean,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnSpec {
    pub column: ColumnRef,
    pub kind: ColumnType,
    pub nullable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Null,
    Text(String),
    Integer(i64),
    Decimal(i64),
    Boolean(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellErrorKind {
    UnknownColumn,
    MissingCell,
    Empty,
    InvalidInteger,
    IntegerOverflow,
    InvalidDecimal,
    DecimalOverflow,
    InvalidBoolean,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellError {
    pub row: usize,
    pub spec: usize,
    pub column: Option<usize>,
    pub kind: CellErrorKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorPolicy {
    Collect,
    FirstPerRow,
    Stop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedResult {
    pub rows: Vec<Vec<Option<Value>>>,
    pub errors: Vec<CellError>,
}

fn integer_syntax(s: &str) -> bool {
    let b = s.as_bytes();
    if b.is_empty() {
        return false;
    }
    let start = usize::from(b[0] == b'-');
    if start == b.len() {
        return false;
    }
    if b[start] == b'0' {
        return start + 1 == b.len() && start == 0;
    }
    (b'1'..=b'9').contains(&b[start])
        && b[start + 1..].iter().all(u8::is_ascii_digit)
}

fn decimal_syntax(s: &str) -> bool {
    let b = s.as_bytes();
    if b.is_empty() {
        return false;
    }
    let start = usize::from(b[0] == b'-');
    if b.len() < start + 4 {
        return false;
    }
    let dot = b.len() - 3;
    if b[dot] != b'.' || !b[dot + 1..].iter().all(u8::is_ascii_digit) {
        return false;
    }
    if dot <= start {
        return false;
    }
    if b[start] == b'0' {
        dot == start + 1
    } else {
        (b'1'..=b'9').contains(&b[start])
            && b[start + 1..dot].iter().all(u8::is_ascii_digit)
    }
}

fn parse_integer(s: &str) -> Result<i64, CellErrorKind> {
    if !integer_syntax(s) {
        return Err(CellErrorKind::InvalidInteger);
    }
    s.parse::<i64>()
        .map_err(|_| CellErrorKind::IntegerOverflow)
}

fn parse_decimal(s: &str) -> Result<i64, CellErrorKind> {
    if !decimal_syntax(s) || s == "-0.00" {
        return Err(CellErrorKind::InvalidDecimal);
    }
    let negative = s.starts_with('-');
    let unsigned = if negative { &s[1..] } else { s };
    let dot = unsigned.len() - 3;
    let whole = unsigned[..dot]
        .parse::<i128>()
        .map_err(|_| CellErrorKind::DecimalOverflow)?;
    let frac = unsigned[dot + 1..]
        .parse::<i128>()
        .expect("fractional digits validated");
    let magnitude = whole
        .checked_mul(100)
        .and_then(|v| v.checked_add(frac))
        .ok_or(CellErrorKind::DecimalOverflow)?;
    let signed = if negative { -magnitude } else { magnitude };
    i64::try_from(signed).map_err(|_| CellErrorKind::DecimalOverflow)
}

pub fn extract_columns(
    table: &Table,
    columns: &[ColumnSpec],
    policy: ErrorPolicy,
) -> TypedResult {
    let mut result = TypedResult {
        rows: Vec::new(),
        errors: Vec::new(),
    };

    for (row_index, row) in table.rows.iter().enumerate() {
        let mut output = Vec::with_capacity(columns.len());
        let mut stop_row = false;

        for (spec_index, spec) in columns.iter().enumerate() {
            if stop_row {
                output.push(None);
                continue;
            }

            let column = match &spec.column {
                ColumnRef::Index(index) => Some(*index),
                ColumnRef::Name(name) => table.column_index(name),
            };

            let evaluated = match column {
                None => Err((None, CellErrorKind::UnknownColumn)),
                Some(index) => match row.get(index) {
                    None => Err((Some(index), CellErrorKind::MissingCell)),
                    Some(value) if value.is_empty() && spec.nullable => Ok(Value::Null),
                    Some(value) if value.is_empty() && spec.kind != ColumnType::Text => {
                        Err((Some(index), CellErrorKind::Empty))
                    }
                    Some(value) => {
                        let parsed = match spec.kind {
                            ColumnType::Text => Ok(Value::Text(value.clone())),
                            ColumnType::Integer => parse_integer(value).map(Value::Integer),
                            ColumnType::Decimal => parse_decimal(value).map(Value::Decimal),
                            ColumnType::Boolean => match value.as_str() {
                                "yes" | "1" => Ok(Value::Boolean(true)),
                                "no" | "0" => Ok(Value::Boolean(false)),
                                _ => Err(CellErrorKind::InvalidBoolean),
                            },
                        };
                        parsed.map_err(|kind| (Some(index), kind))
                    }
                },
            };

            match evaluated {
                Ok(value) => output.push(Some(value)),
                Err((source_column, kind)) => {
                    output.push(None);
                    result.errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column: source_column,
                        kind,
                    });
                    if policy != ErrorPolicy::Collect {
                        stop_row = true;
                    }
                    if policy == ErrorPolicy::Stop {
                        for _ in spec_index + 1..columns.len() {
                            output.push(None);
                        }
                        result.rows.push(output);
                        return result;
                    }
                }
            }
        }

        result.rows.push(output);
    }

    result
}