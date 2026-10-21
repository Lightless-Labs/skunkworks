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
            DuplicatePolicy::KeepLast => header.iter().rposition(|cell| cell == name),
            DuplicatePolicy::Reject | DuplicatePolicy::KeepFirst => {
                header.iter().position(|cell| cell == name)
            }
        }
    }
}

fn validate_config(delimiter: u8, quote: u8) -> Result<(), ConfigPart> {
    if !(0x21..=0x7e).contains(&delimiter) {
        return Err(ConfigPart::Delimiter);
    }
    if !(0x21..=0x7e).contains(&quote) {
        return Err(ConfigPart::Quote);
    }
    if delimiter == quote {
        return Err(ConfigPart::Quote);
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
    let mut record = Vec::new();
    let mut field = Vec::new();
    let mut field_started = false;
    let mut in_quotes = false;
    let mut after_quote = false;
    let mut quote_open = 0usize;
    let mut record_index = 0usize;

    while i < bytes.len() {
        let b = bytes[i];

        if b == 0 {
            return Err(CsvError::Nul {
                record: record_index,
                byte: i,
            });
        }

        if !in_quotes && b == b'\r' && bytes.get(i + 1) != Some(&b'\n') {
            return Err(CsvError::BareCr {
                record: record_index,
                byte: i,
            });
        }

        if in_quotes {
            if b == quote {
                if bytes.get(i + 1) == Some(&quote) {
                    field.push(quote);
                    i += 2;
                } else {
                    in_quotes = false;
                    after_quote = true;
                    i += 1;
                }
            } else {
                field.push(b);
                i += 1;
            }
            continue;
        }

        if after_quote {
            if b == delimiter {
                record.push(String::from_utf8(field).expect("valid UTF-8 field"));
                field = Vec::new();
                field_started = false;
                after_quote = false;
                i += 1;
            } else if b == b'\n' || b == b'\r' {
                record.push(String::from_utf8(field).expect("valid UTF-8 field"));
                records.push(record);
                record = Vec::new();
                field = Vec::new();
                field_started = false;
                after_quote = false;
                record_index += 1;
                i += if b == b'\r' { 2 } else { 1 };
            } else {
                return Err(CsvError::TrailingByte {
                    record: record_index,
                    byte: i,
                });
            }
            continue;
        }

        if b == delimiter {
            record.push(String::from_utf8(field).expect("valid UTF-8 field"));
            field = Vec::new();
            field_started = false;
            i += 1;
        } else if b == b'\n' || b == b'\r' {
            record.push(String::from_utf8(field).expect("valid UTF-8 field"));
            records.push(record);
            record = Vec::new();
            field = Vec::new();
            field_started = false;
            record_index += 1;
            i += if b == b'\r' { 2 } else { 1 };
        } else if b == quote {
            if field_started {
                return Err(CsvError::UnexpectedQuote {
                    record: record_index,
                    byte: i,
                });
            }
            in_quotes = true;
            quote_open = i;
            field_started = true;
            i += 1;
        } else {
            field.push(b);
            field_started = true;
            i += 1;
        }
    }

    if in_quotes {
        return Err(CsvError::UnclosedQuote {
            record: record_index,
            byte: quote_open,
        });
    }

    if !record.is_empty() || field_started || !field.is_empty() {
        record.push(String::from_utf8(field).expect("valid UTF-8 field"));
        records.push(record);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with_dialect(input, b',', b'"')
}

fn duplicate_header_error(header: &[String]) -> Option<CsvError> {
    let mut seen: HashMap<&str, usize> = HashMap::new();
    for (second, name) in header.iter().enumerate() {
        if let Some(&first) = seen.get(name.as_str()) {
            return Some(CsvError::DuplicateHeader {
                name: name.clone(),
                first,
                second,
            });
        }
        seen.insert(name, second);
    }
    None
}

fn table_from_records(
    mut records: Vec<Vec<String>>,
    config: Config,
) -> Result<Table, CsvError> {
    let header = if config.headers && !records.is_empty() {
        Some(records.remove(0))
    } else {
        None
    };

    if let Some(ref header) = header {
        if config.duplicates == DuplicatePolicy::Reject {
            if let Some(error) = duplicate_header_error(header) {
                return Err(error);
            }
        }
        for (index, row) in records.iter().enumerate() {
            if row.len() != header.len() {
                return Err(CsvError::HeaderWidth {
                    record: index + 1,
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
    cached: Option<Result<Table, CsvError>>,
}

impl StreamReader {
    pub fn new(config: Config) -> Result<Self, CsvError> {
        validate_config(config.delimiter, config.quote).map_err(|component| {
            CsvError::InvalidConfig { component }
        })?;
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
            .and_then(|records| table_from_records(records, self.config));
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
    let digits = if bytes[0] == b'-' { &bytes[1..] } else { bytes };
    if digits.is_empty() {
        return false;
    }
    if digits == b"0" {
        return bytes[0] != b'-';
    }
    digits[0].is_ascii_digit()
        && digits[0] != b'0'
        && digits.iter().all(u8::is_ascii_digit)
}

fn parse_integer(s: &str) -> Result<i64, CellErrorKind> {
    if !integer_syntax(s) {
        return Err(CellErrorKind::InvalidInteger);
    }
    s.parse::<i64>().map_err(|_| CellErrorKind::IntegerOverflow)
}

fn decimal_syntax(s: &str) -> bool {
    let bytes = s.as_bytes();
    let body = if bytes.first() == Some(&b'-') {
        &bytes[1..]
    } else {
        bytes
    };
    let Some(dot) = body.iter().position(|&b| b == b'.') else {
        return false;
    };
    if body.len() != dot + 3 || !body[dot + 1..].iter().all(u8::is_ascii_digit) {
        return false;
    }
    let integral = &body[..dot];
    if integral.is_empty() || !integral.iter().all(u8::is_ascii_digit) {
        return false;
    }
    if integral.len() > 1 && integral[0] == b'0' {
        return false;
    }
    if bytes.first() == Some(&b'-') && integral == b"0" && body[dot + 1..] == *b"00" {
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
    let dot = unsigned.find('.').expect("validated decimal");
    let whole = unsigned[..dot]
        .parse::<i128>()
        .map_err(|_| CellErrorKind::DecimalOverflow)?;
    let frac = unsigned[dot + 1..]
        .parse::<i128>()
        .expect("validated fractional digits");
    let magnitude = whole
        .checked_mul(100)
        .and_then(|v| v.checked_add(frac))
        .ok_or(CellErrorKind::DecimalOverflow)?;
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
        let mut out = vec![None; columns.len()];
        let mut row_failed = false;

        for (spec_index, spec) in columns.iter().enumerate() {
            let column_index = match &spec.column {
                ColumnRef::Index(index) => Some(*index),
                ColumnRef::Name(name) => table.column_index(name),
            };

            let Some(column_index) = column_index else {
                result.errors.push(CellError {
                    row: row_index,
                    spec: spec_index,
                    column: None,
                    kind: CellErrorKind::UnknownColumn,
                });
                row_failed = true;
                if policy != ErrorPolicy::Collect {
                    break;
                }
                continue;
            };

            let Some(cell) = row.get(column_index) else {
                result.errors.push(CellError {
                    row: row_index,
                    spec: spec_index,
                    column: Some(column_index),
                    kind: CellErrorKind::MissingCell,
                });
                row_failed = true;
                if policy != ErrorPolicy::Collect {
                    break;
                }
                continue;
            };

            if cell.is_empty() {
                if spec.nullable {
                    out[spec_index] = Some(Value::Null);
                    continue;
                }
                if spec.kind != ColumnType::Text {
                    result.errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column: Some(column_index),
                        kind: CellErrorKind::Empty,
                    });
                    row_failed = true;
                    if policy != ErrorPolicy::Collect {
                        break;
                    }
                    continue;
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
                Ok(value) => out[spec_index] = Some(value),
                Err(kind) => {
                    result.errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column: Some(column_index),
                        kind,
                    });
                    row_failed = true;
                    if policy != ErrorPolicy::Collect {
                        break;
                    }
                }
            }
        }

        result.rows.push(out);

        if policy == ErrorPolicy::Stop && row_failed {
            break;
        }
    }

    result
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    CrLf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteMode {
    Preserve,
    Canonical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteConfig {
    pub delimiter: u8,
    pub quote: u8,
    pub line_ending: LineEnding,
    pub final_terminator: bool,
    pub mode: WriteMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteError {
    InvalidConfig {
        component: ConfigPart,
    },
    Nul {
        record: usize,
        column: usize,
        byte: usize,
    },
    Width {
        record: usize,
        expected: usize,
        actual: usize,
    },
    ZeroColumns {
        record: usize,
    },
    DuplicateHeader {
        name: String,
        first: usize,
        second: usize,
    },
}

fn output_records(table: &Table, config: WriteConfig) -> Result<Vec<Vec<String>>, WriteError> {
    let mut records = Vec::new();
    if let Some(header) = &table.header {
        records.push(header.clone());
    }
    records.extend(table.rows.iter().cloned());

    for (record_index, record) in records.iter().enumerate() {
        for (column_index, cell) in record.iter().enumerate() {
            if let Some(byte) = cell.as_bytes().iter().position(|&b| b == 0) {
                return Err(WriteError::Nul {
                    record: record_index,
                    column: column_index,
                    byte,
                });
            }
        }
    }

    if let Some(header) = &table.header {
        for (row_index, row) in table.rows.iter().enumerate() {
            if row.len() != header.len() {
                return Err(WriteError::Width {
                    record: row_index + 1,
                    expected: header.len(),
                    actual: row.len(),
                });
            }
        }
    }

    for (record_index, record) in records.iter().enumerate() {
        if record.is_empty() {
            return Err(WriteError::ZeroColumns {
                record: record_index,
            });
        }
    }

    if config.mode == WriteMode::Canonical {
        if let Some(header) = &table.header {
            if table.duplicates == DuplicatePolicy::Reject {
                if let Some(CsvError::DuplicateHeader { name, first, second }) =
                    duplicate_header_error(header)
                {
                    return Err(WriteError::DuplicateHeader {
                        name,
                        first,
                        second,
                    });
                }
            }

            let mut chosen: HashMap<&str, usize> = HashMap::new();
            for (index, name) in header.iter().enumerate() {
                match table.duplicates {
                    DuplicatePolicy::KeepFirst | DuplicatePolicy::Reject => {
                        chosen.entry(name.as_str()).or_insert(index);
                    }
                    DuplicatePolicy::KeepLast => {
                        chosen.insert(name.as_str(), index);
                    }
                }
            }
            let mut indexes: Vec<usize> = chosen.values().copied().collect();
            indexes.sort_by(|&a, &b| header[a].as_bytes().cmp(header[b].as_bytes()));

            let projected_header: Vec<String> =
                indexes.iter().map(|&i| header[i].clone()).collect();
            let mut projected_rows: Vec<Vec<String>> = table
                .rows
                .iter()
                .map(|row| indexes.iter().map(|&i| row[i].clone()).collect())
                .collect();
            projected_rows.sort_by(|a, b| compare_rows(a, b));

            let mut canonical = vec![projected_header];
            canonical.extend(projected_rows);
            Ok(canonical)
        } else {
            let mut rows = table.rows.clone();
            rows.sort_by(|a, b| compare_rows(a, b));
            Ok(rows)
        }
    } else {
        Ok(records)
    }
}

fn compare_rows(a: &[String], b: &[String]) -> std::cmp::Ordering {
    for (left, right) in a.iter().zip(b.iter()) {
        let order = left.as_bytes().cmp(right.as_bytes());
        if order != std::cmp::Ordering::Equal {
            return order;
        }
    }
    a.len().cmp(&b.len())
}

fn encode_field(
    field: &str,
    delimiter: u8,
    quote: u8,
    first_field_first_record: bool,
    sole_empty_field: bool,
) -> String {
    let bytes = field.as_bytes();
    let must_quote = bytes.contains(&delimiter)
        || bytes.contains(&quote)
        || bytes.contains(&b'\r')
        || bytes.contains(&b'\n')
        || bytes.first().is_some_and(|&b| b == b' ' || b == b'\t')
        || bytes.last().is_some_and(|&b| b == b' ' || b == b'\t')
        || (first_field_first_record && field.starts_with('\u{feff}'))
        || (sole_empty_field && field.is_empty());

    if !must_quote {
        return field.to_string();
    }

    let mut encoded = String::with_capacity(field.len() + 2);
    encoded.push(quote as char);
    for ch in field.chars() {
        if ch as u32 == quote as u32 {
            encoded.push(quote as char);
            encoded.push(quote as char);
        } else {
            encoded.push(ch);
        }
    }
    encoded.push(quote as char);
    encoded
}

pub fn write_csv(table: &Table, config: WriteConfig) -> Result<String, WriteError> {
    validate_config(config.delimiter, config.quote).map_err(|component| {
        WriteError::InvalidConfig { component }
    })?;

    let records = output_records(table, config)?;
    if records.is_empty() {
        return Ok(String::new());
    }

    let canonical = config.mode == WriteMode::Canonical;
    let terminator = if canonical {
        "\n"
    } else {
        match config.line_ending {
            LineEnding::Lf => "\n",
            LineEnding::CrLf => "\r\n",
        }
    };

    let mut output = String::new();
    for (record_index, record) in records.iter().enumerate() {
        if record_index > 0 {
            output.push_str(terminator);
        }
        let sole_empty = record.len() == 1 && record[0].is_empty();
        for (column_index, field) in record.iter().enumerate() {
            if column_index > 0 {
                output.push(config.delimiter as char);
            }
            output.push_str(&encode_field(
                field,
                config.delimiter,
                config.quote,
                record_index == 0 && column_index == 0,
                sole_empty,
            ));
        }
    }

    if canonical || config.final_terminator {
        output.push_str(terminator);
    }

    Ok(output)
}