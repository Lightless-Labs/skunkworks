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
            DuplicatePolicy::KeepLast => header.iter().rposition(|value| value == name),
            DuplicatePolicy::KeepFirst | DuplicatePolicy::Reject => {
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
        let result = parse_with_config(&self.input, self.config);
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
            let mut first_seen: HashMap<&str, usize> = HashMap::new();
            for (second, name) in header_values.iter().enumerate() {
                if let Some(&first) = first_seen.get(name.as_str()) {
                    return Err(CsvError::DuplicateHeader {
                        name: name.clone(),
                        first,
                        second,
                    });
                }
                first_seen.insert(name.as_str(), second);
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
    let mut pos = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        3
    } else {
        0
    };

    if pos == bytes.len() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    let mut fields = Vec::new();
    let mut field = Vec::new();
    let mut record_index = 0usize;

    #[derive(Clone, Copy)]
    enum State {
        FieldStart,
        Unquoted,
        Quoted { opening: usize },
        AfterQuote,
    }

    let mut state = State::FieldStart;

    while pos < bytes.len() {
        let byte = bytes[pos];

        if byte == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: pos,
            });
        }

        match state {
            State::Quoted { opening } => {
                if byte == quote {
                    if pos + 1 < bytes.len() && bytes[pos + 1] == quote {
                        field.push(quote);
                        pos += 2;
                    } else {
                        state = State::AfterQuote;
                        pos += 1;
                    }
                } else {
                    field.push(byte);
                    pos += 1;
                }
                let _ = opening;
            }
            State::AfterQuote => {
                if byte == b'\r' && (pos + 1 >= bytes.len() || bytes[pos + 1] != b'\n') {
                    return Err(CsvError::BareCr {
                        record: record_index,
                        byte: pos,
                    });
                }
                if byte == delimiter {
                    fields.push(bytes_to_string(std::mem::take(&mut field)));
                    state = State::FieldStart;
                    pos += 1;
                } else if byte == b'\n' || byte == b'\r' {
                    fields.push(bytes_to_string(std::mem::take(&mut field)));
                    records.push(std::mem::take(&mut fields));
                    record_index += 1;
                    state = State::FieldStart;
                    pos += if byte == b'\r' { 2 } else { 1 };
                } else {
                    return Err(CsvError::TrailingByte {
                        record: record_index,
                        byte: pos,
                    });
                }
            }
            State::FieldStart | State::Unquoted => {
                if byte == b'\r' && (pos + 1 >= bytes.len() || bytes[pos + 1] != b'\n') {
                    return Err(CsvError::BareCr {
                        record: record_index,
                        byte: pos,
                    });
                }
                if byte == delimiter {
                    fields.push(bytes_to_string(std::mem::take(&mut field)));
                    state = State::FieldStart;
                    pos += 1;
                } else if byte == b'\n' || byte == b'\r' {
                    fields.push(bytes_to_string(std::mem::take(&mut field)));
                    records.push(std::mem::take(&mut fields));
                    record_index += 1;
                    state = State::FieldStart;
                    pos += if byte == b'\r' { 2 } else { 1 };
                } else if byte == quote {
                    match state {
                        State::FieldStart => {
                            state = State::Quoted { opening: pos };
                            pos += 1;
                        }
                        State::Unquoted => {
                            return Err(CsvError::UnexpectedQuote {
                                record: record_index,
                                byte: pos,
                            });
                        }
                        _ => unreachable!(),
                    }
                } else {
                    field.push(byte);
                    state = State::Unquoted;
                    pos += 1;
                }
            }
        }
    }

    if let State::Quoted { opening } = state {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: opening,
        });
    }

    if pos > 0 && bytes[pos - 1] != b'\n' {
        fields.push(bytes_to_string(field));
        records.push(fields);
    }

    Ok(records)
}

fn bytes_to_string(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).expect("CSV parsing preserves valid UTF-8")
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
        let mut row_failed = false;

        for (spec_index, spec) in columns.iter().enumerate() {
            if row_failed {
                break;
            }

            let resolved = match &spec.column {
                ColumnRef::Index(index) => Some(*index),
                ColumnRef::Name(name) => table.column_index(name),
            };

            let (column, value_or_error) = match resolved {
                None => (None, Err(CellErrorKind::UnknownColumn)),
                Some(index) => match row.get(index) {
                    None => (Some(index), Err(CellErrorKind::MissingCell)),
                    Some(cell) => (Some(index), parse_cell(cell, spec)),
                },
            };

            match value_or_error {
                Ok(value) => output[spec_index] = Some(value),
                Err(kind) => {
                    result.errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column,
                        kind,
                    });
                    if policy != ErrorPolicy::Collect {
                        row_failed = true;
                    }
                }
            }
        }

        result.rows.push(output);
        if policy == ErrorPolicy::Stop && row_failed {
            break;
        }
    }

    result
}

fn parse_cell(cell: &str, spec: &ColumnSpec) -> Result<Value, CellErrorKind> {
    if cell.is_empty() {
        if spec.nullable {
            return Ok(Value::Null);
        }
        if spec.kind != ColumnType::Text {
            return Err(CellErrorKind::Empty);
        }
    }

    match spec.kind {
        ColumnType::Text => Ok(Value::Text(cell.to_owned())),
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

    let digits = if bytes[0] == b'-' { &bytes[1..] } else { bytes };
    if digits.is_empty() {
        return false;
    }
    if digits == b"0" {
        return bytes[0] != b'-';
    }

    (b'1'..=b'9').contains(&digits[0])
        && digits[1..].iter().all(|byte| byte.is_ascii_digit())
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
    let Some(dot) = bytes.iter().position(|&byte| byte == b'.') else {
        return false;
    };
    if bytes[dot + 1..].len() != 2
        || !bytes[dot + 1..].iter().all(u8::is_ascii_digit)
    {
        return false;
    }

    let integer = &bytes[..dot];
    let (negative, digits) = if integer.first() == Some(&b'-') {
        (true, &integer[1..])
    } else {
        (false, integer)
    };

    if digits.is_empty() {
        return false;
    }

    if digits == b"0" {
        return !negative || bytes[dot + 1..] != *b"00";
    }

    (b'1'..=b'9').contains(&digits[0])
        && digits[1..].iter().all(u8::is_ascii_digit)
}

fn parse_decimal(s: &str) -> Result<i64, CellErrorKind> {
    if !decimal_syntax(s) {
        return Err(CellErrorKind::InvalidDecimal);
    }

    let negative = s.starts_with('-');
    let unsigned = if negative { &s[1..] } else { s };
    let dot = unsigned.find('.').expect("validated decimal has dot");
    let whole = &unsigned[..dot];
    let fraction = &unsigned[dot + 1..];

    let mut magnitude: i128 = 0;
    for byte in whole.bytes().chain(fraction.bytes()) {
        magnitude = magnitude * 10 + (byte - b'0') as i128;
    }

    let signed = if negative { -magnitude } else { magnitude };
    i64::try_from(signed).map_err(|_| CellErrorKind::DecimalOverflow)
}

fn main() {}