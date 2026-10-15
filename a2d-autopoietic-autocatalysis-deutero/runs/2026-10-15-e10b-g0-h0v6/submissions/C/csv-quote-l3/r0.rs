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
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut i = start;
    let mut record = 0usize;
    let mut record_started = false;

    #[derive(Clone, Copy)]
    enum State {
        FieldStart,
        Unquoted,
        Quoted { opening: usize },
        AfterQuote,
    }

    let mut state = State::FieldStart;

    while i < bytes.len() {
        let b = bytes[i];

        if b == 0 {
            return Err(CsvError::Nul { record, byte: i });
        }

        if !matches!(state, State::Quoted { .. }) && b == b'\r'
            && bytes.get(i + 1) != Some(&b'\n')
        {
            return Err(CsvError::BareCr { record, byte: i });
        }

        match state {
            State::FieldStart => {
                if b == quote {
                    state = State::Quoted { opening: i };
                    record_started = true;
                    i += 1;
                } else if b == delimiter {
                    fields.push(String::new());
                    record_started = true;
                    i += 1;
                } else if b == b'\n' {
                    fields.push(String::new());
                    records.push(std::mem::take(&mut fields));
                    record += 1;
                    record_started = false;
                    i += 1;
                } else if b == b'\r' {
                    fields.push(String::new());
                    records.push(std::mem::take(&mut fields));
                    record += 1;
                    record_started = false;
                    i += 2;
                } else {
                    let ch = input[i..].chars().next().unwrap();
                    field.push(ch);
                    state = State::Unquoted;
                    record_started = true;
                    i += ch.len_utf8();
                }
            }
            State::Unquoted => {
                if b == quote {
                    return Err(CsvError::UnexpectedQuote { record, byte: i });
                } else if b == delimiter {
                    fields.push(std::mem::take(&mut field));
                    state = State::FieldStart;
                    record_started = true;
                    i += 1;
                } else if b == b'\n' {
                    fields.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut fields));
                    record += 1;
                    state = State::FieldStart;
                    record_started = false;
                    i += 1;
                } else if b == b'\r' {
                    fields.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut fields));
                    record += 1;
                    state = State::FieldStart;
                    record_started = false;
                    i += 2;
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
                    if i == bytes.len() {
                        let _ = opening;
                    }
                }
            }
            State::AfterQuote => {
                if b == delimiter {
                    fields.push(std::mem::take(&mut field));
                    state = State::FieldStart;
                    record_started = true;
                    i += 1;
                } else if b == b'\n' {
                    fields.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut fields));
                    record += 1;
                    state = State::FieldStart;
                    record_started = false;
                    i += 1;
                } else if b == b'\r' {
                    fields.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut fields));
                    record += 1;
                    state = State::FieldStart;
                    record_started = false;
                    i += 2;
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

    if record_started || !fields.is_empty() || !field.is_empty() {
        fields.push(field);
        records.push(fields);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with_dialect(input, b',', b'"')
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
        let result = parse_table(&self.input, self.config);
        self.result = Some(result.clone());
        result
    }

    pub fn reset(&mut self) {
        self.input.clear();
        self.result = None;
    }
}

fn parse_table(input: &str, config: Config) -> Result<Table, CsvError> {
    validate_config(config)?;
    let mut records = parse_with_dialect(input, config.delimiter, config.quote)?;

    let header = if config.headers && !records.is_empty() {
        Some(records.remove(0))
    } else {
        None
    };

    if let Some(ref names) = header {
        if config.duplicates == DuplicatePolicy::Reject {
            for second in 0..names.len() {
                if let Some(first) = names[..second].iter().position(|n| n == &names[second]) {
                    return Err(CsvError::DuplicateHeader {
                        name: names[second].clone(),
                        first,
                        second,
                    });
                }
            }
        }

        for (row_index, row) in records.iter().enumerate() {
            if row.len() != names.len() {
                return Err(CsvError::HeaderWidth {
                    record: row_index + 1,
                    expected: names.len(),
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
    (b'1'..=b'9').contains(&digits[0])
        && digits[1..].iter().all(|c| c.is_ascii_digit())
}

fn decimal_syntax(s: &str) -> bool {
    let b = s.as_bytes();
    if b.is_empty() {
        return false;
    }
    let neg = b[0] == b'-';
    let body = if neg { &b[1..] } else { b };
    if body.len() < 4 {
        return false;
    }
    let dot = body.len() - 3;
    if body[dot] != b'.' || !body[dot + 1..].iter().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let integral = &body[..dot];
    if integral == b"0" {
        !neg || body[dot + 1..] != *b"00"
    } else {
        !integral.is_empty()
            && (b'1'..=b'9').contains(&integral[0])
            && integral[1..].iter().all(|c| c.is_ascii_digit())
    }
}

fn parse_integer_value(s: &str) -> Result<i64, CellErrorKind> {
    if !integer_syntax(s) {
        return Err(CellErrorKind::InvalidInteger);
    }
    s.parse::<i64>().map_err(|_| CellErrorKind::IntegerOverflow)
}

fn parse_decimal_value(s: &str) -> Result<i64, CellErrorKind> {
    if !decimal_syntax(s) {
        return Err(CellErrorKind::InvalidDecimal);
    }
    let negative = s.starts_with('-');
    let body = if negative { &s[1..] } else { s };
    let dot = body.len() - 3;
    let integral: i128 = body[..dot]
        .parse()
        .map_err(|_| CellErrorKind::DecimalOverflow)?;
    let fractional: i128 = body[dot + 1..]
        .parse()
        .map_err(|_| CellErrorKind::DecimalOverflow)?;
    let magnitude = integral * 100 + fractional;
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
        let mut output = Vec::with_capacity(columns.len());
        let mut failed = false;

        for (spec_index, spec) in columns.iter().enumerate() {
            if failed {
                output.push(None);
                continue;
            }

            let column_index = match &spec.column {
                ColumnRef::Index(index) => Some(*index),
                ColumnRef::Name(name) => table.column_index(name),
            };

            let Some(column_index) = column_index else {
                output.push(None);
                result.errors.push(CellError {
                    row: row_index,
                    spec: spec_index,
                    column: None,
                    kind: CellErrorKind::UnknownColumn,
                });
                failed = policy != ErrorPolicy::Collect;
                continue;
            };

            let Some(cell) = row.get(column_index) else {
                output.push(None);
                result.errors.push(CellError {
                    row: row_index,
                    spec: spec_index,
                    column: Some(column_index),
                    kind: CellErrorKind::MissingCell,
                });
                failed = policy != ErrorPolicy::Collect;
                continue;
            };

            if cell.is_empty() {
                if spec.nullable {
                    output.push(Some(Value::Null));
                    continue;
                }
                if spec.kind != ColumnType::Text {
                    output.push(None);
                    result.errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column: Some(column_index),
                        kind: CellErrorKind::Empty,
                    });
                    failed = policy != ErrorPolicy::Collect;
                    continue;
                }
            }

            let parsed = match spec.kind {
                ColumnType::Text => Ok(Value::Text(cell.clone())),
                ColumnType::Integer => parse_integer_value(cell).map(Value::Integer),
                ColumnType::Decimal => parse_decimal_value(cell).map(Value::Decimal),
                ColumnType::Boolean => match cell.as_str() {
                    "yes" | "1" => Ok(Value::Boolean(true)),
                    "no" | "0" => Ok(Value::Boolean(false)),
                    _ => Err(CellErrorKind::InvalidBoolean),
                },
            };

            match parsed {
                Ok(value) => output.push(Some(value)),
                Err(kind) => {
                    output.push(None);
                    result.errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column: Some(column_index),
                        kind,
                    });
                    failed = policy != ErrorPolicy::Collect;
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

fn main() {}