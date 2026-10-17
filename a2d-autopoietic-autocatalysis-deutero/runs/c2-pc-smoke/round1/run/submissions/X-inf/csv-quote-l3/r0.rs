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
    let mut row = Vec::new();
    let mut field = String::new();
    let mut i = start;
    let mut record = 0usize;
    let mut quoted = false;
    let mut after_quote = false;
    let mut opening_quote = 0usize;
    let mut field_started = false;

    while i < bytes.len() {
        let b = bytes[i];

        if b == 0 {
            return Err(CsvError::Nul { record, byte: i });
        }

        if quoted {
            if b == quote {
                if i + 1 < bytes.len() && bytes[i + 1] == quote {
                    field.push(quote as char);
                    i += 2;
                } else {
                    quoted = false;
                    after_quote = true;
                    i += 1;
                }
            } else {
                let ch = input[i..].chars().next().unwrap();
                field.push(ch);
                i += ch.len_utf8();
            }
            continue;
        }

        if b == b'\r' && (i + 1 >= bytes.len() || bytes[i + 1] != b'\n') {
            return Err(CsvError::BareCr { record, byte: i });
        }

        if after_quote {
            if b == delimiter {
                row.push(std::mem::take(&mut field));
                after_quote = false;
                field_started = false;
                i += 1;
            } else if b == b'\n' || b == b'\r' {
                row.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut row));
                record += 1;
                after_quote = false;
                field_started = false;
                i += if b == b'\r' { 2 } else { 1 };
            } else {
                return Err(CsvError::TrailingByte { record, byte: i });
            }
            continue;
        }

        if b == quote {
            if field_started {
                return Err(CsvError::UnexpectedQuote { record, byte: i });
            }
            quoted = true;
            opening_quote = i;
            field_started = true;
            i += 1;
        } else if b == delimiter {
            row.push(std::mem::take(&mut field));
            field_started = false;
            i += 1;
        } else if b == b'\n' || b == b'\r' {
            row.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut row));
            record += 1;
            field_started = false;
            i += if b == b'\r' { 2 } else { 1 };
        } else {
            let ch = input[i..].chars().next().unwrap();
            field.push(ch);
            field_started = true;
            i += ch.len_utf8();
        }
    }

    if quoted {
        return Err(CsvError::UnclosedQuote {
            record,
            byte: opening_quote,
        });
    }

    if after_quote || field_started || !field.is_empty() || !row.is_empty() {
        row.push(field);
        records.push(row);
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
            for second in 0..parsed_header.len() {
                if let Some(first) = parsed_header[..second]
                    .iter()
                    .position(|name| name == &parsed_header[second])
                {
                    return Err(CsvError::DuplicateHeader {
                        name: parsed_header[second].clone(),
                        first,
                        second,
                    });
                }
            }
        }
        header = Some(parsed_header);
    }

    if let Some(ref names) = header {
        for (index, row) in rows.iter().enumerate() {
            if row.len() != names.len() {
                return Err(CsvError::HeaderWidth {
                    record: index + 1,
                    expected: names.len(),
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

fn parse_integer(s: &str) -> Result<i64, CellErrorKind> {
    if !integer_syntax(s) {
        return Err(CellErrorKind::InvalidInteger);
    }
    s.parse::<i64>().map_err(|_| CellErrorKind::IntegerOverflow)
}

fn decimal_syntax(s: &str) -> bool {
    let b = s.as_bytes();
    let digits = if b.first() == Some(&b'-') {
        &b[1..]
    } else {
        b
    };
    let dot = match digits.iter().position(|&x| x == b'.') {
        Some(pos) => pos,
        None => return false,
    };
    let integral = &digits[..dot];
    let fractional = &digits[dot + 1..];
    if fractional.len() != 2 || !fractional.iter().all(u8::is_ascii_digit) {
        return false;
    }
    if integral == b"0" {
        return b.first() != Some(&b'-') || fractional != b"00";
    }
    !integral.is_empty()
        && integral[0].is_ascii_digit()
        && integral[0] != b'0'
        && integral.iter().all(u8::is_ascii_digit)
}

fn parse_decimal(s: &str) -> Result<i64, CellErrorKind> {
    if !decimal_syntax(s) {
        return Err(CellErrorKind::InvalidDecimal);
    }
    let negative = s.starts_with('-');
    let body = if negative { &s[1..] } else { s };
    let dot = body.find('.').unwrap();
    let whole: i128 = body[..dot].parse().unwrap();
    let frac: i128 = body[dot + 1..].parse().unwrap();
    let magnitude = whole * 100 + frac;
    let signed = if negative { -magnitude } else { magnitude };
    i64::try_from(signed).map_err(|_| CellErrorKind::DecimalOverflow)
}

fn evaluate_cell(
    cell: &str,
    spec: &ColumnSpec,
) -> Result<Value, CellErrorKind> {
    if cell.is_empty() {
        if spec.nullable {
            return Ok(Value::Null);
        }
        if spec.kind != ColumnType::Text {
            return Err(CellErrorKind::Empty);
        }
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

pub fn extract_columns(
    table: &Table,
    columns: &[ColumnSpec],
    policy: ErrorPolicy,
) -> TypedResult {
    let mut output = Vec::with_capacity(table.rows.len());
    let mut errors = Vec::new();

    for (row_index, row) in table.rows.iter().enumerate() {
        let mut values = vec![None; columns.len()];
        let mut stop_row = false;

        for (spec_index, spec) in columns.iter().enumerate() {
            if stop_row {
                break;
            }

            let resolved = match &spec.column {
                ColumnRef::Index(index) => Some(*index),
                ColumnRef::Name(name) => table.column_index(name),
            };

            let outcome = match resolved {
                None => Err((None, CellErrorKind::UnknownColumn)),
                Some(index) => match row.get(index) {
                    None => Err((Some(index), CellErrorKind::MissingCell)),
                    Some(cell) => evaluate_cell(cell, spec).map_err(|kind| (Some(index), kind)),
                },
            };

            match outcome {
                Ok(value) => values[spec_index] = Some(value),
                Err((column, kind)) => {
                    errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column,
                        kind,
                    });
                    match policy {
                        ErrorPolicy::Collect => {}
                        ErrorPolicy::FirstPerRow => stop_row = true,
                        ErrorPolicy::Stop => {
                            output.push(values);
                            return TypedResult { rows: output, errors };
                        }
                    }
                }
            }
        }

        output.push(values);
    }

    TypedResult {
        rows: output,
        errors,
    }
}