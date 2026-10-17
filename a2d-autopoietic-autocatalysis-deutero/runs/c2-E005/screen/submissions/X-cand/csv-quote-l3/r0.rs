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

fn parse_csv_with(input: &str, delimiter: u8, quote: u8) -> Result<Vec<Vec<String>>, CsvError> {
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
    let mut record = Vec::new();
    let mut field = String::new();
    let mut i = start;
    let mut record_index = 0usize;

    let mut in_quotes = false;
    let mut after_quote = false;
    let mut field_started = false;
    let mut opening_quote = 0usize;

    while i < bytes.len() {
        let byte = bytes[i];

        if byte == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: i,
            });
        }

        if in_quotes {
            if byte == quote {
                if i + 1 < bytes.len() && bytes[i + 1] == quote {
                    field.push(quote as char);
                    i += 2;
                    continue;
                }
                in_quotes = false;
                after_quote = true;
                i += 1;
                continue;
            }
            let ch = input[i..].chars().next().expect("valid UTF-8");
            field.push(ch);
            i += ch.len_utf8();
            continue;
        }

        if byte == b'\r' && (i + 1 >= bytes.len() || bytes[i + 1] != b'\n') {
            return Err(CsvError::BareCr {
                record: record_index,
                byte: i,
            });
        }

        if after_quote {
            if byte == delimiter {
                record.push(std::mem::take(&mut field));
                field_started = false;
                after_quote = false;
                i += 1;
            } else if byte == b'\n' || byte == b'\r' {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
                record_index += 1;
                field_started = false;
                after_quote = false;
                i += if byte == b'\r' { 2 } else { 1 };
            } else {
                return Err(CsvError::TrailingByte {
                    record: record_index,
                    byte: i,
                });
            }
            continue;
        }

        if byte == quote {
            if field_started {
                return Err(CsvError::UnexpectedQuote {
                    record: record_index,
                    byte: i,
                });
            }
            in_quotes = true;
            opening_quote = i;
            field_started = true;
            i += 1;
            continue;
        }

        if byte == delimiter {
            record.push(std::mem::take(&mut field));
            field_started = false;
            i += 1;
            continue;
        }

        if byte == b'\n' || byte == b'\r' {
            record.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut record));
            record_index += 1;
            field_started = false;
            i += if byte == b'\r' { 2 } else { 1 };
            continue;
        }

        let ch = input[i..].chars().next().expect("valid UTF-8");
        field.push(ch);
        field_started = true;
        i += ch.len_utf8();
    }

    if in_quotes {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: opening_quote,
        });
    }

    if !record.is_empty() || field_started || !field.is_empty() {
        record.push(field);
        records.push(record);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_csv_with(input, b',', b'"')
}

fn build_table(records: Vec<Vec<String>>, config: Config) -> Result<Table, CsvError> {
    let mut header = None;
    let mut rows = records;

    if config.headers && !rows.is_empty() {
        let parsed_header = rows.remove(0);
        if config.duplicates == DuplicatePolicy::Reject {
            let mut seen = std::collections::HashMap::<&str, usize>::new();
            for (second, name) in parsed_header.iter().enumerate() {
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

        for (data_index, row) in rows.iter().enumerate() {
            if row.len() != parsed_header.len() {
                return Err(CsvError::HeaderWidth {
                    record: data_index + 1,
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
        let result = parse_csv_with(&self.input, self.config.delimiter, self.config.quote)
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
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let digits = if bytes[0] == b'-' { &bytes[1..] } else { bytes };
    if digits.is_empty() {
        return false;
    }
    if digits[0] == b'0' {
        return digits.len() == 1 && bytes[0] != b'-';
    }
    digits[0].is_ascii_digit()
        && digits[0] != b'0'
        && digits.iter().all(u8::is_ascii_digit)
}

fn parse_integer(s: &str) -> Result<i64, CellErrorKind> {
    if !integer_syntax(s) {
        return Err(CellErrorKind::InvalidInteger);
    }
    s.parse::<i64>()
        .map_err(|_| CellErrorKind::IntegerOverflow)
}

fn decimal_syntax(s: &str) -> bool {
    let bytes = s.as_bytes();
    let body = if bytes.first() == Some(&b'-') {
        &bytes[1..]
    } else {
        bytes
    };
    let Some(dot) = body.iter().position(|&b| b == b'.') else {
        return false;
    };
    let integer = &body[..dot];
    let fractional = &body[dot + 1..];

    if fractional.len() != 2 || !fractional.iter().all(u8::is_ascii_digit) || integer.is_empty() {
        return false;
    }
    let valid_integer = if integer[0] == b'0' {
        integer.len() == 1
    } else {
        integer[0].is_ascii_digit()
            && integer[0] != b'0'
            && integer.iter().all(u8::is_ascii_digit)
    };
    if !valid_integer {
        return false;
    }
    !(bytes.first() == Some(&b'-')
        && integer == b"0"
        && fractional.iter().all(|&b| b == b'0'))
}

fn parse_decimal(s: &str) -> Result<i64, CellErrorKind> {
    if !decimal_syntax(s) {
        return Err(CellErrorKind::InvalidDecimal);
    }

    let negative = s.starts_with('-');
    let body = if negative { &s[1..] } else { s };
    let (integer, fractional) = body.split_once('.').expect("validated decimal");
    let digits = format!("{integer}{fractional}");
    let magnitude = digits
        .parse::<i128>()
        .map_err(|_| CellErrorKind::DecimalOverflow)?;
    let signed = if negative { -magnitude } else { magnitude };
    i64::try_from(signed).map_err(|_| CellErrorKind::DecimalOverflow)
}

pub fn extract_columns(
    table: &Table,
    columns: &[ColumnSpec],
    policy: ErrorPolicy,
) -> TypedResult {
    let mut result = TypedResult {
        rows: Vec::with_capacity(table.rows.len()),
        errors: Vec::new(),
    };

    'rows: for (row_index, row) in table.rows.iter().enumerate() {
        let mut output = vec![None; columns.len()];
        for (spec_index, spec) in columns.iter().enumerate() {
            let column_index = match &spec.column {
                ColumnRef::Index(index) => Some(*index),
                ColumnRef::Name(name) => table.column_index(name),
            };

            let Some(column_index) = column_index else {
                result.errors.push(CellError {
                    row: row_index,
                    spec: spec_index,
                    column: None,
                    kind: CellErrorKind::UnknownColumn,
                });
                if policy == ErrorPolicy::Stop {
                    result.rows.push(output);
                    break 'rows;
                }
                if policy == ErrorPolicy::FirstPerRow {
                    break;
                }
                continue;
            };

            let Some(cell) = row.get(column_index) else {
                result.errors.push(CellError {
                    row: row_index,
                    spec: spec_index,
                    column: Some(column_index),
                    kind: CellErrorKind::MissingCell,
                });
                if policy == ErrorPolicy::Stop {
                    result.rows.push(output);
                    break 'rows;
                }
                if policy == ErrorPolicy::FirstPerRow {
                    break;
                }
                continue;
            };

            if cell.is_empty() {
                if spec.nullable {
                    output[spec_index] = Some(Value::Null);
                    continue;
                }
                if spec.kind != ColumnType::Text {
                    result.errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column: Some(column_index),
                        kind: CellErrorKind::Empty,
                    });
                    if policy == ErrorPolicy::Stop {
                        result.rows.push(output);
                        break 'rows;
                    }
                    if policy == ErrorPolicy::FirstPerRow {
                        break;
                    }
                    continue;
                }
            }

            let value = match spec.kind {
                ColumnType::Text => Ok(Value::Text(cell.clone())),
                ColumnType::Integer => parse_integer(cell).map(Value::Integer),
                ColumnType::Decimal => parse_decimal(cell).map(Value::Decimal),
                ColumnType::Boolean => match cell.as_str() {
                    "yes" | "1" => Ok(Value::Boolean(true)),
                    "no" | "0" => Ok(Value::Boolean(false)),
                    _ => Err(CellErrorKind::InvalidBoolean),
                },
            };

            match value {
                Ok(value) => output[spec_index] = Some(value),
                Err(kind) => {
                    result.errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column: Some(column_index),
                        kind,
                    });
                    if policy == ErrorPolicy::Stop {
                        result.rows.push(output);
                        break 'rows;
                    }
                    if policy == ErrorPolicy::FirstPerRow {
                        break;
                    }
                }
            }
        }
        result.rows.push(output);
    }

    result
}