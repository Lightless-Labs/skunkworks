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
    result: Option<Result<Table, CsvError>>,
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

fn parse_with(input: &str, delimiter: u8, quote: u8) -> Result<Vec<Vec<String>>, CsvError> {
    let bytes = input.as_bytes();
    let start = if input.starts_with('\u{feff}') { 3 } else { 0 };
    if start == bytes.len() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut i = start;
    let mut record_index = 0usize;
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
            return Err(CsvError::Nul {
                record: record_index,
                byte: i,
            });
        }

        let outside = !matches!(state, State::Quoted { .. });
        if outside && b == b'\r' && (i + 1 >= bytes.len() || bytes[i + 1] != b'\n') {
            return Err(CsvError::BareCr {
                record: record_index,
                byte: i,
            });
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
                } else if b == b'\n' || b == b'\r' {
                    fields.push(String::new());
                    records.push(std::mem::take(&mut fields));
                    record_index += 1;
                    record_started = false;
                    i += if b == b'\r' { 2 } else { 1 };
                } else {
                    field.push(b as char);
                    // The input is UTF-8; append this byte through the general
                    // unquoted path below when it begins a multibyte character.
                    if b >= 0x80 {
                        let ch = input[i..].chars().next().unwrap();
                        field.pop();
                        field.push(ch);
                        i += ch.len_utf8();
                    } else {
                        i += 1;
                    }
                    state = State::Unquoted;
                    record_started = true;
                }
            }
            State::Unquoted => {
                if b == quote {
                    return Err(CsvError::UnexpectedQuote {
                        record: record_index,
                        byte: i,
                    });
                } else if b == delimiter {
                    fields.push(std::mem::take(&mut field));
                    state = State::FieldStart;
                    record_started = true;
                    i += 1;
                } else if b == b'\n' || b == b'\r' {
                    fields.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut fields));
                    record_index += 1;
                    record_started = false;
                    state = State::FieldStart;
                    i += if b == b'\r' { 2 } else { 1 };
                } else if b >= 0x80 {
                    let ch = input[i..].chars().next().unwrap();
                    field.push(ch);
                    i += ch.len_utf8();
                } else {
                    field.push(b as char);
                    i += 1;
                }
            }
            State::Quoted { opening } => {
                if b == quote {
                    if i + 1 < bytes.len() && bytes[i + 1] == quote {
                        field.push(quote as char);
                        i += 2;
                    } else {
                        state = State::AfterQuote;
                        i += 1;
                    }
                } else if b >= 0x80 {
                    let ch = input[i..].chars().next().unwrap();
                    field.push(ch);
                    i += ch.len_utf8();
                } else {
                    field.push(b as char);
                    i += 1;
                }
                let _ = opening;
            }
            State::AfterQuote => {
                if b == delimiter {
                    fields.push(std::mem::take(&mut field));
                    state = State::FieldStart;
                    record_started = true;
                    i += 1;
                } else if b == b'\n' || b == b'\r' {
                    fields.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut fields));
                    record_index += 1;
                    record_started = false;
                    state = State::FieldStart;
                    i += if b == b'\r' { 2 } else { 1 };
                } else {
                    return Err(CsvError::TrailingByte {
                        record: record_index,
                        byte: i,
                    });
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

    if record_started || !fields.is_empty() || !field.is_empty() {
        fields.push(field);
        records.push(fields);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with(input, b',', b'"')
}

fn build_table(records: Vec<Vec<String>>, config: Config) -> Result<Table, CsvError> {
    let mut records = records.into_iter();
    let header = if config.headers {
        records.next()
    } else {
        None
    };

    if let Some(ref names) = header {
        if config.duplicates == DuplicatePolicy::Reject {
            let mut seen: Vec<(&str, usize)> = Vec::new();
            for (second, name) in names.iter().enumerate() {
                if let Some((_, first)) = seen.iter().find(|(existing, _)| *existing == name) {
                    return Err(CsvError::DuplicateHeader {
                        name: name.clone(),
                        first: *first,
                        second,
                    });
                }
                seen.push((name.as_str(), second));
            }
        }
    }

    let rows: Vec<Vec<String>> = records.collect();
    if let Some(ref names) = header {
        for (offset, row) in rows.iter().enumerate() {
            if row.len() != names.len() {
                return Err(CsvError::HeaderWidth {
                    record: offset + 1,
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
        let result = parse_with(&self.input, self.config.delimiter, self.config.quote)
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
    validate_config(config)?;
    let records = parse_with(input, config.delimiter, config.quote)?;
    build_table(records, config)
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

fn decimal_syntax(s: &str) -> bool {
    let b = s.as_bytes();
    let body = if b.first() == Some(&b'-') {
        &b[1..]
    } else {
        b
    };
    let Some(dot) = body.iter().position(|&byte| byte == b'.') else {
        return false;
    };
    if body[dot + 1..].len() != 2
        || !body[dot + 1..].iter().all(u8::is_ascii_digit)
    {
        return false;
    }
    let integral = &body[..dot];
    if integral.is_empty() {
        return false;
    }
    let valid_integral = integral == b"0"
        || (integral[0].is_ascii_digit()
            && integral[0] != b'0'
            && integral.iter().all(u8::is_ascii_digit));
    if !valid_integral {
        return false;
    }
    !(b.first() == Some(&b'-') && integral == b"0" && body[dot + 1..] == *b"00")
}

fn parse_integer(s: &str) -> Result<i64, CellErrorKind> {
    if !integer_syntax(s) {
        return Err(CellErrorKind::InvalidInteger);
    }
    s.parse::<i64>()
        .map_err(|_| CellErrorKind::IntegerOverflow)
}

fn parse_decimal(s: &str) -> Result<i64, CellErrorKind> {
    if !decimal_syntax(s) {
        return Err(CellErrorKind::InvalidDecimal);
    }
    let negative = s.starts_with('-');
    let unsigned = if negative { &s[1..] } else { s };
    let dot = unsigned.find('.').unwrap();
    let whole = unsigned[..dot]
        .parse::<i128>()
        .map_err(|_| CellErrorKind::DecimalOverflow)?;
    let fraction = unsigned[dot + 1..].parse::<i128>().unwrap();
    let magnitude = whole * 100 + fraction;
    let signed = if negative { -magnitude } else { magnitude };
    i64::try_from(signed).map_err(|_| CellErrorKind::DecimalOverflow)
}

pub fn extract_columns(
    table: &Table,
    columns: &[ColumnSpec],
    policy: ErrorPolicy,
) -> TypedResult {
    let mut output = TypedResult {
        rows: Vec::new(),
        errors: Vec::new(),
    };

    'rows: for (row_index, row) in table.rows.iter().enumerate() {
        let mut out_row = Vec::with_capacity(columns.len());
        for (spec_index, spec) in columns.iter().enumerate() {
            let column_index = match &spec.column {
                ColumnRef::Index(index) => Some(*index),
                ColumnRef::Name(name) => table.column_index(name),
            };

            let Some(column_index) = column_index else {
                output.errors.push(CellError {
                    row: row_index,
                    spec: spec_index,
                    column: None,
                    kind: CellErrorKind::UnknownColumn,
                });
                out_row.push(None);
                match policy {
                    ErrorPolicy::Collect => continue,
                    ErrorPolicy::FirstPerRow => {
                        out_row.resize(columns.len(), None);
                        break;
                    }
                    ErrorPolicy::Stop => {
                        out_row.resize(columns.len(), None);
                        output.rows.push(out_row);
                        break 'rows;
                    }
                }
            };

            let Some(cell) = row.get(column_index) else {
                output.errors.push(CellError {
                    row: row_index,
                    spec: spec_index,
                    column: Some(column_index),
                    kind: CellErrorKind::MissingCell,
                });
                out_row.push(None);
                match policy {
                    ErrorPolicy::Collect => continue,
                    ErrorPolicy::FirstPerRow => {
                        out_row.resize(columns.len(), None);
                        break;
                    }
                    ErrorPolicy::Stop => {
                        out_row.resize(columns.len(), None);
                        output.rows.push(out_row);
                        break 'rows;
                    }
                }
            };

            if cell.is_empty() {
                if spec.nullable {
                    out_row.push(Some(Value::Null));
                    continue;
                } else if spec.kind != ColumnType::Text {
                    output.errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column: Some(column_index),
                        kind: CellErrorKind::Empty,
                    });
                    out_row.push(None);
                    match policy {
                        ErrorPolicy::Collect => continue,
                        ErrorPolicy::FirstPerRow => {
                            out_row.resize(columns.len(), None);
                            break;
                        }
                        ErrorPolicy::Stop => {
                            out_row.resize(columns.len(), None);
                            output.rows.push(out_row);
                            break 'rows;
                        }
                    }
                }
            }

            let value = match spec.kind {
                ColumnType::Text => Ok(Value::Text(cell.clone())),
                ColumnType::Integer => parse_integer(cell).map(Value::Integer),
                ColumnType::Decimal => parse_decimal(cell).map(Value::Decimal),
                ColumnType::Boolean => match cell.as_str() {
                    "yes" | "1" => Ok(Value::Boolean(true)),
                    "no" | "0" => Ok(Value::Boolean(false)),
                    _ => Err(CellErrorKind::InvalidBoolean),
                },
            };

            match value {
                Ok(value) => out_row.push(Some(value)),
                Err(kind) => {
                    output.errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column: Some(column_index),
                        kind,
                    });
                    out_row.push(None);
                    match policy {
                        ErrorPolicy::Collect => {}
                        ErrorPolicy::FirstPerRow => {
                            out_row.resize(columns.len(), None);
                            break;
                        }
                        ErrorPolicy::Stop => {
                            out_row.resize(columns.len(), None);
                            output.rows.push(out_row);
                            break 'rows;
                        }
                    }
                }
            }
        }
        output.rows.push(out_row);
    }

    output
}