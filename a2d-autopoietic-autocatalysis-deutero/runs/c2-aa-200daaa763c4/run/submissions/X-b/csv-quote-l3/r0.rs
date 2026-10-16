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
    let mut i = if input.starts_with('\u{feff}') { 3 } else { 0 };
    let mut records = Vec::new();
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut record = 0usize;
    let mut in_quotes = false;
    let mut after_quote = false;
    let mut quote_open = 0usize;
    let mut field_started = false;
    let mut record_started = false;

    while i < bytes.len() {
        let b = bytes[i];

        if b == 0 {
            return Err(CsvError::Nul { record, byte: i });
        }

        if !in_quotes && b == b'\r' && bytes.get(i + 1) != Some(&b'\n') {
            return Err(CsvError::BareCr { record, byte: i });
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
                fields.push(std::mem::take(&mut field));
                field_started = false;
                record_started = true;
                after_quote = false;
                i += 1;
                continue;
            }
            if b == b'\n' || (b == b'\r' && bytes.get(i + 1) == Some(&b'\n')) {
                fields.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut fields));
                record += 1;
                field_started = false;
                record_started = false;
                after_quote = false;
                i += if b == b'\r' { 2 } else { 1 };
                continue;
            }
            return Err(CsvError::TrailingByte { record, byte: i });
        }

        if b == delimiter {
            fields.push(std::mem::take(&mut field));
            field_started = false;
            record_started = true;
            i += 1;
        } else if b == b'\n' || (b == b'\r' && bytes.get(i + 1) == Some(&b'\n')) {
            fields.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut fields));
            record += 1;
            field_started = false;
            record_started = false;
            i += if b == b'\r' { 2 } else { 1 };
        } else if b == quote {
            if field_started {
                return Err(CsvError::UnexpectedQuote { record, byte: i });
            }
            in_quotes = true;
            quote_open = i;
            field_started = true;
            record_started = true;
            i += 1;
        } else {
            let ch = input[i..].chars().next().unwrap();
            field.push(ch);
            field_started = true;
            record_started = true;
            i += ch.len_utf8();
        }
    }

    if in_quotes {
        return Err(CsvError::UnclosedQuote {
            record,
            byte: quote_open,
        });
    }

    if record_started || field_started || !fields.is_empty() || !field.is_empty() || after_quote {
        fields.push(field);
        records.push(fields);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with_dialect(input, b',', b'"')
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
            let mut seen: std::collections::HashMap<&str, usize> =
                std::collections::HashMap::new();
            for (second, name) in names.iter().enumerate() {
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
        let result = parse_table(&self.input, self.config);
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

fn parse_integer(value: &str) -> Result<i64, CellErrorKind> {
    let bytes = value.as_bytes();
    let start = usize::from(bytes.first() == Some(&b'-'));
    let valid = if bytes.len() == start {
        false
    } else if bytes[start] == b'0' {
        bytes.len() == start + 1
    } else {
        (b'1'..=b'9').contains(&bytes[start])
            && bytes[start + 1..].iter().all(u8::is_ascii_digit)
    };

    if !valid {
        return Err(CellErrorKind::InvalidInteger);
    }
    value
        .parse::<i64>()
        .map_err(|_| CellErrorKind::IntegerOverflow)
}

fn parse_decimal(value: &str) -> Result<i64, CellErrorKind> {
    let bytes = value.as_bytes();
    let negative = bytes.first() == Some(&b'-');
    let start = usize::from(negative);
    let dot = value.find('.');

    let valid = if let Some(dot) = dot {
        let integral = &bytes[start..dot];
        let fractional = &bytes[dot + 1..];
        let integral_valid = if integral == b"0" {
            true
        } else {
            !integral.is_empty()
                && (b'1'..=b'9').contains(&integral[0])
                && integral[1..].iter().all(u8::is_ascii_digit)
        };
        integral_valid
            && fractional.len() == 2
            && fractional.iter().all(u8::is_ascii_digit)
            && (!negative || integral != b"0" || fractional != b"00")
            && value.bytes().filter(|&b| b == b'.').count() == 1
    } else {
        false
    };

    if !valid {
        return Err(CellErrorKind::InvalidDecimal);
    }

    let negative = value.starts_with('-');
    let unsigned = if negative { &value[1..] } else { value };
    let dot = unsigned.find('.').unwrap();
    let digits = format!("{}{}", &unsigned[..dot], &unsigned[dot + 1..]);
    let magnitude = digits
        .parse::<i128>()
        .map_err(|_| CellErrorKind::DecimalOverflow)?;
    let signed = if negative { -magnitude } else { magnitude };
    i64::try_from(signed).map_err(|_| CellErrorKind::DecimalOverflow)
}

fn convert_cell(
    value: &str,
    kind: ColumnType,
    nullable: bool,
) -> Result<Value, CellErrorKind> {
    if value.is_empty() && nullable {
        return Ok(Value::Null);
    }
    if value.is_empty() && kind != ColumnType::Text {
        return Err(CellErrorKind::Empty);
    }

    match kind {
        ColumnType::Text => Ok(Value::Text(value.to_owned())),
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
        rows: Vec::new(),
        errors: Vec::new(),
    };

    'rows: for (row_index, row) in table.rows.iter().enumerate() {
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

            let Some(index) = column_index else {
                result.errors.push(CellError {
                    row: row_index,
                    spec: spec_index,
                    column: None,
                    kind: CellErrorKind::UnknownColumn,
                });
                output.push(None);
                row_failed = true;
                if policy == ErrorPolicy::Stop {
                    result.rows.push(output);
                    break 'rows;
                }
                if policy == ErrorPolicy::FirstPerRow {
                    continue;
                }
                continue;
            };

            let Some(cell) = row.get(index) else {
                result.errors.push(CellError {
                    row: row_index,
                    spec: spec_index,
                    column: Some(index),
                    kind: CellErrorKind::MissingCell,
                });
                output.push(None);
                row_failed = true;
                if policy == ErrorPolicy::Stop {
                    result.rows.push(output);
                    break 'rows;
                }
                if policy == ErrorPolicy::FirstPerRow {
                    continue;
                }
                continue;
            };

            match convert_cell(cell, spec.kind, spec.nullable) {
                Ok(value) => output.push(Some(value)),
                Err(kind) => {
                    result.errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column: Some(index),
                        kind,
                    });
                    output.push(None);
                    row_failed = true;
                    if policy == ErrorPolicy::Stop {
                        result.rows.push(output);
                        break 'rows;
                    }
                    if policy == ErrorPolicy::FirstPerRow {
                        continue;
                    }
                }
            }
        }

        result.rows.push(output);
    }

    result
}