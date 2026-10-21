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
    finished: Option<Result<Table, CsvError>>,
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

    if start == bytes.len() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut index = start;
    let mut record = 0usize;
    let mut record_started = false;

    #[derive(Clone, Copy)]
    enum State {
        Start,
        Unquoted,
        Quoted { opening: usize },
        AfterQuote,
    }

    let mut state = State::Start;

    while index < bytes.len() {
        let byte = bytes[index];

        if byte == 0 {
            return Err(CsvError::Nul { record, byte: index });
        }

        let outside = !matches!(state, State::Quoted { .. });
        if outside && byte == b'\r' && bytes.get(index + 1) != Some(&b'\n') {
            return Err(CsvError::BareCr { record, byte: index });
        }

        match state {
            State::Start => {
                if byte == quote {
                    record_started = true;
                    state = State::Quoted { opening: index };
                    index += 1;
                } else if byte == delimiter {
                    row.push(String::new());
                    record_started = true;
                    index += 1;
                } else if byte == b'\n' || byte == b'\r' {
                    row.push(String::new());
                    records.push(std::mem::take(&mut row));
                    record += 1;
                    record_started = false;
                    index += if byte == b'\r' { 2 } else { 1 };
                } else {
                    let ch = input[index..].chars().next().unwrap();
                    field.push(ch);
                    record_started = true;
                    state = State::Unquoted;
                    index += ch.len_utf8();
                }
            }
            State::Unquoted => {
                if byte == quote {
                    return Err(CsvError::UnexpectedQuote { record, byte: index });
                } else if byte == delimiter {
                    row.push(std::mem::take(&mut field));
                    state = State::Start;
                    record_started = true;
                    index += 1;
                } else if byte == b'\n' || byte == b'\r' {
                    row.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut row));
                    record += 1;
                    record_started = false;
                    state = State::Start;
                    index += if byte == b'\r' { 2 } else { 1 };
                } else {
                    let ch = input[index..].chars().next().unwrap();
                    field.push(ch);
                    index += ch.len_utf8();
                }
            }
            State::Quoted { .. } => {
                if byte == quote {
                    if bytes.get(index + 1) == Some(&quote) {
                        field.push(quote as char);
                        index += 2;
                    } else {
                        state = State::AfterQuote;
                        index += 1;
                    }
                } else {
                    let ch = input[index..].chars().next().unwrap();
                    field.push(ch);
                    index += ch.len_utf8();
                }
            }
            State::AfterQuote => {
                if byte == delimiter {
                    row.push(std::mem::take(&mut field));
                    state = State::Start;
                    record_started = true;
                    index += 1;
                } else if byte == b'\n' || byte == b'\r' {
                    row.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut row));
                    record += 1;
                    record_started = false;
                    state = State::Start;
                    index += if byte == b'\r' { 2 } else { 1 };
                } else {
                    return Err(CsvError::TrailingByte { record, byte: index });
                }
            }
        }
    }

    match state {
        State::Quoted { opening } => {
            return Err(CsvError::UnclosedQuote {
                record,
                byte: opening,
            });
        }
        State::AfterQuote | State::Unquoted => {
            row.push(field);
            records.push(row);
        }
        State::Start if record_started || !row.is_empty() => {
            row.push(String::new());
            records.push(row);
        }
        State::Start => {}
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with_dialect(input, b',', b'"')
}

fn build_table(records: Vec<Vec<String>>, config: Config) -> Result<Table, CsvError> {
    let mut header = None;
    let mut rows = records;

    if config.headers && !rows.is_empty() {
        let first = rows.remove(0);

        if config.duplicates == DuplicatePolicy::Reject {
            for second in 0..first.len() {
                if let Some(previous) = first[..second]
                    .iter()
                    .position(|name| name == &first[second])
                {
                    return Err(CsvError::DuplicateHeader {
                        name: first[second].clone(),
                        first: previous,
                        second,
                    });
                }
            }
        }

        for (data_index, row) in rows.iter().enumerate() {
            if row.len() != first.len() {
                return Err(CsvError::HeaderWidth {
                    record: data_index + 1,
                    expected: first.len(),
                    actual: row.len(),
                });
            }
        }

        header = Some(first);
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
            .and_then(|records| build_table(records, self.config));
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
    let digits = if bytes.first() == Some(&b'-') {
        &bytes[1..]
    } else {
        bytes
    };
    if digits.is_empty() {
        return false;
    }
    if digits == b"0" {
        return bytes.first() != Some(&b'-');
    }
    digits[0].is_ascii_digit()
        && digits[0] != b'0'
        && digits.iter().all(u8::is_ascii_digit)
}

fn parse_integer(value: &str) -> Result<i64, CellErrorKind> {
    if !integer_syntax(value) {
        return Err(CellErrorKind::InvalidInteger);
    }
    value
        .parse::<i64>()
        .map_err(|_| CellErrorKind::IntegerOverflow)
}

fn decimal_syntax(value: &str) -> bool {
    let bytes = value.as_bytes();
    let body = if bytes.first() == Some(&b'-') {
        &bytes[1..]
    } else {
        bytes
    };
    let Some(dot) = body.iter().position(|&byte| byte == b'.') else {
        return false;
    };
    if body[dot + 1..].len() != 2 || !body[dot + 1..].iter().all(u8::is_ascii_digit) {
        return false;
    }
    let integral = &body[..dot];
    if integral.is_empty() {
        return false;
    }
    if integral == b"0" {
        return bytes.first() != Some(&b'-') || body[dot + 1..] != *b"00";
    }
    integral[0].is_ascii_digit()
        && integral[0] != b'0'
        && integral.iter().all(u8::is_ascii_digit)
}

fn parse_decimal(value: &str) -> Result<i64, CellErrorKind> {
    if !decimal_syntax(value) {
        return Err(CellErrorKind::InvalidDecimal);
    }
    let negative = value.starts_with('-');
    let unsigned = if negative { &value[1..] } else { value };
    let dot = unsigned.find('.').unwrap();
    let whole = unsigned[..dot]
        .parse::<i128>()
        .map_err(|_| CellErrorKind::DecimalOverflow)?;
    let fraction = unsigned[dot + 1..]
        .parse::<i128>()
        .unwrap();
    let magnitude = whole
        .checked_mul(100)
        .and_then(|number| number.checked_add(fraction))
        .ok_or(CellErrorKind::DecimalOverflow)?;
    let signed = if negative { -magnitude } else { magnitude };
    i64::try_from(signed).map_err(|_| CellErrorKind::DecimalOverflow)
}

fn parse_value(value: &str, kind: ColumnType) -> Result<Value, CellErrorKind> {
    match kind {
        ColumnType::Text => Ok(Value::Text(value.to_string())),
        ColumnType::Integer => parse_integer(value).map(Value::Integer),
        ColumnType::Decimal => parse_decimal(value).map(Value::Decimal),
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
        let mut row_failed = false;

        for (spec_index, spec) in columns.iter().enumerate() {
            if row_failed {
                output.push(None);
                continue;
            }

            let resolved = match &spec.column {
                ColumnRef::Index(index) => Some(*index),
                ColumnRef::Name(name) => table.column_index(name),
            };

            let error = if resolved.is_none() {
                Some(CellError {
                    row: row_index,
                    spec: spec_index,
                    column: None,
                    kind: CellErrorKind::UnknownColumn,
                })
            } else {
                let column = resolved.unwrap();
                match row.get(column) {
                    None => Some(CellError {
                        row: row_index,
                        spec: spec_index,
                        column: Some(column),
                        kind: CellErrorKind::MissingCell,
                    }),
                    Some(cell) if cell.is_empty() && spec.nullable => {
                        output.push(Some(Value::Null));
                        None
                    }
                    Some(cell) if cell.is_empty() && spec.kind != ColumnType::Text => {
                        Some(CellError {
                            row: row_index,
                            spec: spec_index,
                            column: Some(column),
                            kind: CellErrorKind::Empty,
                        })
                    }
                    Some(cell) => match parse_value(cell, spec.kind) {
                        Ok(value) => {
                            output.push(Some(value));
                            None
                        }
                        Err(kind) => Some(CellError {
                            row: row_index,
                            spec: spec_index,
                            column: Some(column),
                            kind,
                        }),
                    },
                }
            };

            if let Some(error) = error {
                output.push(None);
                result.errors.push(error);
                match policy {
                    ErrorPolicy::Collect => {}
                    ErrorPolicy::FirstPerRow => row_failed = true,
                    ErrorPolicy::Stop => {
                        output.resize(columns.len(), None);
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