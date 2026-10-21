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
    let mut row = Vec::new();
    let mut field = String::new();
    let mut i = start;
    let mut record_index = 0usize;

    let mut in_quotes = false;
    let mut after_quote = false;
    let mut field_started = false;
    let mut quote_opening = 0usize;
    let mut record_started = false;

    while i < bytes.len() {
        let b = bytes[i];

        if b == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: i,
            });
        }

        if !in_quotes && b == b'\r' && bytes.get(i + 1) != Some(&b'\n') {
            return Err(CsvError::BareCr {
                record: record_index,
                byte: i,
            });
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
            let ch = input[i..].chars().next().expect("valid UTF-8 boundary");
            field.push(ch);
            i += ch.len_utf8();
            continue;
        }

        if after_quote {
            if b == delimiter {
                row.push(std::mem::take(&mut field));
                after_quote = false;
                field_started = false;
                record_started = true;
                i += 1;
                continue;
            }
            if b == b'\n' || b == b'\r' {
                row.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut row));
                record_index += 1;
                after_quote = false;
                field_started = false;
                record_started = false;
                i += if b == b'\r' { 2 } else { 1 };
                continue;
            }
            return Err(CsvError::TrailingByte {
                record: record_index,
                byte: i,
            });
        }

        if b == delimiter {
            row.push(std::mem::take(&mut field));
            field_started = false;
            record_started = true;
            i += 1;
            continue;
        }
        if b == b'\n' || b == b'\r' {
            row.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut row));
            record_index += 1;
            field_started = false;
            record_started = false;
            i += if b == b'\r' { 2 } else { 1 };
            continue;
        }
        if b == quote {
            if !field_started {
                in_quotes = true;
                quote_opening = i;
                field_started = true;
                record_started = true;
                i += 1;
                continue;
            }
            return Err(CsvError::UnexpectedQuote {
                record: record_index,
                byte: i,
            });
        }

        let ch = input[i..].chars().next().expect("valid UTF-8 boundary");
        field.push(ch);
        field_started = true;
        record_started = true;
        i += ch.len_utf8();
    }

    if in_quotes {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: quote_opening,
        });
    }

    if record_started || field_started || after_quote || !row.is_empty() || !field.is_empty() {
        row.push(field);
        records.push(row);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with_dialect(input, b',', b'"')
}

fn read_csv_validated(input: &str, config: Config) -> Result<Table, CsvError> {
    let mut records = parse_with_dialect(input, config.delimiter, config.quote)?;
    let mut header = None;

    if config.headers && !records.is_empty() {
        let first = records.remove(0);
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
        header = Some(first);
        let expected = header.as_ref().unwrap().len();
        for (index, row) in records.iter().enumerate() {
            let record = index + 1;
            if row.len() != expected {
                return Err(CsvError::HeaderWidth {
                    record,
                    expected,
                    actual: row.len(),
                });
            }
        }
    }

    Ok(Table {
        header,
        rows: records,
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
        let result = read_csv_validated(&self.input, self.config);
        self.finished = Some(result.clone());
        result
    }

    pub fn reset(&mut self) {
        self.input.clear();
        self.finished = None;
    }
}

pub fn read_csv(input: &str, config: Config) -> Result<Table, CsvError> {
    validate_config(config)?;
    let mut reader = StreamReader {
        config,
        input: String::new(),
        finished: None,
    };
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
    let digits = if b[0] == b'-' { &b[1..] } else { b };
    if digits.is_empty() {
        return false;
    }
    if digits == b"0" {
        return b[0] != b'-';
    }
    digits[0].is_ascii_digit()
        && digits[0] != b'0'
        && digits.iter().all(u8::is_ascii_digit)
}

fn decimal_syntax(s: &str) -> bool {
    let b = s.as_bytes();
    let body = if b.first() == Some(&b'-') {
        &b[1..]
    } else {
        b
    };
    let Some(dot) = body.iter().position(|&x| x == b'.') else {
        return false;
    };
    if body[dot + 1..].len() != 2
        || !body[dot + 1..].iter().all(u8::is_ascii_digit)
        || body[..dot].is_empty()
    {
        return false;
    }
    let whole = &body[..dot];
    let valid_whole = whole == b"0"
        || (whole[0].is_ascii_digit()
            && whole[0] != b'0'
            && whole.iter().all(u8::is_ascii_digit));
    if !valid_whole {
        return false;
    }
    !(b.first() == Some(&b'-') && whole == b"0" && body[dot + 1..] == *b"00")
}

fn evaluate(value: &str, spec: &ColumnSpec) -> Result<Value, CellErrorKind> {
    if value.is_empty() {
        if spec.nullable {
            return Ok(Value::Null);
        }
        if spec.kind != ColumnType::Text {
            return Err(CellErrorKind::Empty);
        }
    }

    match spec.kind {
        ColumnType::Text => Ok(Value::Text(value.to_string())),
        ColumnType::Integer => {
            if !integer_syntax(value) {
                return Err(CellErrorKind::InvalidInteger);
            }
            value
                .parse::<i64>()
                .map(Value::Integer)
                .map_err(|_| CellErrorKind::IntegerOverflow)
        }
        ColumnType::Decimal => {
            if !decimal_syntax(value) {
                return Err(CellErrorKind::InvalidDecimal);
            }

            let negative = value.starts_with('-');
            let unsigned = if negative { &value[1..] } else { value };
            let (whole, fraction) = unsigned.split_once('.').expect("validated decimal");

            let mut magnitude: u128 = 0;
            for byte in whole.bytes().chain(fraction.bytes()) {
                magnitude = magnitude * 10 + u128::from(byte - b'0');
            }

            let limit = if negative {
                (i64::MAX as u128) + 1
            } else {
                i64::MAX as u128
            };
            if magnitude > limit {
                return Err(CellErrorKind::DecimalOverflow);
            }

            if negative {
                if magnitude == (i64::MAX as u128) + 1 {
                    Ok(Value::Decimal(i64::MIN))
                } else {
                    Ok(Value::Decimal(-(magnitude as i64)))
                }
            } else {
                Ok(Value::Decimal(magnitude as i64))
            }
        }
        ColumnType::Boolean => match value {
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
        rows: Vec::with_capacity(table.rows.len()),
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

            let column_index = match &spec.column {
                ColumnRef::Index(index) => Some(*index),
                ColumnRef::Name(name) => table.column_index(name),
            };

            let evaluated = match column_index {
                None => Err((None, CellErrorKind::UnknownColumn)),
                Some(index) => match row.get(index) {
                    None => Err((Some(index), CellErrorKind::MissingCell)),
                    Some(value) => evaluate(value, spec).map_err(|kind| (Some(index), kind)),
                },
            };

            match evaluated {
                Ok(value) => output.push(Some(value)),
                Err((column, kind)) => {
                    output.push(None);
                    result.errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column,
                        kind,
                    });
                    match policy {
                        ErrorPolicy::Collect => {}
                        ErrorPolicy::FirstPerRow | ErrorPolicy::Stop => stop_row = true,
                    }
                }
            }

            if policy == ErrorPolicy::Stop && !result.errors.is_empty() {
                for _ in spec_index + 1..columns.len() {
                    output.push(None);
                }
                result.rows.push(output);
                return result;
            }
        }

        result.rows.push(output);
    }

    result
}