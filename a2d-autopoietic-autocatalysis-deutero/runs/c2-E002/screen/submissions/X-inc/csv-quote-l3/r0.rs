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

#[derive(Debug, Clone, PartialEq, Eq)]
enum CachedResult {
    Ok(Table),
    Err(CsvError),
}

pub struct StreamReader {
    config: Config,
    input: String,
    cached: Option<CachedResult>,
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

    #[derive(Clone, Copy)]
    enum State {
        FieldStart,
        Unquoted,
        Quoted { opening: usize },
        AfterQuote,
    }

    let mut records = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut state = State::FieldStart;
    let mut i = start;
    let mut record = 0usize;

    while i < bytes.len() {
        let b = bytes[i];

        if b == 0 {
            return Err(CsvError::Nul { record, byte: i });
        }

        if !matches!(state, State::Quoted { .. }) && b == b'\r' {
            if bytes.get(i + 1) != Some(&b'\n') {
                return Err(CsvError::BareCr { record, byte: i });
            }
        }

        match state {
            State::FieldStart => {
                if b == delimiter {
                    row.push(String::new());
                    i += 1;
                } else if b == quote {
                    state = State::Quoted { opening: i };
                    i += 1;
                } else if b == b'\n' || b == b'\r' {
                    row.push(String::new());
                    records.push(std::mem::take(&mut row));
                    record += 1;
                    i += if b == b'\r' { 2 } else { 1 };
                } else {
                    let ch = input[i..].chars().next().unwrap();
                    field.push(ch);
                    state = State::Unquoted;
                    i += ch.len_utf8();
                }
            }
            State::Unquoted => {
                if b == delimiter {
                    row.push(std::mem::take(&mut field));
                    state = State::FieldStart;
                    i += 1;
                } else if b == b'\n' || b == b'\r' {
                    row.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut row));
                    record += 1;
                    state = State::FieldStart;
                    i += if b == b'\r' { 2 } else { 1 };
                } else if b == quote {
                    return Err(CsvError::UnexpectedQuote { record, byte: i });
                } else {
                    let ch = input[i..].chars().next().unwrap();
                    field.push(ch);
                    i += ch.len_utf8();
                }
            }
            State::Quoted { opening } => {
                if b == quote {
                    if bytes.get(i + 1) == Some(&quote) {
                        field.push(quote as char);
                        i += 2;
                    } else {
                        state = State::AfterQuote;
                        i += 1;
                    }
                } else {
                    let ch = input[i..].chars().next().unwrap();
                    field.push(ch);
                    i += ch.len_utf8();
                }
                if i == bytes.len() && matches!(state, State::Quoted { .. }) {
                    return Err(CsvError::UnclosedQuote {
                        record,
                        byte: opening,
                    });
                }
            }
            State::AfterQuote => {
                if b == delimiter {
                    row.push(std::mem::take(&mut field));
                    state = State::FieldStart;
                    i += 1;
                } else if b == b'\n' || b == b'\r' {
                    row.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut row));
                    record += 1;
                    state = State::FieldStart;
                    i += if b == b'\r' { 2 } else { 1 };
                } else {
                    return Err(CsvError::TrailingByte { record, byte: i });
                }
            }
        }
    }

    if let State::Quoted { opening } = state {
        return Err(CsvError::UnclosedQuote {
            record,
            byte: opening,
        });
    }

    if !matches!(state, State::FieldStart) || !row.is_empty() {
        row.push(field);
        records.push(row);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with_dialect(input, b',', b'"')
}

fn build_table(input: &str, config: Config) -> Result<Table, CsvError> {
    validate_config(config)?;
    let records = parse_with_dialect(input, config.delimiter, config.quote)?;

    let (header, rows) = if config.headers {
        if records.is_empty() {
            (None, Vec::new())
        } else {
            let mut records = records.into_iter();
            (records.next(), records.collect::<Vec<_>>())
        }
    } else {
        (None, records)
    };

    if let Some(ref header_values) = header {
        if config.duplicates == DuplicatePolicy::Reject {
            let mut seen: Vec<(&str, usize)> = Vec::new();
            for (second, name) in header_values.iter().enumerate() {
                if let Some((_, first)) = seen.iter().find(|(seen_name, _)| *seen_name == name) {
                    return Err(CsvError::DuplicateHeader {
                        name: name.clone(),
                        first: *first,
                        second,
                    });
                }
                seen.push((name.as_str(), second));
            }
        }

        for (row_index, row) in rows.iter().enumerate() {
            if row.len() != header_values.len() {
                return Err(CsvError::HeaderWidth {
                    record: row_index + 1,
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
        if let Some(cached) = &self.cached {
            return match cached {
                CachedResult::Ok(table) => Ok(table.clone()),
                CachedResult::Err(error) => Err(error.clone()),
            };
        }

        let result = build_table(&self.input, self.config);
        self.cached = Some(match &result {
            Ok(table) => CachedResult::Ok(table.clone()),
            Err(error) => CachedResult::Err(error.clone()),
        });
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
    let bytes = s.as_bytes();
    let mut i = 0;
    if bytes.first() == Some(&b'-') {
        i = 1;
    }
    if i >= bytes.len() {
        return false;
    }
    if bytes[i] == b'0' {
        return i + 1 == bytes.len();
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
    let mut i = 0;
    if bytes.first() == Some(&b'-') {
        i = 1;
    }
    if i >= bytes.len() {
        return false;
    }

    let integral_start = i;
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
    if bytes[0] == b'-' && bytes[integral_start..i].iter().all(|&b| b == b'0')
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
    let unsigned = if negative { &s[1..] } else { s };
    let digits: String = unsigned.chars().filter(|&c| c != '.').collect();
    let magnitude = digits
        .parse::<i128>()
        .map_err(|_| CellErrorKind::DecimalOverflow)?;
    let signed = if negative { -magnitude } else { magnitude };
    i64::try_from(signed).map_err(|_| CellErrorKind::DecimalOverflow)
}

fn evaluate_cell(
    table: &Table,
    row: &[String],
    row_index: usize,
    spec: &ColumnSpec,
    spec_index: usize,
) -> Result<Value, CellError> {
    let column = match &spec.column {
        ColumnRef::Index(index) => Some(*index),
        ColumnRef::Name(name) => table.column_index(name),
    };

    let Some(column) = column else {
        return Err(CellError {
            row: row_index,
            spec: spec_index,
            column: None,
            kind: CellErrorKind::UnknownColumn,
        });
    };

    let Some(cell) = row.get(column) else {
        return Err(CellError {
            row: row_index,
            spec: spec_index,
            column: Some(column),
            kind: CellErrorKind::MissingCell,
        });
    };

    if spec.nullable && cell.is_empty() {
        return Ok(Value::Null);
    }

    let result = if cell.is_empty() && spec.kind != ColumnType::Text {
        Err(CellErrorKind::Empty)
    } else {
        match spec.kind {
            ColumnType::Text => Ok(Value::Text(cell.clone())),
            ColumnType::Integer => parse_integer(cell).map(Value::Integer),
            ColumnType::Decimal => parse_decimal(cell).map(Value::Decimal),
            ColumnType::Boolean => match cell.as_str() {
                "yes" | "1" => Ok(Value::Boolean(true)),
                "no" | "0" => Ok(Value::Boolean(false)),
                _ => Err(CellErrorKind::InvalidBoolean),
            },
        }
    };

    result.map_err(|kind| CellError {
        row: row_index,
        spec: spec_index,
        column: Some(column),
        kind,
    })
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

    'rows: for (row_index, row) in table.rows.iter().enumerate() {
        let mut output = vec![None; columns.len()];
        for (spec_index, spec) in columns.iter().enumerate() {
            match evaluate_cell(table, row, row_index, spec, spec_index) {
                Ok(value) => output[spec_index] = Some(value),
                Err(error) => {
                    result.errors.push(error);
                    match policy {
                        ErrorPolicy::Collect => {}
                        ErrorPolicy::FirstPerRow => break,
                        ErrorPolicy::Stop => {
                            result.rows.push(output);
                            break 'rows;
                        }
                    }
                }
            }
        }
        result.rows.push(output);
    }

    result
}