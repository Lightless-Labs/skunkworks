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

#[derive(Clone, Copy, PartialEq, Eq)]
enum CsvState {
    FieldStart,
    Unquoted,
    Quoted,
    AfterQuote,
}

fn parse_csv_with(input: &str, delimiter: u8, quote: u8) -> Result<Vec<Vec<String>>, CsvError> {
    let bytes = input.as_bytes();
    let offset = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) { 3 } else { 0 };
    if offset == bytes.len() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    let mut row = Vec::new();
    let mut field = Vec::new();
    let mut state = CsvState::FieldStart;
    let mut quote_open = 0usize;
    let mut record = 0usize;
    let mut i = offset;

    while i < bytes.len() {
        let b = bytes[i];

        if b == 0 {
            return Err(CsvError::Nul { record, byte: i });
        }

        let outside = state != CsvState::Quoted;
        if outside && b == b'\r' && (i + 1 >= bytes.len() || bytes[i + 1] != b'\n') {
            return Err(CsvError::BareCr { record, byte: i });
        }

        match state {
            CsvState::FieldStart => {
                if b == quote {
                    quote_open = i;
                    state = CsvState::Quoted;
                    i += 1;
                } else if b == delimiter {
                    row.push(String::new());
                    i += 1;
                } else if b == b'\n' {
                    row.push(String::new());
                    records.push(std::mem::take(&mut row));
                    record += 1;
                    i += 1;
                } else if b == b'\r' {
                    row.push(String::new());
                    records.push(std::mem::take(&mut row));
                    record += 1;
                    i += 2;
                } else {
                    field.push(b);
                    state = CsvState::Unquoted;
                    i += 1;
                }
            }
            CsvState::Unquoted => {
                if b == quote {
                    return Err(CsvError::UnexpectedQuote { record, byte: i });
                } else if b == delimiter {
                    row.push(String::from_utf8(std::mem::take(&mut field)).unwrap());
                    state = CsvState::FieldStart;
                    i += 1;
                } else if b == b'\n' {
                    row.push(String::from_utf8(std::mem::take(&mut field)).unwrap());
                    records.push(std::mem::take(&mut row));
                    record += 1;
                    state = CsvState::FieldStart;
                    i += 1;
                } else if b == b'\r' {
                    row.push(String::from_utf8(std::mem::take(&mut field)).unwrap());
                    records.push(std::mem::take(&mut row));
                    record += 1;
                    state = CsvState::FieldStart;
                    i += 2;
                } else {
                    field.push(b);
                    i += 1;
                }
            }
            CsvState::Quoted => {
                if b == quote {
                    if i + 1 < bytes.len() && bytes[i + 1] == quote {
                        field.push(quote);
                        i += 2;
                    } else {
                        state = CsvState::AfterQuote;
                        i += 1;
                    }
                } else {
                    field.push(b);
                    i += 1;
                }
            }
            CsvState::AfterQuote => {
                if b == delimiter {
                    row.push(String::from_utf8(std::mem::take(&mut field)).unwrap());
                    state = CsvState::FieldStart;
                    i += 1;
                } else if b == b'\n' {
                    row.push(String::from_utf8(std::mem::take(&mut field)).unwrap());
                    records.push(std::mem::take(&mut row));
                    record += 1;
                    state = CsvState::FieldStart;
                    i += 1;
                } else if b == b'\r' {
                    row.push(String::from_utf8(std::mem::take(&mut field)).unwrap());
                    records.push(std::mem::take(&mut row));
                    record += 1;
                    state = CsvState::FieldStart;
                    i += 2;
                } else {
                    return Err(CsvError::TrailingByte { record, byte: i });
                }
            }
        }
    }

    if state == CsvState::Quoted {
        return Err(CsvError::UnclosedQuote {
            record,
            byte: quote_open,
        });
    }

    if state != CsvState::FieldStart || !row.is_empty() {
        row.push(String::from_utf8(field).unwrap());
        records.push(row);
    }
    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_csv_with(input, b',', b'"')
}

fn build_table(records: Vec<Vec<String>>, config: Config) -> Result<Table, CsvError> {
    let (header, rows, header_record) = if config.headers {
        if records.is_empty() {
            (None, Vec::new(), None)
        } else {
            let mut iter = records.into_iter();
            (iter.next(), iter.collect::<Vec<_>>(), Some(0usize))
        }
    } else {
        (None, records, None)
    };

    if let (Some(header), DuplicatePolicy::Reject) = (&header, config.duplicates) {
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

    if let (Some(header), Some(first_data_record)) = (&header, header_record) {
        for (index, row) in rows.iter().enumerate() {
            if row.len() != header.len() {
                return Err(CsvError::HeaderWidth {
                    record: first_data_record + index + 1,
                    expected: header.len(),
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
        let result = parse_csv_with(&self.input, self.config.delimiter, self.config.quote)
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

fn is_integer_syntax(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let start = usize::from(bytes[0] == b'-');
    if start == bytes.len() {
        return false;
    }
    if bytes[start] == b'0' {
        return start + 1 == bytes.len() && start == 0;
    }
    if !(b'1'..=b'9').contains(&bytes[start]) {
        return false;
    }
    bytes[start + 1..].iter().all(u8::is_ascii_digit)
}

fn parse_integer(s: &str) -> Result<i64, CellErrorKind> {
    if !is_integer_syntax(s) {
        return Err(CellErrorKind::InvalidInteger);
    }
    s.parse::<i64>().map_err(|_| CellErrorKind::IntegerOverflow)
}

fn is_decimal_syntax(s: &str) -> bool {
    let bytes = s.as_bytes();
    let start = usize::from(bytes.first() == Some(&b'-'));
    if start >= bytes.len() {
        return false;
    }
    let dot = match bytes[start..].iter().position(|&b| b == b'.') {
        Some(position) => start + position,
        None => return false,
    };
    if dot + 3 != bytes.len() {
        return false;
    }
    if !bytes[dot + 1..].iter().all(u8::is_ascii_digit) {
        return false;
    }
    let integral = &bytes[start..dot];
    if integral.is_empty() {
        return false;
    }
    if integral == b"0" {
        return true;
    }
    (b'1'..=b'9').contains(&integral[0])
        && integral[1..].iter().all(u8::is_ascii_digit)
}

fn parse_decimal(s: &str) -> Result<i64, CellErrorKind> {
    if !is_decimal_syntax(s) {
        return Err(CellErrorKind::InvalidDecimal);
    }
    let negative = s.starts_with('-');
    let unsigned = if negative { &s[1..] } else { s };
    let mut digits = String::with_capacity(unsigned.len() - 1);
    for b in unsigned.bytes() {
        if b != b'.' {
            digits.push(b as char);
        }
    }
    let magnitude = digits
        .parse::<i64>()
        .map_err(|_| CellErrorKind::DecimalOverflow)?;
    if negative {
        magnitude
            .checked_neg()
            .ok_or(CellErrorKind::DecimalOverflow)
    } else {
        Ok(magnitude)
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
        let mut output = vec![None; columns.len()];
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
                    if policy != ErrorPolicy::Collect {
                        break;
                    }
                    continue;
                }
            };

            let cell = match row.get(column) {
                Some(cell) => cell,
                None => {
                    result.errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column: Some(column),
                        kind: CellErrorKind::MissingCell,
                    });
                    if policy != ErrorPolicy::Collect {
                        break;
                    }
                    continue;
                }
            };

            if spec.kind == ColumnType::Text {
                if spec.nullable && cell.is_empty() {
                    output[spec_index] = Some(Value::Null);
                } else {
                    output[spec_index] = Some(Value::Text(cell.clone()));
                }
                continue;
            }

            if cell.is_empty() {
                if spec.nullable {
                    output[spec_index] = Some(Value::Null);
                    continue;
                }
                result.errors.push(CellError {
                    row: row_index,
                    spec: spec_index,
                    column: Some(column),
                    kind: CellErrorKind::Empty,
                });
                if policy != ErrorPolicy::Collect {
                    break;
                }
                continue;
            }

            let converted = match spec.kind {
                ColumnType::Text => unreachable!(),
                ColumnType::Integer => parse_integer(cell).map(Value::Integer),
                ColumnType::Decimal => parse_decimal(cell).map(Value::Decimal),
                ColumnType::Boolean => match cell.as_str() {
                    "yes" | "1" => Ok(Value::Boolean(true)),
                    "no" | "0" => Ok(Value::Boolean(false)),
                    _ => Err(CellErrorKind::InvalidBoolean),
                },
            };
            match converted {
                Ok(value) => output[spec_index] = Some(value),
                Err(kind) => {
                    result.errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column: Some(column),
                        kind,
                    });
                    if policy != ErrorPolicy::Collect {
                        break;
                    }
                }
            }
        }
        result.rows.push(output);
        if policy == ErrorPolicy::Stop && !result.errors.is_empty() {
            break;
        }
    }

    result
}