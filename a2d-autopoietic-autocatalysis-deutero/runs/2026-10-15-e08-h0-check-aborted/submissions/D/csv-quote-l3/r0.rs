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
            DuplicatePolicy::KeepLast => header.iter().rposition(|n| n == name),
            DuplicatePolicy::Reject | DuplicatePolicy::KeepFirst => {
                header.iter().position(|n| n == name)
            }
        }
    }
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
        let result = parse_with_config(&self.input, self.config);
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

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_records(input, b',', b'"')
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

fn parse_with_config(input: &str, config: Config) -> Result<Table, CsvError> {
    validate_config(config)?;
    let records = parse_records(input, config.delimiter, config.quote)?;

    let (header, rows) = if config.headers {
        if records.is_empty() {
            (None, Vec::new())
        } else {
            let mut iter = records.into_iter();
            (iter.next(), iter.collect())
        }
    } else {
        (None, records)
    };

    if let Some(header_values) = &header {
        if config.duplicates == DuplicatePolicy::Reject {
            let mut seen: std::collections::HashMap<&str, usize> =
                std::collections::HashMap::new();
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

        for (index, row) in rows.iter().enumerate() {
            if row.len() != header_values.len() {
                return Err(CsvError::HeaderWidth {
                    record: index + 1,
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

fn parse_records(input: &str, delimiter: u8, quote: u8) -> Result<Vec<Vec<String>>, CsvError> {
    let bytes = input.as_bytes();
    let mut i = if input.starts_with('\u{feff}') { 3 } else { 0 };
    if i == bytes.len() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = Vec::new();
    let mut record_index = 0usize;
    let mut at_field_start = true;
    let mut in_quotes = false;
    let mut after_quote = false;
    let mut opening_quote = 0usize;
    let mut record_has_content = false;

    while i < bytes.len() {
        let b = bytes[i];

        if b == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: i,
            });
        }

        if !in_quotes && b == b'\r' {
            if bytes.get(i + 1) != Some(&b'\n') {
                return Err(CsvError::BareCr {
                    record: record_index,
                    byte: i,
                });
            }
        }

        if in_quotes {
            if b == quote {
                if bytes.get(i + 1) == Some(&quote) {
                    field.push(quote);
                    i += 2;
                    continue;
                }
                in_quotes = false;
                after_quote = true;
                i += 1;
                continue;
            }
            field.push(b);
            i += 1;
            continue;
        }

        if after_quote {
            if b == delimiter {
                record.push(String::from_utf8(field).expect("input is valid UTF-8"));
                field = Vec::new();
                at_field_start = true;
                after_quote = false;
                record_has_content = true;
                i += 1;
                continue;
            }
            if b == b'\n' || b == b'\r' {
                record.push(String::from_utf8(field).expect("input is valid UTF-8"));
                records.push(record);
                record = Vec::new();
                field = Vec::new();
                at_field_start = true;
                after_quote = false;
                record_has_content = false;
                record_index += 1;
                i += if b == b'\r' { 2 } else { 1 };
                continue;
            }
            return Err(CsvError::TrailingByte {
                record: record_index,
                byte: i,
            });
        }

        if at_field_start && b == quote {
            in_quotes = true;
            opening_quote = i;
            at_field_start = false;
            record_has_content = true;
            i += 1;
            continue;
        }

        if b == quote {
            return Err(CsvError::UnexpectedQuote {
                record: record_index,
                byte: i,
            });
        }

        if b == delimiter {
            record.push(String::from_utf8(field).expect("input is valid UTF-8"));
            field = Vec::new();
            at_field_start = true;
            record_has_content = true;
            i += 1;
            continue;
        }

        if b == b'\n' || b == b'\r' {
            record.push(String::from_utf8(field).expect("input is valid UTF-8"));
            records.push(record);
            record = Vec::new();
            field = Vec::new();
            at_field_start = true;
            record_has_content = false;
            record_index += 1;
            i += if b == b'\r' { 2 } else { 1 };
            continue;
        }

        field.push(b);
        at_field_start = false;
        record_has_content = true;
        i += 1;
    }

    if in_quotes {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: opening_quote,
        });
    }

    if record_has_content || !record.is_empty() || !field.is_empty() || after_quote {
        record.push(String::from_utf8(field).expect("input is valid UTF-8"));
        records.push(record);
    }

    Ok(records)
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
        let mut output = vec![None; columns.len()];
        let mut failed = false;

        for (spec_index, spec) in columns.iter().enumerate() {
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
                failed = true;
                if policy != ErrorPolicy::Collect {
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
                failed = true;
                if policy != ErrorPolicy::Collect {
                    break;
                }
                continue;
            };

            match convert_cell(cell, spec) {
                Ok(value) => output[spec_index] = Some(value),
                Err(kind) => {
                    result.errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column: Some(column_index),
                        kind,
                    });
                    failed = true;
                    if policy != ErrorPolicy::Collect {
                        break;
                    }
                }
            }
        }

        result.rows.push(output);
        if failed && policy == ErrorPolicy::Stop {
            break;
        }
    }

    TypedResult {
        rows: result.rows,
        errors: result.errors,
    }
}

fn convert_cell(cell: &str, spec: &ColumnSpec) -> Result<Value, CellErrorKind> {
    if cell.is_empty() {
        if spec.nullable {
            return Ok(Value::Null);
        }
        if spec.kind != ColumnType::Text {
            return Err(CellErrorKind::Empty);
        }
        return Ok(Value::Text(String::new()));
    }

    match spec.kind {
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

fn integer_syntax(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let mut i = 0;
    if bytes[0] == b'-' {
        i = 1;
        if i == bytes.len() {
            return false;
        }
    }
    if bytes[i] == b'0' {
        return i + 1 == bytes.len() && i == 0;
    }
    if !(b'1'..=b'9').contains(&bytes[i]) {
        return false;
    }
    i += 1;
    bytes[i..].iter().all(u8::is_ascii_digit)
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
    if bytes.len() < 4 {
        return false;
    }
    let mut i = 0;
    if bytes[0] == b'-' {
        i = 1;
    }
    if i >= bytes.len() {
        return false;
    }
    let integer_start = i;
    if bytes[i] == b'0' {
        i += 1;
        if i < bytes.len() && bytes[i].is_ascii_digit() {
            return false;
        }
    } else if (b'1'..=b'9').contains(&bytes[i]) {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
    } else {
        return false;
    }

    if bytes.get(i) != Some(&b'.') || bytes.len() != i + 3 {
        return false;
    }
    if !bytes[i + 1].is_ascii_digit() || !bytes[i + 2].is_ascii_digit() {
        return false;
    }

    if bytes[0] == b'-'
        && bytes[integer_start] == b'0'
        && bytes[i + 1] == b'0'
        && bytes[i + 2] == b'0'
    {
        return false;
    }
    true
}

fn parse_decimal(s: &str) -> Result<i64, CellErrorKind> {
    if !decimal_syntax(s) {
        return Err(CellErrorKind::InvalidDecimal);
    }

    let negative = s.starts_with('-');
    let digits: String = s
        .bytes()
        .filter(|b| *b != b'-' && *b != b'.')
        .map(char::from)
        .collect();
    let magnitude = digits
        .parse::<i128>()
        .map_err(|_| CellErrorKind::DecimalOverflow)?;
    let signed = if negative { -magnitude } else { magnitude };
    i64::try_from(signed).map_err(|_| CellErrorKind::DecimalOverflow)
}