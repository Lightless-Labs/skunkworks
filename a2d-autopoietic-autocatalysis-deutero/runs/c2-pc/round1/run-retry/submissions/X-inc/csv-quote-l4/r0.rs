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
            DuplicatePolicy::KeepLast => header.iter().rposition(|value| value == name),
            DuplicatePolicy::Reject | DuplicatePolicy::KeepFirst => {
                header.iter().position(|value| value == name)
            }
        }
    }
}

fn validate_dialect(delimiter: u8, quote: u8) -> Result<(), ConfigPart> {
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
    let mut pos = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        3
    } else {
        0
    };

    if pos == bytes.len() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut after_quote = false;
    let mut field_started = false;
    let mut record = 0usize;
    let mut quote_start = 0usize;

    while pos < bytes.len() {
        let byte = bytes[pos];

        if byte == 0 {
            return Err(CsvError::Nul { record, byte: pos });
        }

        if in_quotes {
            if byte == quote {
                if pos + 1 < bytes.len() && bytes[pos + 1] == quote {
                    field.push(quote as char);
                    pos += 2;
                } else {
                    in_quotes = false;
                    after_quote = true;
                    pos += 1;
                }
                continue;
            }

            if byte < 0x80 {
                field.push(byte as char);
                pos += 1;
            } else {
                let ch = input[pos..].chars().next().unwrap();
                field.push(ch);
                pos += ch.len_utf8();
            }
            continue;
        }

        if byte == b'\r' && (pos + 1 >= bytes.len() || bytes[pos + 1] != b'\n') {
            return Err(CsvError::BareCr { record, byte: pos });
        }

        if after_quote {
            if byte == delimiter {
                row.push(std::mem::take(&mut field));
                field_started = false;
                after_quote = false;
                pos += 1;
                continue;
            }
            if byte == b'\n' || byte == b'\r' {
                row.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut row));
                field_started = false;
                after_quote = false;
                record += 1;
                pos += if byte == b'\r' { 2 } else { 1 };
                continue;
            }
            return Err(CsvError::TrailingByte { record, byte: pos });
        }

        if byte == delimiter {
            row.push(std::mem::take(&mut field));
            field_started = false;
            pos += 1;
            continue;
        }

        if byte == b'\n' || byte == b'\r' {
            row.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut row));
            field_started = false;
            record += 1;
            pos += if byte == b'\r' { 2 } else { 1 };
            continue;
        }

        if byte == quote {
            if field_started {
                return Err(CsvError::UnexpectedQuote { record, byte: pos });
            }
            in_quotes = true;
            quote_start = pos;
            field_started = true;
            pos += 1;
            continue;
        }

        field_started = true;
        if byte < 0x80 {
            field.push(byte as char);
            pos += 1;
        } else {
            let ch = input[pos..].chars().next().unwrap();
            field.push(ch);
            pos += ch.len_utf8();
        }
    }

    if in_quotes {
        return Err(CsvError::UnclosedQuote {
            record,
            byte: quote_start,
        });
    }

    if !row.is_empty() || !field.is_empty() || field_started || after_quote {
        row.push(field);
        records.push(row);
    }

    Ok(records)
}

pub fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, CsvError> {
    parse_with_dialect(input, b',', b'"')
}

fn duplicate_header(header: &[String]) -> Option<CsvError> {
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

fn table_from_records(records: Vec<Vec<String>>, config: Config) -> Result<Table, CsvError> {
    if !config.headers {
        return Ok(Table {
            header: None,
            rows: records,
            duplicates: config.duplicates,
        });
    }

    let mut records = records.into_iter();
    let Some(header) = records.next() else {
        return Ok(Table {
            header: None,
            rows: Vec::new(),
            duplicates: config.duplicates,
        });
    };

    if config.duplicates == DuplicatePolicy::Reject {
        if let Some(error) = duplicate_header(&header) {
            return Err(error);
        }
    }

    let rows: Vec<Vec<String>> = records.collect();
    for (index, row) in rows.iter().enumerate() {
        if row.len() != header.len() {
            return Err(CsvError::HeaderWidth {
                record: index + 1,
                expected: header.len(),
                actual: row.len(),
            });
        }
    }

    Ok(Table {
        header: Some(header),
        rows,
        duplicates: config.duplicates,
    })
}

pub struct StreamReader {
    config: Config,
    input: String,
    finished: Option<Result<Table, CsvError>>,
}

impl StreamReader {
    pub fn new(config: Config) -> Result<Self, CsvError> {
        validate_dialect(config.delimiter, config.quote)
            .map_err(|component| CsvError::InvalidConfig { component })?;
        Ok(Self {
            config,
            input: String::new(),
            finished: None,
        })
    }

    pub fn feed(&mut self, chunk: &str) -> Result<(), CsvError> {
        if self.finished.is_some() {
            return Err(CsvError::ReaderFinished);
        }
        self.input.push_str(chunk);
        Ok(())
    }

    pub fn finish(&mut self) -> Result<Table, CsvError> {
        if let Some(result) = &self.finished {
            return result.clone();
        }

        let result = parse_with_dialect(&self.input, self.config.delimiter, self.config.quote)
            .and_then(|records| table_from_records(records, self.config));
        self.finished = Some(result.clone());
        result
    }

    pub fn reset(&mut self) {
        self.input.clear();
        self.finished = None;
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

fn integer_syntax(value: &str) -> bool {
    let bytes = value.as_bytes();
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

fn decimal_syntax(value: &str) -> bool {
    let bytes = value.as_bytes();
    let body = if bytes.first() == Some(&b'-') {
        &bytes[1..]
    } else {
        bytes
    };
    let Some(dot) = body.iter().position(|&b| b == b'.') else {
        return false;
    };
    if body[dot + 1..].len() != 2 {
        return false;
    }
    let integral = &body[..dot];
    let fraction = &body[dot + 1..];
    if !fraction.iter().all(u8::is_ascii_digit) || integral.is_empty() {
        return false;
    }
    let integral_ok = integral == b"0"
        || (integral[0].is_ascii_digit()
            && integral[0] != b'0'
            && integral.iter().all(u8::is_ascii_digit));
    if !integral_ok {
        return false;
    }
    !(bytes.first() == Some(&b'-') && integral == b"0" && fraction == b"00")
}

fn evaluate_cell(value: Option<&String>, spec: &ColumnSpec) -> Result<Value, CellErrorKind> {
    let Some(value) = value else {
        return Err(CellErrorKind::MissingCell);
    };

    if value.is_empty() {
        if spec.nullable {
            return Ok(Value::Null);
        }
        if spec.kind != ColumnType::Text {
            return Err(CellErrorKind::Empty);
        }
    }

    match spec.kind {
        ColumnType::Text => Ok(Value::Text(value.clone())),
        ColumnType::Integer => {
            if !integer_syntax(value) {
                return Err(CellErrorKind::InvalidInteger);
            }
            value
                .parse::<i64>()
                .map(Value::Integer)
                .map_err(|_| CellErrorKind::IntegerOverflow)
        }
        ColumnType::Decimal => {
            if !decimal_syntax(value) {
                return Err(CellErrorKind::InvalidDecimal);
            }
            let negative = value.starts_with('-');
            let body = if negative { &value[1..] } else { value.as_str() };
            let mut parts = body.split('.');
            let whole = parts.next().unwrap();
            let fraction = parts.next().unwrap();
            let magnitude = format!("{whole}{fraction}");
            if negative {
                if magnitude == "9223372036854775808" {
                    Ok(Value::Decimal(i64::MIN))
                } else {
                    magnitude
                        .parse::<i64>()
                        .map(|n| Value::Decimal(-n))
                        .map_err(|_| CellErrorKind::DecimalOverflow)
                }
            } else {
                magnitude
                    .parse::<i64>()
                    .map(Value::Decimal)
                    .map_err(|_| CellErrorKind::DecimalOverflow)
            }
        }
        ColumnType::Boolean => match value.as_str() {
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

        for (spec_index, spec) in columns.iter().enumerate() {
            let resolved = match &spec.column {
                ColumnRef::Index(index) => Some(*index),
                ColumnRef::Name(name) => table.column_index(name),
            };

            let (column_index, value) = match resolved {
                None => (None, Err(CellErrorKind::UnknownColumn)),
                Some(index) => (Some(index), evaluate_cell(row.get(index), spec)),
            };

            match value {
                Ok(value) => output.push(Some(value)),
                Err(kind) => {
                    output.push(None);
                    result.errors.push(CellError {
                        row: row_index,
                        spec: spec_index,
                        column: column_index,
                        kind,
                    });

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
        }

        output.resize(columns.len(), None);
        result.rows.push(output);
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
    InvalidConfig { component: ConfigPart },
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
    ZeroColumns { record: usize },
    DuplicateHeader {
        name: String,
        first: usize,
        second: usize,
    },
}

fn all_records(table: &Table) -> Vec<&Vec<String>> {
    let mut records = Vec::with_capacity(table.rows.len() + usize::from(table.header.is_some()));
    if let Some(header) = &table.header {
        records.push(header);
    }
    records.extend(table.rows.iter());
    records
}

fn compare_vectors(left: &[String], right: &[String]) -> std::cmp::Ordering {
    for (a, b) in left.iter().zip(right.iter()) {
        let ordering = a.as_bytes().cmp(b.as_bytes());
        if ordering != std::cmp::Ordering::Equal {
            return ordering;
        }
    }
    left.len().cmp(&right.len())
}

fn serialize_field(
    output: &mut String,
    value: &str,
    delimiter: u8,
    quote: u8,
    first_record_first_field: bool,
    sole_empty: bool,
) {
    let bytes = value.as_bytes();
    let needs_quote = bytes.contains(&delimiter)
        || bytes.contains(&quote)
        || bytes.contains(&b'\r')
        || bytes.contains(&b'\n')
        || bytes.first().is_some_and(|b| *b == b' ' || *b == b'\t')
        || bytes.last().is_some_and(|b| *b == b' ' || *b == b'\t')
        || (first_record_first_field && bytes.starts_with(&[0xef, 0xbb, 0xbf]))
        || (sole_empty && value.is_empty());

    if !needs_quote {
        output.push_str(value);
        return;
    }

    output.push(quote as char);
    let mut start = 0;
    for (index, &byte) in bytes.iter().enumerate() {
        if byte == quote {
            output.push_str(&value[start..index]);
            output.push(quote as char);
            output.push(quote as char);
            start = index + 1;
        }
    }
    output.push_str(&value[start..]);
    output.push(quote as char);
}

pub fn write_csv(table: &Table, config: WriteConfig) -> Result<String, WriteError> {
    validate_dialect(config.delimiter, config.quote)
        .map_err(|component| WriteError::InvalidConfig { component })?;

    let original_records = all_records(table);

    for (record_index, record) in original_records.iter().enumerate() {
        for (column_index, field) in record.iter().enumerate() {
            if let Some(byte) = field.as_bytes().iter().position(|&b| b == 0) {
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

    for (record_index, record) in original_records.iter().enumerate() {
        if record.is_empty() {
            return Err(WriteError::ZeroColumns {
                record: record_index,
            });
        }
    }

    if config.mode == WriteMode::Canonical
        && table.header.is_some()
        && table.duplicates == DuplicatePolicy::Reject
    {
        if let Some(CsvError::DuplicateHeader {
            name,
            first,
            second,
        }) = duplicate_header(table.header.as_ref().unwrap())
        {
            return Err(WriteError::DuplicateHeader {
                name,
                first,
                second,
            });
        }
    }

    let (header, mut rows): (Option<Vec<String>>, Vec<Vec<String>>) =
        match config.mode {
            WriteMode::Preserve => (table.header.clone(), table.rows.clone()),
            WriteMode::Canonical => {
                if let Some(original_header) = &table.header {
                    let mut chosen: HashMap<&str, usize> = HashMap::new();
                    for (index, name) in original_header.iter().enumerate() {
                        chosen
                            .entry(name.as_str())
                            .and_modify(|old| {
                                if table.duplicates == DuplicatePolicy::KeepLast {
                                    *old = index;
                                }
                            })
                            .or_insert(index);
                    }
                    let mut retained: Vec<(String, usize)> = chosen
                        .into_iter()
                        .map(|(name, index)| (name.to_string(), index))
                        .collect();
                    retained.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
                    let new_header: Vec<String> =
                        retained.iter().map(|(name, _)| name.clone()).collect();
                    let mut new_rows = Vec::with_capacity(table.rows.len());
                    for row in &table.rows {
                        new_rows.push(
                            retained
                                .iter()
                                .map(|(_, index)| row[*index].clone())
                                .collect(),
                        );
                    }
                    (Some(new_header), new_rows)
                } else {
                    (None, table.rows.clone())
                }
            }
        };

    if config.mode == WriteMode::Canonical {
        rows.sort_by(|a, b| compare_vectors(a, b));
    }

    let mut records: Vec<Vec<String>> = Vec::with_capacity(rows.len() + 1);
    if let Some(header) = header {
        records.push(header);
    }
    records.extend(rows);

    if records.is_empty() {
        return Ok(String::new());
    }

    let canonical = config.mode == WriteMode::Canonical;
    let ending = if canonical {
        "\n"
    } else {
        match config.line_ending {
            LineEnding::Lf => "\n",
            LineEnding::CrLf => "\r\n",
        }
    };

    let mut output = String::new();
    for (record_index, record) in records.iter().enumerate() {
        for (column_index, field) in record.iter().enumerate() {
            if column_index > 0 {
                output.push(config.delimiter as char);
            }
            serialize_field(
                &mut output,
                field,
                config.delimiter,
                config.quote,
                record_index == 0 && column_index == 0,
                record.len() == 1 && field.is_empty(),
            );
        }

        let has_next = record_index + 1 < records.len();
        if has_next || canonical || config.final_terminator {
            output.push_str(ending);
        }
    }

    Ok(output)
}