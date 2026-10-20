use std::collections::HashMap;

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

fn parse_with_config(input: &str, delimiter: u8, quote: u8) -> Result<Vec<Vec<String>>, CsvError> {
    let bytes = input.as_bytes();
    let offset = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        3
    } else {
        0
    };

    let mut records = Vec::new();
    let mut fields = Vec::new();
    let mut field = Vec::new();
    let mut index = offset;
    let mut record = 0usize;
    let mut record_started = false;

    #[derive(Clone, Copy)]
    enum State {
        Start,
        Unquoted,
        Quoted(usize),
        AfterQuote,
    }

    let mut state = State::Start;

    while index < bytes.len() {
        let byte = bytes[index];

        if byte == 0 {
            return Err(CsvError::Nul {
                record,
                byte: index,
            });
        }

        let outside_quotes = !matches!(state, State::Quoted(_));
        if outside_quotes && byte == b'\r' {
            if bytes.get(index + 1) != Some(&b'\n') {
                return Err(CsvError::BareCr {
                    record,
                    byte: index,
                });
            }
        }

        match state {
            State::Start => {
                if byte == quote {
                    state = State::Quoted(index);
                    record_started = true;
                    index += 1;
                } else if byte == delimiter {
                    fields.push(String::new());
                    record_started = true;
                    index += 1;
                } else if byte == b'\n' || byte == b'\r' {
                    fields.push(String::new());
                    records.push(std::mem::take(&mut fields));
                    record += 1;
                    record_started = false;
                    index += if byte == b'\r' { 2 } else { 1 };
                } else {
                    field.push(byte);
                    state = State::Unquoted;
                    record_started = true;
                    index += 1;
                }
            }
            State::Unquoted => {
                if byte == quote {
                    return Err(CsvError::UnexpectedQuote {
                        record,
                        byte: index,
                    });
                } else if byte == delimiter {
                    fields.push(String::from_utf8(std::mem::take(&mut field)).unwrap());
                    state = State::Start;
                    record_started = true;
                    index += 1;
                } else if byte == b'\n' || byte == b'\r' {
                    fields.push(String::from_utf8(std::mem::take(&mut field)).unwrap());
                    records.push(std::mem::take(&mut fields));
                    record += 1;
                    record_started = false;
                    state = State::Start;
                    index += if byte == b'\r' { 2 } else { 1 };
                } else {
                    field.push(byte);
                    index += 1;
                }
            }
            State::Quoted(opening) => {
                if byte == quote {
                    if bytes.get(index + 1) == Some(&quote) {
                        field.push(quote);
                        index += 2;
                    } else {
                        state = State::AfterQuote;
                        index += 1;
                    }
                } else {
                    field.push(byte);
                    index += 1;
                    let _ = opening;
                }
            }
            State::AfterQuote => {
                if byte == delimiter {
                    fields.push(String::from_utf8(std::mem::take(&mut field)).unwrap());
                    state = State::Start;
                    record_started = true;
                    index += 1;
                } else if byte == b'\n' || byte == b'\r' {
                    fields.push(String::from_utf8(std::mem::take(&mut field)).unwrap());
                    records.push(std::mem::take(&mut fields));
                    record += 1;
                    record_started = false;
                    state = State::Start;
                    index += if byte == b'\r' { 2 } else { 1 };
                } else {
                    return Err(CsvError::TrailingByte {
                        record,
                        byte: index,
                    });
                }
            }
        }
    }

    if let State::Quoted(opening) = state {
        return Err(CsvError::UnclosedQuote {
            record,
            byte: opening,
        });
    }

    if record_started || !fields.is_empty() || !field.is_empty() {
        fields.push(String::from_utf8(field).unwrap());
        records.push(fields);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with_config(input, b',', b'"')
}

fn build_table(records: Vec<Vec<String>>, config: Config) -> Result<Table, CsvError> {
    let mut records = records.into_iter();
    let header = if config.headers {
        records.next()
    } else {
        None
    };

    if let Some(ref header_values) = header {
        if config.duplicates == DuplicatePolicy::Reject {
            let mut seen: HashMap<&str, usize> = HashMap::new();
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
    }

    let rows: Vec<Vec<String>> = records.collect();
    if let Some(ref header_values) = header {
        for (row_index, row) in rows.iter().enumerate() {
            let record = row_index + 1;
            if row.len() != header_values.len() {
                return Err(CsvError::HeaderWidth {
                    record,
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
        if self.result.is_none() {
            let parsed = parse_with_config(&self.input, self.config.delimiter, self.config.quote);
            self.result = Some(match parsed {
                Ok(records) => build_table(records, self.config),
                Err(error) => Err(error),
            });
        }
        self.result.as_ref().unwrap().clone()
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

fn integer_syntax(value: &str) -> bool {
    let bytes = value.as_bytes();
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
    (b'1'..=b'9').contains(&digits[0]) && digits[1..].iter().all(u8::is_ascii_digit)
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
    if body[dot + 1..].len() != 2 {
        return false;
    }
    let whole = &body[..dot];
    if whole.is_empty()
        || !body[dot + 1..].iter().all(u8::is_ascii_digit)
        || !whole.iter().all(u8::is_ascii_digit)
    {
        return false;
    }
    if whole[0] == b'0' {
        if whole.len() != 1 {
            return false;
        }
        if bytes.first() == Some(&b'-') && body[dot + 1..] == *b"00" {
            return false;
        }
        true
    } else {
        (b'1'..=b'9').contains(&whole[0])
    }
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
    let fraction = unsigned[dot + 1..].parse::<i128>().unwrap();
    let magnitude = whole
        .checked_mul(100)
        .and_then(|v| v.checked_add(fraction))
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
        let mut output = vec![None; columns.len()];
        let mut row_failed = false;

        for (spec_index, spec) in columns.iter().enumerate() {
            let column = match &spec.column {
                ColumnRef::Index(index) => Some(*index),
                ColumnRef::Name(name) => table.column_index(name),
            };

            let Some(column_index) = column else {
                result.errors.push(CellError {
                    row: row_index,
                    spec: spec_index,
                    column: None,
                    kind: CellErrorKind::UnknownColumn,
                });
                row_failed = true;
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
                row_failed = true;
                if policy != ErrorPolicy::Collect {
                    break;
                }
                continue;
            };

            let value = if cell.is_empty() {
                if spec.nullable {
                    Some(Value::Null)
                } else if spec.kind == ColumnType::Text {
                    Some(Value::Text(String::new()))
                } else {
                    result.errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column: Some(column_index),
                        kind: CellErrorKind::Empty,
                    });
                    row_failed = true;
                    if policy != ErrorPolicy::Collect {
                        break;
                    }
                    continue;
                }
            } else {
                let parsed = match spec.kind {
                    ColumnType::Text => Ok(Value::Text(cell.clone())),
                    ColumnType::Integer => parse_integer(cell).map(Value::Integer),
                    ColumnType::Decimal => parse_decimal(cell).map(Value::Decimal),
                    ColumnType::Boolean => match cell.as_str() {
                        "yes" | "1" => Ok(Value::Boolean(true)),
                        "no" | "0" => Ok(Value::Boolean(false)),
                        _ => Err(CellErrorKind::InvalidBoolean),
                    },
                };

                match parsed {
                    Ok(value) => Some(value),
                    Err(kind) => {
                        result.errors.push(CellError {
                            row: row_index,
                            spec: spec_index,
                            column: Some(column_index),
                            kind,
                        });
                        row_failed = true;
                        if policy != ErrorPolicy::Collect {
                            break;
                        }
                        continue;
                    }
                }
            };
            output[spec_index] = value;
        }

        result.rows.push(output);
        if policy == ErrorPolicy::Stop && row_failed {
            break;
        }
    }

    result
}