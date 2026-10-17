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
            DuplicatePolicy::KeepLast => header.iter().rposition(|candidate| candidate == name),
            DuplicatePolicy::Reject | DuplicatePolicy::KeepFirst => {
                header.iter().position(|candidate| candidate == name)
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
    let mut pos = if input.starts_with('\u{feff}') {
        '\u{feff}'.len_utf8()
    } else {
        0
    };
    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut after_quote = false;
    let mut quoted_open = 0usize;
    let mut record_index = 0usize;
    let mut record_started = false;

    while pos < bytes.len() {
        let b = bytes[pos];

        if b == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: pos,
            });
        }

        if !in_quotes && b == b'\r' {
            if bytes.get(pos + 1) == Some(&b'\n') {
                records.push({
                    record.push(std::mem::take(&mut field));
                    record_started = false;
                    std::mem::take(&mut record)
                });
                record_index += 1;
                after_quote = false;
                pos += 2;
                continue;
            } else {
                return Err(CsvError::BareCr {
                    record: record_index,
                    byte: pos,
                });
            }
        }

        if in_quotes {
            if b == quote {
                if bytes.get(pos + 1) == Some(&quote) {
                    field.push(quote as char);
                    pos += 2;
                    continue;
                } else {
                    in_quotes = false;
                    after_quote = true;
                    pos += 1;
                    continue;
                }
            } else if b == b'\n' || b == b'\r' {
                field.push(b as char);
                pos += 1;
                continue;
            } else {
                let ch = input[pos..].chars().next().expect("valid UTF-8");
                field.push(ch);
                pos += ch.len_utf8();
                continue;
            }
        }

        if after_quote {
            if b == delimiter {
                record.push(std::mem::take(&mut field));
                record_started = true;
                after_quote = false;
                pos += 1;
                continue;
            } else if b == b'\n' {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
                record_index += 1;
                record_started = false;
                after_quote = false;
                pos += 1;
                continue;
            } else {
                return Err(CsvError::TrailingByte {
                    record: record_index,
                    byte: pos,
                });
            }
        }

        if b == delimiter {
            record.push(std::mem::take(&mut field));
            record_started = true;
            pos += 1;
            continue;
        } else if b == b'\n' {
            record.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut record));
            record_index += 1;
            record_started = false;
            pos += 1;
            continue;
        } else if b == quote {
            if field.is_empty() {
                in_quotes = true;
                quoted_open = pos;
                record_started = true;
                pos += 1;
                continue;
            } else {
                return Err(CsvError::UnexpectedQuote {
                    record: record_index,
                    byte: pos,
                });
            }
        } else {
            let ch = input[pos..].chars().next().expect("valid UTF-8");
            field.push(ch);
            record_started = true;
            pos += ch.len_utf8();
        }
    }

    if in_quotes {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: quoted_open,
        });
    }

    if record_started || after_quote || !record.is_empty() || !field.is_empty() {
        record.push(field);
        records.push(record);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with_dialect(input, b',', b'"')
}

fn build_table(
    records: Vec<Vec<String>>,
    config: Config,
) -> Result<Table, CsvError> {
    let (header, rows) = if config.headers {
        if records.is_empty() {
            (None, Vec::new())
        } else {
            let mut iter = records.into_iter();
            (iter.next(), iter.collect::<Vec<_>>())
        }
    } else {
        (None, records)
    };

    if let Some(ref header_values) = header {
        if config.duplicates == DuplicatePolicy::Reject {
            for second in 0..header_values.len() {
                if let Some(first) = header_values[..second]
                    .iter()
                    .position(|name| name == &header_values[second])
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

fn integer_syntax(s: &str) -> bool {
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
    if !integer_syntax(s) {
        return Err(CellErrorKind::InvalidInteger);
    }
    s.parse::<i64>()
        .map_err(|_| CellErrorKind::IntegerOverflow)
}

fn decimal_syntax(s: &str) -> bool {
    let bytes = s.as_bytes();
    let start = usize::from(bytes.first() == Some(&b'-'));
    if bytes.len() < start + 4 {
        return false;
    }
    let dot = bytes.len() - 3;
    if bytes[dot] != b'.'
        || !bytes[dot + 1..].iter().all(u8::is_ascii_digit)
        || bytes[dot + 1..].len() != 2
    {
        return false;
    }
    let integral = &bytes[start..dot];
    if integral.is_empty() {
        return false;
    }
    if integral[0] == b'0' {
        if integral.len() != 1 {
            return false;
        }
        if start == 1 && integral[0] == b'0' && bytes[dot + 1..].iter().all(|b| *b == b'0') {
            return false;
        }
        return true;
    }
    (b'1'..=b'9').contains(&integral[0])
        && integral[1..].iter().all(u8::is_ascii_digit)
}

fn parse_decimal(s: &str) -> Result<i64, CellErrorKind> {
    if !decimal_syntax(s) {
        return Err(CellErrorKind::InvalidDecimal);
    }
    let negative = s.starts_with('-');
    let unsigned = if negative { &s[1..] } else { s };
    let dot = unsigned.len() - 3;
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

fn evaluate_cell(
    text: Option<&String>,
    column: Option<usize>,
    spec: &ColumnSpec,
) -> Result<Value, CellErrorKind> {
    let text = text.ok_or(CellErrorKind::MissingCell)?;
    if spec.nullable && text.is_empty() {
        return Ok(Value::Null);
    }
    if text.is_empty() && spec.kind != ColumnType::Text {
        return Err(CellErrorKind::Empty);
    }
    match spec.kind {
        ColumnType::Text => Ok(Value::Text(text.clone())),
        ColumnType::Integer => parse_integer(text).map(Value::Integer),
        ColumnType::Decimal => parse_decimal(text).map(Value::Decimal),
        ColumnType::Boolean => match text.as_str() {
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
        rows: Vec::new(),
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

            let evaluated = match resolved {
                None => Err(CellErrorKind::UnknownColumn),
                Some(index) => evaluate_cell(row.get(index), Some(index), spec),
            };

            match evaluated {
                Ok(value) => output.push(Some(value)),
                Err(kind) => {
                    output.push(None);
                    result.errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column: resolved,
                        kind,
                    });
                    match policy {
                        ErrorPolicy::Collect => {}
                        ErrorPolicy::FirstPerRow | ErrorPolicy::Stop => row_failed = true,
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