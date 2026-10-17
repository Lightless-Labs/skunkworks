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

fn parse_with_dialect(
    input: &str,
    delimiter: u8,
    quote: u8,
) -> Result<Vec<Vec<String>>, CsvError> {
    let bytes = input.as_bytes();
    let mut pos = if input.starts_with('\u{feff}') { 3 } else { 0 };
    let mut records = Vec::new();
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut record_started = false;
    let mut in_quotes = false;
    let mut after_quote = false;
    let mut quote_open = 0usize;

    while pos < bytes.len() {
        let b = bytes[pos];

        if b == 0 {
            return Err(CsvError::Nul {
                record: records.len(),
                byte: pos,
            });
        }

        if !in_quotes && b == b'\r' && bytes.get(pos + 1) != Some(&b'\n') {
            return Err(CsvError::BareCr {
                record: records.len(),
                byte: pos,
            });
        }

        if in_quotes {
            if b == quote {
                if bytes.get(pos + 1) == Some(&quote) {
                    field.push(quote as char);
                    pos += 2;
                } else {
                    in_quotes = false;
                    after_quote = true;
                    pos += 1;
                }
            } else {
                let ch = input[pos..].chars().next().unwrap();
                field.push(ch);
                pos += ch.len_utf8();
            }
            continue;
        }

        if after_quote {
            if b == delimiter {
                fields.push(std::mem::take(&mut field));
                record_started = true;
                after_quote = false;
                pos += 1;
            } else if b == b'\n' || b == b'\r' {
                fields.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut fields));
                record_started = false;
                after_quote = false;
                pos += if b == b'\r' { 2 } else { 1 };
            } else {
                return Err(CsvError::TrailingByte {
                    record: records.len(),
                    byte: pos,
                });
            }
            continue;
        }

        if b == delimiter {
            fields.push(std::mem::take(&mut field));
            record_started = true;
            pos += 1;
        } else if b == b'\n' || b == b'\r' {
            fields.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut fields));
            record_started = false;
            pos += if b == b'\r' { 2 } else { 1 };
        } else if b == quote {
            if field.is_empty() {
                in_quotes = true;
                record_started = true;
                quote_open = pos;
                pos += 1;
            } else {
                return Err(CsvError::UnexpectedQuote {
                    record: records.len(),
                    byte: pos,
                });
            }
        } else {
            let ch = input[pos..].chars().next().unwrap();
            field.push(ch);
            record_started = true;
            pos += ch.len_utf8();
        }
    }

    if in_quotes {
        return Err(CsvError::UnclosedQuote {
            record: records.len(),
            byte: quote_open,
        });
    }

    if record_started || after_quote || !fields.is_empty() || !field.is_empty() {
        fields.push(field);
        records.push(fields);
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
        let parsed_header = rows.remove(0);

        if config.duplicates == DuplicatePolicy::Reject {
            for second in 0..parsed_header.len() {
                if let Some(first) = parsed_header[..second]
                    .iter()
                    .position(|value| value == &parsed_header[second])
                {
                    return Err(CsvError::DuplicateHeader {
                        name: parsed_header[second].clone(),
                        first,
                        second,
                    });
                }
            }
        }

        for (index, row) in rows.iter().enumerate() {
            if row.len() != parsed_header.len() {
                return Err(CsvError::HeaderWidth {
                    record: index + 1,
                    expected: parsed_header.len(),
                    actual: row.len(),
                });
            }
        }

        header = Some(parsed_header);
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

        let result = parse_with_dialect(&self.input, self.config.delimiter, self.config.quote)
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

fn valid_integer_syntax(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let start = usize::from(bytes[0] == b'-');
    if start == bytes.len() {
        return false;
    }
    if bytes[start] == b'0' {
        return start + 1 == bytes.len();
    }
    if !(b'1'..=b'9').contains(&bytes[start]) {
        return false;
    }
    bytes[start + 1..].iter().all(u8::is_ascii_digit)
}

fn parse_integer(s: &str) -> Result<i64, CellErrorKind> {
    if !valid_integer_syntax(s) {
        return Err(CellErrorKind::InvalidInteger);
    }
    s.parse::<i64>()
        .map_err(|_| CellErrorKind::IntegerOverflow)
}

fn valid_decimal_syntax(s: &str) -> bool {
    let bytes = s.as_bytes();
    let start = usize::from(bytes.first() == Some(&b'-'));
    if start >= bytes.len() {
        return false;
    }
    let dot = match bytes[start..].iter().position(|&b| b == b'.') {
        Some(offset) => start + offset,
        None => return false,
    };
    if dot + 3 != bytes.len() {
        return false;
    }
    if !bytes[dot + 1..].iter().all(u8::is_ascii_digit) {
        return false;
    }
    if dot == start {
        return false;
    }
    if bytes[start] == b'0' {
        if dot != start + 1 {
            return false;
        }
    } else {
        if !(b'1'..=b'9').contains(&bytes[start]) {
            return false;
        }
        if !bytes[start + 1..dot].iter().all(u8::is_ascii_digit) {
            return false;
        }
    }
    !(s.starts_with("-0.") && bytes[dot + 1..].iter().all(|&b| b == b'0'))
}

fn parse_decimal(s: &str) -> Result<i64, CellErrorKind> {
    if !valid_decimal_syntax(s) {
        return Err(CellErrorKind::InvalidDecimal);
    }
    let negative = s.starts_with('-');
    let unsigned = if negative { &s[1..] } else { s };
    let dot = unsigned.find('.').unwrap();
    let whole = unsigned[..dot]
        .parse::<i128>()
        .map_err(|_| CellErrorKind::DecimalOverflow)?;
    let fraction = unsigned[dot + 1..]
        .parse::<i128>()
        .map_err(|_| CellErrorKind::DecimalOverflow)?;
    let magnitude = whole
        .checked_mul(100)
        .and_then(|v| v.checked_add(fraction))
        .ok_or(CellErrorKind::DecimalOverflow)?;
    let signed = if negative { -magnitude } else { magnitude };
    i64::try_from(signed).map_err(|_| CellErrorKind::DecimalOverflow)
}

fn convert_cell(value: &str, kind: ColumnType) -> Result<Value, CellErrorKind> {
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

            let column_index = match &spec.column {
                ColumnRef::Index(index) => Some(*index),
                ColumnRef::Name(name) => table.column_index(name),
            };

            let error_kind;
            let value = if column_index.is_none() {
                error_kind = Some(CellErrorKind::UnknownColumn);
                None
            } else {
                let index = column_index.unwrap();
                if index >= row.len() {
                    error_kind = Some(CellErrorKind::MissingCell);
                    None
                } else {
                    let cell = &row[index];
                    if cell.is_empty() && spec.nullable {
                        error_kind = None;
                        Some(Value::Null)
                    } else if cell.is_empty() && spec.kind != ColumnType::Text {
                        error_kind = Some(CellErrorKind::Empty);
                        None
                    } else {
                        match convert_cell(cell, spec.kind) {
                            Ok(value) => {
                                error_kind = None;
                                Some(value)
                            }
                            Err(kind) => {
                                error_kind = Some(kind);
                                None
                            }
                        }
                    }
                }
            };

            if let Some(kind) = error_kind {
                result.errors.push(CellError {
                    row: row_index,
                    spec: spec_index,
                    column: column_index,
                    kind,
                });
                output.push(None);
                if policy != ErrorPolicy::Collect {
                    row_failed = true;
                }
                if policy == ErrorPolicy::Stop {
                    for _ in spec_index + 1..columns.len() {
                        output.push(None);
                    }
                    result.rows.push(output);
                    return result;
                }
            } else {
                output.push(value);
            }
        }

        result.rows.push(output);
    }

    result
}