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

pub struct StreamReader {
    config: Result<Config, CsvError>,
    input: String,
    cached: Option<Result<Table, CsvError>>,
}

fn validate_config(config: Config) -> Result<Config, CsvError> {
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
    Ok(config)
}

fn parse_with_dialect(
    input: &str,
    delimiter: u8,
    quote: u8,
) -> Result<Vec<Vec<String>>, CsvError> {
    let bytes = input.as_bytes();
    let mut pos = if input.starts_with('\u{feff}') { 3 } else { 0 };
    let mut records = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut record_index = 0usize;
    let mut record_started = false;

    #[derive(Clone, Copy)]
    enum State {
        FieldStart,
        Unquoted,
        Quoted(usize),
        AfterQuote,
    }

    let mut state = State::FieldStart;

    while pos < bytes.len() {
        let b = bytes[pos];

        if b == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: pos,
            });
        }

        if !matches!(state, State::Quoted(_)) && b == b'\r' {
            if pos + 1 >= bytes.len() || bytes[pos + 1] != b'\n' {
                return Err(CsvError::BareCr {
                    record: record_index,
                    byte: pos,
                });
            }
            row.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut row));
            record_index += 1;
            record_started = false;
            state = State::FieldStart;
            pos += 2;
            continue;
        }

        if !matches!(state, State::Quoted(_)) && b == b'\n' {
            row.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut row));
            record_index += 1;
            record_started = false;
            state = State::FieldStart;
            pos += 1;
            continue;
        }

        match state {
            State::FieldStart => {
                if b == delimiter {
                    row.push(String::new());
                    record_started = true;
                    pos += 1;
                } else if b == quote {
                    state = State::Quoted(pos);
                    record_started = true;
                    pos += 1;
                } else {
                    let ch = input[pos..].chars().next().unwrap();
                    field.push(ch);
                    state = State::Unquoted;
                    record_started = true;
                    pos += ch.len_utf8();
                }
            }
            State::Unquoted => {
                if b == delimiter {
                    row.push(std::mem::take(&mut field));
                    state = State::FieldStart;
                    record_started = true;
                    pos += 1;
                } else if b == quote {
                    return Err(CsvError::UnexpectedQuote {
                        record: record_index,
                        byte: pos,
                    });
                } else {
                    let ch = input[pos..].chars().next().unwrap();
                    field.push(ch);
                    pos += ch.len_utf8();
                }
            }
            State::Quoted(opening) => {
                if b == quote {
                    if pos + 1 < bytes.len() && bytes[pos + 1] == quote {
                        field.push(quote as char);
                        pos += 2;
                    } else {
                        state = State::AfterQuote;
                        pos += 1;
                    }
                } else {
                    let ch = input[pos..].chars().next().unwrap();
                    field.push(ch);
                    pos += ch.len_utf8();
                    let _ = opening;
                }
            }
            State::AfterQuote => {
                if b == delimiter {
                    row.push(std::mem::take(&mut field));
                    state = State::FieldStart;
                    record_started = true;
                    pos += 1;
                } else {
                    return Err(CsvError::TrailingByte {
                        record: record_index,
                        byte: pos,
                    });
                }
            }
        }
    }

    if let State::Quoted(opening) = state {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: opening,
        });
    }

    if record_started || !row.is_empty() || !field.is_empty() {
        row.push(field);
        records.push(row);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with_dialect(input, b',', b'"')
}

fn build_table(input: &str, config: Config) -> Result<Table, CsvError> {
    let records = parse_with_dialect(input, config.delimiter, config.quote)?;
    let mut header = None;
    let mut rows = records;

    if config.headers && !rows.is_empty() {
        header = Some(rows.remove(0));
        let header_values = header.as_ref().unwrap();

        if config.duplicates == DuplicatePolicy::Reject {
            for second in 0..header_values.len() {
                if let Some(first) = header_values[..second]
                    .iter()
                    .position(|value| value == &header_values[second])
                {
                    return Err(CsvError::DuplicateHeader {
                        name: header_values[second].clone(),
                        first,
                        second,
                    });
                }
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

impl StreamReader {
    pub fn new(config: Config) -> Result<Self, CsvError> {
        let validated = validate_config(config);
        Ok(Self {
            config: validated,
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

        let result = match self.config {
            Err(ref error) => Err(error.clone()),
            Ok(config) => build_table(&self.input, config),
        };
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

fn integer_syntax(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let digits = if bytes[0] == b'-' { &bytes[1..] } else { bytes };
    if digits.is_empty() {
        return false;
    }
    if digits == b"0" {
        return bytes[0] != b'-';
    }
    (b'1'..=b'9').contains(&digits[0]) && digits[1..].iter().all(u8::is_ascii_digit)
}

fn decimal_syntax(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let unsigned = if bytes[0] == b'-' { &bytes[1..] } else { bytes };
    if unsigned.len() < 4 || unsigned[unsigned.len() - 3] != b'.' {
        return false;
    }
    let integral = &unsigned[..unsigned.len() - 3];
    let fraction = &unsigned[unsigned.len() - 2..];
    if !fraction.iter().all(u8::is_ascii_digit) {
        return false;
    }
    if integral == b"0" {
        return bytes[0] != b'-' || fraction != b"00";
    }
    !integral.is_empty()
        && (b'1'..=b'9').contains(&integral[0])
        && integral[1..].iter().all(u8::is_ascii_digit)
}

fn parse_decimal(value: &str) -> Option<i64> {
    let negative = value.starts_with('-');
    let unsigned = if negative { &value[1..] } else { value };
    let dot = unsigned.len() - 3;
    let integral: i128 = unsigned[..dot].parse().ok()?;
    let fraction: i128 = unsigned[dot + 1..].parse().ok()?;
    let magnitude = integral.checked_mul(100)?.checked_add(fraction)?;
    let signed = if negative { -magnitude } else { magnitude };
    i64::try_from(signed).ok()
}

enum Evaluation {
    Success(Value),
    Failure(CellErrorKind),
}

fn evaluate_cell(text: &str, spec: &ColumnSpec) -> Evaluation {
    if text.is_empty() {
        if spec.nullable {
            return Evaluation::Success(Value::Null);
        }
        if spec.kind != ColumnType::Text {
            return Evaluation::Failure(CellErrorKind::Empty);
        }
    }

    match spec.kind {
        ColumnType::Text => Evaluation::Success(Value::Text(text.to_owned())),
        ColumnType::Integer => {
            if !integer_syntax(text) {
                Evaluation::Failure(CellErrorKind::InvalidInteger)
            } else {
                match text.parse::<i64>() {
                    Ok(value) => Evaluation::Success(Value::Integer(value)),
                    Err(_) => Evaluation::Failure(CellErrorKind::IntegerOverflow),
                }
            }
        }
        ColumnType::Decimal => {
            if !decimal_syntax(text) {
                Evaluation::Failure(CellErrorKind::InvalidDecimal)
            } else {
                match parse_decimal(text) {
                    Some(value) => Evaluation::Success(Value::Decimal(value)),
                    None => Evaluation::Failure(CellErrorKind::DecimalOverflow),
                }
            }
        }
        ColumnType::Boolean => match text {
            "yes" | "1" => Evaluation::Success(Value::Boolean(true)),
            "no" | "0" => Evaluation::Success(Value::Boolean(false)),
            _ => Evaluation::Failure(CellErrorKind::InvalidBoolean),
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
        let mut failed = false;

        for (spec_index, spec) in columns.iter().enumerate() {
            let resolved = match &spec.column {
                ColumnRef::Index(index) => Some(*index),
                ColumnRef::Name(name) => table.column_index(name),
            };

            let column = match resolved {
                Some(index) => index,
                None => {
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
                }
            };

            let Some(cell) = row.get(column) else {
                result.errors.push(CellError {
                    row: row_index,
                    spec: spec_index,
                    column: Some(column),
                    kind: CellErrorKind::MissingCell,
                });
                failed = true;
                if policy != ErrorPolicy::Collect {
                    break;
                }
                continue;
            };

            match evaluate_cell(cell, spec) {
                Evaluation::Success(value) => output[spec_index] = Some(value),
                Evaluation::Failure(kind) => {
                    result.errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column: Some(column),
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

    result
}