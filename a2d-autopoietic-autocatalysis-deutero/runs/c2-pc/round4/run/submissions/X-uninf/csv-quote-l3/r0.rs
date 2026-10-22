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
            DuplicatePolicy::KeepLast => header.iter().rposition(|v| v == name),
            DuplicatePolicy::Reject | DuplicatePolicy::KeepFirst => {
                header.iter().position(|v| v == name)
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

fn parse_with_dialect(
    input: &str,
    delimiter: u8,
    quote: u8,
) -> Result<Vec<Vec<String>>, CsvError> {
    let bytes = input.as_bytes();
    let start = if input.starts_with('\u{feff}') { 3 } else { 0 };

    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut i = start;
    let mut record_index = 0usize;

    let mut in_quotes = false;
    let mut after_quote = false;
    let mut field_started = false;
    let mut quote_open = 0usize;
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
            if b == delimiter {
                record.push(std::mem::take(&mut field));
                after_quote = false;
                field_started = false;
                record_started = true;
                i += 1;
                continue;
            }
            if b == b'\n' || b == b'\r' {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
                after_quote = false;
                field_started = false;
                record_started = false;
                record_index += 1;
                if b == b'\r' {
                    i += 2;
                } else {
                    i += 1;
                }
                continue;
            }
            return Err(CsvError::TrailingByte {
                record: record_index,
                byte: i,
            });
        }

        if b == delimiter {
            record.push(std::mem::take(&mut field));
            field_started = false;
            record_started = true;
            i += 1;
            continue;
        }

        if b == b'\n' || b == b'\r' {
            record.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut record));
            field_started = false;
            record_started = false;
            record_index += 1;
            if b == b'\r' {
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }

        if b == quote {
            if !field_started {
                in_quotes = true;
                quote_open = i;
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
            byte: quote_open,
        });
    }

    if record_started || field_started || after_quote || !record.is_empty() {
        record.push(field);
        records.push(record);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with_dialect(input, b',', b'"')
}

fn build_table(records: Vec<Vec<String>>, config: Config) -> Result<Table, CsvError> {
    let (header, rows) = if config.headers {
        if records.is_empty() {
            (None, Vec::new())
        } else {
            let mut iter = records.into_iter();
            (iter.next(), iter.collect::<Vec<_>>())
        }
    } else {
        (None, records)
    };

    if let (Some(header_values), DuplicatePolicy::Reject) = (&header, config.duplicates) {
        let mut seen = std::collections::HashMap::<&str, usize>::new();
        for (second, name) in header_values.iter().enumerate() {
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

    if let Some(header_values) = &header {
        for (record_index, row) in rows.iter().enumerate() {
            if row.len() != header_values.len() {
                return Err(CsvError::HeaderWidth {
                    record: record_index + 1,
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

        let result = parse_with_dialect(&self.input, self.config.delimiter, self.config.quote)
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
        return start == 0 && b.len() == 1;
    }
    if !(b'1'..=b'9').contains(&b[start]) {
        return false;
    }
    b[start + 1..].iter().all(u8::is_ascii_digit)
}

fn parse_integer(s: &str) -> Result<i64, CellErrorKind> {
    if !integer_syntax(s) {
        return Err(CellErrorKind::InvalidInteger);
    }
    s.parse::<i64>()
        .map_err(|_| CellErrorKind::IntegerOverflow)
}

fn decimal_syntax(s: &str) -> bool {
    let b = s.as_bytes();
    if b.is_empty() {
        return false;
    }
    let start = usize::from(b[0] == b'-');
    if start >= b.len() {
        return false;
    }

    let dot = match b[start..].iter().position(|&c| c == b'.') {
        Some(pos) => start + pos,
        None => return false,
    };

    if dot + 3 != b.len() {
        return false;
    }
    if !b[dot + 1..].iter().all(u8::is_ascii_digit) {
        return false;
    }
    if dot == start {
        return false;
    }

    let integral = &b[start..dot];
    let valid_integral = if integral == b"0" {
        true
    } else {
        (b'1'..=b'9').contains(&integral[0])
            && integral[1..].iter().all(u8::is_ascii_digit)
    };
    if !valid_integral {
        return false;
    }

    !(b[0] == b'-' && integral == b"0" && b[dot + 1] == b'0' && b[dot + 2] == b'0')
}

fn parse_decimal(s: &str) -> Result<i64, CellErrorKind> {
    if !decimal_syntax(s) {
        return Err(CellErrorKind::InvalidDecimal);
    }

    let negative = s.starts_with('-');
    let number = if negative { &s[1..] } else { s };
    let dot = number.find('.').expect("decimal syntax checked");
    let integral = number[..dot]
        .parse::<i128>()
        .map_err(|_| CellErrorKind::DecimalOverflow)?;
    let fraction = number[dot + 1..]
        .parse::<i128>()
        .map_err(|_| CellErrorKind::InvalidDecimal)?;
    let magnitude = integral
        .checked_mul(100)
        .and_then(|v| v.checked_add(fraction))
        .ok_or(CellErrorKind::DecimalOverflow)?;
    let signed = if negative { -magnitude } else { magnitude };
    i64::try_from(signed).map_err(|_| CellErrorKind::DecimalOverflow)
}

fn evaluate_cell(
    cell: &str,
    kind: ColumnType,
    nullable: bool,
) -> Result<Value, CellErrorKind> {
    if cell.is_empty() {
        if nullable {
            return Ok(Value::Null);
        }
        if kind != ColumnType::Text {
            return Err(CellErrorKind::Empty);
        }
    }

    match kind {
        ColumnType::Text => Ok(Value::Text(cell.to_string())),
        ColumnType::Integer => parse_integer(cell).map(Value::Integer),
        ColumnType::Decimal => parse_decimal(cell).map(Value::Decimal),
        ColumnType::Boolean => match cell {
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
        let mut row_failed = false;

        for (spec_index, spec) in columns.iter().enumerate() {
            if row_failed {
                output.push(None);
                continue;
            }

            let column_index = match &spec.column {
                ColumnRef::Index(index) => Some(*index),
                ColumnRef::Name(name) => table.column_index(name),
            };

            let Some(index) = column_index else {
                output.push(None);
                result.errors.push(CellError {
                    row: row_index,
                    spec: spec_index,
                    column: None,
                    kind: CellErrorKind::UnknownColumn,
                });
                if policy != ErrorPolicy::Collect {
                    row_failed = true;
                }
                if policy == ErrorPolicy::Stop {
                    for _ in spec_index + 1..columns.len() {
                        output.push(None);
                    }
                    result.rows.push(output);
                    return result;
                }
                continue;
            };

            let Some(cell) = row.get(index) else {
                output.push(None);
                result.errors.push(CellError {
                    row: row_index,
                    spec: spec_index,
                    column: Some(index),
                    kind: CellErrorKind::MissingCell,
                });
                if policy != ErrorPolicy::Collect {
                    row_failed = true;
                }
                if policy == ErrorPolicy::Stop {
                    for _ in spec_index + 1..columns.len() {
                        output.push(None);
                    }
                    result.rows.push(output);
                    return result;
                }
                continue;
            };

            match evaluate_cell(cell, spec.kind, spec.nullable) {
                Ok(value) => output.push(Some(value)),
                Err(kind) => {
                    output.push(None);
                    result.errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column: Some(index),
                        kind,
                    });
                    if policy != ErrorPolicy::Collect {
                        row_failed = true;
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