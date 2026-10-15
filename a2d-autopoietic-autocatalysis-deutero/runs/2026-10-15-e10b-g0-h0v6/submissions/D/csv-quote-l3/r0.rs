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

fn parse_with_dialect(
    input: &str,
    delimiter: u8,
    quote: u8,
) -> Result<Vec<Vec<String>>, CsvError> {
    let bytes = input.as_bytes();
    let mut i = if input.starts_with('\u{feff}') { 3 } else { 0 };
    if i == bytes.len() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut after_quote = false;
    let mut field_started = false;
    let mut quote_open = 0usize;
    let mut record_started = false;

    while i < bytes.len() {
        let b = bytes[i];

        if b == 0 {
            return Err(CsvError::Nul {
                record: records.len(),
                byte: i,
            });
        }

        if !in_quotes && b == b'\r' && bytes.get(i + 1) != Some(&b'\n') {
            return Err(CsvError::BareCr {
                record: records.len(),
                byte: i,
            });
        }

        if in_quotes {
            if b == quote {
                if bytes.get(i + 1) == Some(&quote) {
                    field.push(quote as char);
                    i += 2;
                    continue;
                }
                in_quotes = false;
                after_quote = true;
                i += 1;
                continue;
            }
            field.push(b as char);
            i += 1;
            continue;
        }

        if after_quote {
            if b == delimiter {
                record.push(std::mem::take(&mut field));
                after_quote = false;
                field_started = false;
                record_started = true;
                i += 1;
                continue;
            }
            if b == b'\n' || b == b'\r' {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
                after_quote = false;
                field_started = false;
                record_started = false;
                i += if b == b'\r' { 2 } else { 1 };
                continue;
            }
            return Err(CsvError::TrailingByte {
                record: records.len(),
                byte: i,
            });
        }

        if b == quote {
            if field_started {
                return Err(CsvError::UnexpectedQuote {
                    record: records.len(),
                    byte: i,
                });
            }
            in_quotes = true;
            field_started = true;
            record_started = true;
            quote_open = i;
            i += 1;
            continue;
        }

        if b == delimiter {
            record.push(std::mem::take(&mut field));
            field_started = false;
            record_started = true;
            i += 1;
            continue;
        }

        if b == b'\n' || b == b'\r' {
            record.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut record));
            field_started = false;
            record_started = false;
            i += if b == b'\r' { 2 } else { 1 };
            continue;
        }

        field.push(b as char);
        field_started = true;
        record_started = true;
        i += 1;
    }

    if in_quotes {
        return Err(CsvError::UnclosedQuote {
            record: records.len(),
            byte: quote_open,
        });
    }

    if record_started || field_started || after_quote || !record.is_empty() {
        record.push(field);
        records.push(record);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with_dialect(input, b',', b'"')
}

fn build_table(input: &str, config: Config) -> Result<Table, CsvError> {
    validate_config(config)?;
    let mut parsed = parse_with_dialect(input, config.delimiter, config.quote)?;

    let mut header = None;
    if config.headers && !parsed.is_empty() {
        header = Some(parsed.remove(0));
    }

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

        for (index, row) in parsed.iter().enumerate() {
            if row.len() != names.len() {
                return Err(CsvError::HeaderWidth {
                    record: index + 1,
                    expected: names.len(),
                    actual: row.len(),
                });
            }
        }
    }

    Ok(Table {
        header,
        rows: parsed,
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
        let result = build_table(&self.input, self.config);
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
    let b = s.as_bytes();
    if b.is_empty() {
        return false;
    }
    let start = if b[0] == b'-' { 1 } else { 0 };
    if start == b.len() {
        return false;
    }
    if b[start] == b'0' {
        return start + 1 == b.len();
    }
    if !(b'1'..=b'9').contains(&b[start]) {
        return false;
    }
    b[start + 1..].iter().all(u8::is_ascii_digit)
}

fn parse_integer(s: &str) -> Result<i64, CellErrorKind> {
    if !valid_integer_syntax(s) {
        return Err(CellErrorKind::InvalidInteger);
    }
    s.parse::<i64>().map_err(|_| CellErrorKind::IntegerOverflow)
}

fn parse_decimal(s: &str) -> Result<i64, CellErrorKind> {
    let b = s.as_bytes();
    let start = usize::from(b.first() == Some(&b'-'));
    let valid = b.len() >= start + 4
        && b[b.len() - 3] == b'.'
        && b[b.len() - 2].is_ascii_digit()
        && b[b.len() - 1].is_ascii_digit()
        && {
            let integral = &s[start..s.len() - 3];
            !integral.is_empty()
                && if integral == "0" {
                    true
                } else {
                    let ib = integral.as_bytes();
                    (b'1'..=b'9').contains(&ib[0])
                        && ib[1..].iter().all(u8::is_ascii_digit)
                }
        }
        && !(b.first() == Some(&b'-') && &s[1..s.len() - 3] == "0");

    if !valid {
        return Err(CellErrorKind::InvalidDecimal);
    }
    let negative = b.first() == Some(&b'-');
    let integral_start = start;
    let integral_end = s.len() - 3;
    let integral = s[integral_start..integral_end]
        .parse::<i128>()
        .map_err(|_| CellErrorKind::DecimalOverflow)?;
    let fraction = ((b[b.len() - 2] - b'0') as i128) * 10
        + (b[b.len() - 1] - b'0') as i128;
    let magnitude = integral
        .checked_mul(100)
        .and_then(|v| v.checked_add(fraction))
        .ok_or(CellErrorKind::DecimalOverflow)?;
    let signed = if negative { -magnitude } else { magnitude };
    i64::try_from(signed).map_err(|_| CellErrorKind::DecimalOverflow)
}

fn convert_cell(value: &str, spec: &ColumnSpec) -> Result<Value, CellErrorKind> {
    if value.is_empty() {
        if spec.nullable {
            return Ok(Value::Null);
        }
        if spec.kind != ColumnType::Text {
            return Err(CellErrorKind::Empty);
        }
    }

    match spec.kind {
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
        let mut stopped = false;

        for (spec_index, spec) in columns.iter().enumerate() {
            if stopped {
                output.push(None);
                continue;
            }

            let resolved = match &spec.column {
                ColumnRef::Index(index) => Some(*index),
                ColumnRef::Name(name) => table.column_index(name),
            };

            let value_result = match resolved {
                None => Err((None, CellErrorKind::UnknownColumn)),
                Some(column) => match row.get(column) {
                    None => Err((Some(column), CellErrorKind::MissingCell)),
                    Some(cell) => convert_cell(cell, spec).map_err(|kind| (Some(column), kind)),
                },
            };

            match value_result {
                Ok(value) => output.push(Some(value)),
                Err((column, kind)) => {
                    output.push(None);
                    result.errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column,
                        kind,
                    });
                    match policy {
                        ErrorPolicy::Collect => {}
                        ErrorPolicy::FirstPerRow | ErrorPolicy::Stop => stopped = true,
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