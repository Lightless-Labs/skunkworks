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
            DuplicatePolicy::KeepLast => header.iter().rposition(|v| v == name),
            DuplicatePolicy::KeepFirst | DuplicatePolicy::Reject => {
                header.iter().position(|v| v == name)
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

fn parse_with(input: &str, delimiter: u8, quote: u8) -> Result<Vec<Vec<String>>, CsvError> {
    let bytes = input.as_bytes();
    let mut i = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) { 3 } else { 0 };
    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut after_quote = false;
    let mut field_started = false;
    let mut record_open = false;
    let mut quote_start = 0usize;
    let mut record_index = 0usize;

    while i < bytes.len() {
        let b = bytes[i];

        if b == 0 {
            return Err(CsvError::Nul { record: record_index, byte: i });
        }

        if !in_quotes && b == b'\r' && bytes.get(i + 1) != Some(&b'\n') {
            return Err(CsvError::BareCr { record: record_index, byte: i });
        }

        if in_quotes {
            if b == quote {
                if bytes.get(i + 1) == Some(&quote) {
                    field.push(quote as char);
                    i += 2;
                } else {
                    in_quotes = false;
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

        if after_quote {
            if b == delimiter {
                record.push(std::mem::take(&mut field));
                after_quote = false;
                field_started = false;
                record_open = true;
                i += 1;
            } else if b == b'\n' || b == b'\r' {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
                after_quote = false;
                field_started = false;
                record_open = false;
                record_index += 1;
                i += if b == b'\r' { 2 } else { 1 };
            } else {
                return Err(CsvError::TrailingByte { record: record_index, byte: i });
            }
            continue;
        }

        if b == delimiter {
            record.push(std::mem::take(&mut field));
            field_started = false;
            record_open = true;
            i += 1;
        } else if b == b'\n' || b == b'\r' {
            record.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut record));
            field_started = false;
            record_open = false;
            record_index += 1;
            i += if b == b'\r' { 2 } else { 1 };
        } else if b == quote {
            if field_started {
                return Err(CsvError::UnexpectedQuote { record: record_index, byte: i });
            }
            in_quotes = true;
            quote_start = i;
            record_open = true;
            i += 1;
        } else {
            let ch = input[i..].chars().next().unwrap();
            field.push(ch);
            field_started = true;
            record_open = true;
            i += ch.len_utf8();
        }
    }

    if in_quotes {
        return Err(CsvError::UnclosedQuote { record: record_index, byte: quote_start });
    }

    if record_open || !record.is_empty() || !field.is_empty() || after_quote || field_started {
        record.push(field);
        records.push(record);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with(input, b',', b'"')
}

fn read_validated(input: &str, config: Config) -> Result<Table, CsvError> {
    let mut records = parse_with(input, config.delimiter, config.quote)?;

    let header = if config.headers && !records.is_empty() {
        Some(records.remove(0))
    } else {
        None
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

    if let Some(header) = &header {
        for (offset, row) in records.iter().enumerate() {
            if row.len() != header.len() {
                return Err(CsvError::HeaderWidth {
                    record: offset + 1,
                    expected: header.len(),
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
            self.result = Some(read_validated(&self.input, self.config));
        }
        self.result.as_ref().unwrap().clone()
    }

    pub fn reset(&mut self) {
        self.input.clear();
        self.result = None;
    }
}

pub fn read_csv(input: &str, config: Config) -> Result<Table, CsvError> {
    validate_config(config)?;
    read_validated(input, config)
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
    let start = usize::from(b[0] == b'-');
    if start == b.len() {
        return false;
    }
    if b[start] == b'0' {
        return start == 0 && b.len() == 1;
    }
    b[start].is_ascii_digit() && b[start] != b'0'
        && b[start..].iter().all(u8::is_ascii_digit)
}

fn parse_integer(s: &str) -> Result<i64, CellErrorKind> {
    if !integer_syntax(s) {
        return Err(CellErrorKind::InvalidInteger);
    }
    s.parse::<i64>().map_err(|_| CellErrorKind::IntegerOverflow)
}

fn decimal_syntax(s: &str) -> bool {
    let b = s.as_bytes();
    let start = usize::from(b.starts_with(b"-"));
    if b.len() < start + 4 {
        return false;
    }
    let dot = b.len() - 3;
    if b[dot] != b'.' || !b[dot + 1..].iter().all(u8::is_ascii_digit) {
        return false;
    }
    let whole = &b[start..dot];
    if whole.is_empty() {
        return false;
    }
    let valid_whole = if whole == b"0" {
        true
    } else {
        whole[0].is_ascii_digit()
            && whole[0] != b'0'
            && whole.iter().all(u8::is_ascii_digit)
    };
    if !valid_whole {
        return false;
    }
    !(start == 1 && whole == b"0" && b[dot + 1..] == *b"00")
}

fn parse_decimal(s: &str) -> Result<i64, CellErrorKind> {
    if !decimal_syntax(s) {
        return Err(CellErrorKind::InvalidDecimal);
    }
    let negative = s.starts_with('-');
    let unsigned = if negative { &s[1..] } else { s };
    let dot = unsigned.len() - 3;
    let whole = unsigned[..dot].parse::<i128>().unwrap();
    let frac = unsigned[dot + 1..].parse::<i128>().unwrap();
    let magnitude = whole * 100 + frac;
    let signed = if negative { -magnitude } else { magnitude };
    i64::try_from(signed).map_err(|_| CellErrorKind::DecimalOverflow)
}

fn evaluate_cell(
    table: &Table,
    row: &[String],
    row_index: usize,
    spec_index: usize,
    spec: &ColumnSpec,
) -> (Option<Value>, Option<CellError>) {
    let column = match &spec.column {
        ColumnRef::Index(i) => Some(*i),
        ColumnRef::Name(name) => table.column_index(name),
    };
    let Some(column) = column else {
        return (
            None,
            Some(CellError {
                row: row_index,
                spec: spec_index,
                column: None,
                kind: CellErrorKind::UnknownColumn,
            }),
        );
    };
    let Some(cell) = row.get(column) else {
        return (
            None,
            Some(CellError {
                row: row_index,
                spec: spec_index,
                column: Some(column),
                kind: CellErrorKind::MissingCell,
            }),
        );
    };

    let error = |kind| {
        Some(CellError {
            row: row_index,
            spec: spec_index,
            column: Some(column),
            kind,
        })
    };

    if cell.is_empty() {
        if spec.nullable {
            return (Some(Value::Null), None);
        }
        if spec.kind != ColumnType::Text {
            return (None, error(CellErrorKind::Empty));
        }
    }

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
        Ok(value) => (Some(value), None),
        Err(kind) => (None, error(kind)),
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

    'rows: for (row_index, row) in table.rows.iter().enumerate() {
        let mut output = Vec::with_capacity(columns.len());
        let mut had_error = false;
        for (spec_index, spec) in columns.iter().enumerate() {
            let (value, error) =
                evaluate_cell(table, row, row_index, spec_index, spec);
            output.push(value);
            if let Some(error) = error {
                result.errors.push(error);
                had_error = true;
                match policy {
                    ErrorPolicy::Collect => {}
                    ErrorPolicy::FirstPerRow => {
                        output.resize(columns.len(), None);
                        break;
                    }
                    ErrorPolicy::Stop => {
                        output.resize(columns.len(), None);
                        result.rows.push(output);
                        break 'rows;
                    }
                }
            }
        }
        result.rows.push(output);
        if policy == ErrorPolicy::Stop && had_error {
            break;
        }
    }

    result
}