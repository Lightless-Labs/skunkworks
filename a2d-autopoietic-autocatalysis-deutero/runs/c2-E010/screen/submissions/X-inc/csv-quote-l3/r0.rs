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
            DuplicatePolicy::KeepLast => header.iter().rposition(|item| item == name),
            DuplicatePolicy::Reject | DuplicatePolicy::KeepFirst => {
                header.iter().position(|item| item == name)
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

fn parse_with(input: &str, delimiter: u8, quote: u8) -> Result<Vec<Vec<String>>, CsvError> {
    let bytes = input.as_bytes();
    let mut pos = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) { 3 } else { 0 };
    if pos == bytes.len() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    let mut fields = Vec::new();
    let mut field = Vec::new();
    let mut record_index = 0usize;
    let mut in_quotes = false;
    let mut after_quote = false;
    let mut field_started = false;
    let mut opening_quote = 0usize;

    while pos < bytes.len() {
        let byte = bytes[pos];
        if byte == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: pos,
            });
        }

        if in_quotes {
            if byte == quote {
                if pos + 1 < bytes.len() && bytes[pos + 1] == quote {
                    field.push(quote);
                    pos += 2;
                } else {
                    in_quotes = false;
                    after_quote = true;
                    pos += 1;
                }
            } else {
                field.push(byte);
                pos += 1;
            }
            continue;
        }

        if byte == b'\r' && (pos + 1 >= bytes.len() || bytes[pos + 1] != b'\n') {
            return Err(CsvError::BareCr {
                record: record_index,
                byte: pos,
            });
        }

        if after_quote {
            if byte == delimiter {
                fields.push(String::from_utf8(field).expect("CSV field remains valid UTF-8"));
                field = Vec::new();
                field_started = false;
                after_quote = false;
                pos += 1;
                continue;
            }
            if byte == b'\n' || byte == b'\r' {
                fields.push(String::from_utf8(field).expect("CSV field remains valid UTF-8"));
                records.push(fields);
                fields = Vec::new();
                field = Vec::new();
                field_started = false;
                after_quote = false;
                record_index += 1;
                pos += if byte == b'\r' { 2 } else { 1 };
                continue;
            }
            return Err(CsvError::TrailingByte {
                record: record_index,
                byte: pos,
            });
        }

        if byte == delimiter {
            fields.push(String::from_utf8(field).expect("CSV field remains valid UTF-8"));
            field = Vec::new();
            field_started = false;
            pos += 1;
        } else if byte == b'\n' || byte == b'\r' {
            fields.push(String::from_utf8(field).expect("CSV field remains valid UTF-8"));
            records.push(fields);
            fields = Vec::new();
            field = Vec::new();
            field_started = false;
            record_index += 1;
            pos += if byte == b'\r' { 2 } else { 1 };
        } else if byte == quote {
            if field_started {
                return Err(CsvError::UnexpectedQuote {
                    record: record_index,
                    byte: pos,
                });
            }
            in_quotes = true;
            opening_quote = pos;
            field_started = true;
            pos += 1;
        } else {
            field.push(byte);
            field_started = true;
            pos += 1;
        }
    }

    if in_quotes {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: opening_quote,
        });
    }

    if !fields.is_empty() || !field.is_empty() || field_started || after_quote {
        fields.push(String::from_utf8(field).expect("CSV field remains valid UTF-8"));
        records.push(fields);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with(input, b',', b'"')
}

fn build_table(records: Vec<Vec<String>>, config: Config) -> Result<Table, CsvError> {
    if !config.headers {
        return Ok(Table {
            header: None,
            rows: records,
            duplicates: config.duplicates,
        });
    }

    let mut iter = records.into_iter();
    let Some(header) = iter.next() else {
        return Ok(Table {
            header: None,
            rows: Vec::new(),
            duplicates: config.duplicates,
        });
    };

    if config.duplicates == DuplicatePolicy::Reject {
        for second in 0..header.len() {
            if let Some(first) = header[..second].iter().position(|name| name == &header[second]) {
                return Err(CsvError::DuplicateHeader {
                    name: header[second].clone(),
                    first,
                    second,
                });
            }
        }
    }

    let mut rows = Vec::new();
    for (record, row) in iter.enumerate() {
        let record = record + 1;
        if row.len() != header.len() {
            return Err(CsvError::HeaderWidth {
                record,
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
    cached: Option<Result<Table, CsvError>>,
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
            .and_then(|records| build_table(records, self.config));
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
    build_table(records, config)
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

fn parse_integer(text: &str) -> Result<i64, CellErrorKind> {
    let bytes = text.as_bytes();
    let valid = if bytes == b"0" {
        true
    } else {
        let digits = if bytes.first() == Some(&b'-') {
            &bytes[1..]
        } else {
            bytes
        };
        !digits.is_empty()
            && (b'1'..=b'9').contains(&digits[0])
            && digits[1..].iter().all(u8::is_ascii_digit)
            && !(bytes.first() == Some(&b'-') && digits == b"0")
    };
    if !valid {
        return Err(CellErrorKind::InvalidInteger);
    }
    text.parse::<i64>().map_err(|_| CellErrorKind::IntegerOverflow)
}

fn parse_decimal(text: &str) -> Result<i64, CellErrorKind> {
    let bytes = text.as_bytes();
    let body = if bytes.first() == Some(&b'-') {
        &bytes[1..]
    } else {
        bytes
    };
    let valid = if let Some(dot) = body.iter().position(|&b| b == b'.') {
        let integral = &body[..dot];
        let fractional = &body[dot + 1..];
        !integral.is_empty()
            && fractional.len() == 2
            && fractional.iter().all(u8::is_ascii_digit)
            && (integral == b"0"
                || ((b'1'..=b'9').contains(&integral[0])
                    && integral[1..].iter().all(u8::is_ascii_digit)))
            && !(bytes.first() == Some(&b'-') && integral == b"0" && fractional == b"00")
    } else {
        false
    };
    if !valid {
        return Err(CellErrorKind::InvalidDecimal);
    }
    let negative = bytes.first() == Some(&b'-');
    let unsigned = if negative { &text[1..] } else { text };
    let mut digits = unsigned.replace('.', "");
    if negative {
        digits.insert(0, '-');
    }
    digits
        .parse::<i64>()
        .map_err(|_| CellErrorKind::DecimalOverflow)
}

fn evaluate_cell(text: &str, kind: ColumnType, nullable: bool) -> Result<Value, CellErrorKind> {
    if text.is_empty() {
        if nullable {
            return Ok(Value::Null);
        }
        if kind != ColumnType::Text {
            return Err(CellErrorKind::Empty);
        }
    }

    match kind {
        ColumnType::Text => Ok(Value::Text(text.to_owned())),
        ColumnType::Integer => parse_integer(text).map(Value::Integer),
        ColumnType::Decimal => parse_decimal(text).map(Value::Decimal),
        ColumnType::Boolean => match text {
            "yes" | "1" => Ok(Value::Boolean(true)),
            "no" | "0" => Ok(Value::Boolean(false)),
            _ => Err(CellErrorKind::InvalidBoolean),
        },
    }
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
        let mut output = vec![None; columns.len()];
        let mut row_failed = false;

        for (spec_index, spec) in columns.iter().enumerate() {
            if policy == ErrorPolicy::Stop && !result.errors.is_empty() {
                break;
            }
            if row_failed {
                break;
            }

            let resolved = match &spec.column {
                ColumnRef::Index(index) => Some(*index),
                ColumnRef::Name(name) => table.column_index(name),
            };

            let Some(column_index) = resolved else {
                result.errors.push(CellError {
                    row: row_index,
                    spec: spec_index,
                    column: None,
                    kind: CellErrorKind::UnknownColumn,
                });
                row_failed = true;
                continue;
            };

            let Some(cell) = row.get(column_index) else {
                result.errors.push(CellError {
                    row: row_index,
                    spec: spec_index,
                    column: Some(column_index),
                    kind: CellErrorKind::MissingCell,
                });
                row_failed = true;
                continue;
            };

            match evaluate_cell(cell, spec.kind, spec.nullable) {
                Ok(value) => output[spec_index] = Some(value),
                Err(kind) => {
                    result.errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column: Some(column_index),
                        kind,
                    });
                    row_failed = true;
                }
            }

            if row_failed && policy == ErrorPolicy::Stop {
                break;
            }
        }

        result.rows.push(output);
        if policy == ErrorPolicy::Stop && !result.errors.is_empty() {
            break;
        }
    }

    result
}